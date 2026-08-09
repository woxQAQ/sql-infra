//! Top-level and object-specific `DROP` statement parsing.
//!
//! Shared behavior and missing-object options are combined with identity grammars
//! for functions, operators, mappings, and other special object families.

use super::*;

impl Parser {
    pub(super) fn parse_drop(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Drop)?;
        self.record_completion_tokens(&[
            TokenKind::Access,
            TokenKind::Database,
            TokenKind::Cast,
            TokenKind::Transform,
            TokenKind::Operator,
            TokenKind::User,
            TokenKind::Role,
            TokenKind::GroupP,
            TokenKind::Owned,
            TokenKind::Tablespace,
            TokenKind::Subscription,
            TokenKind::Table,
            TokenKind::Sequence,
            TokenKind::View,
            TokenKind::Materialized,
            TokenKind::Index,
            TokenKind::Schema,
            TokenKind::TypeP,
            TokenKind::DomainP,
            TokenKind::Function,
            TokenKind::Procedure,
            TokenKind::Routine,
            TokenKind::Aggregate,
            TokenKind::Collation,
            TokenKind::Extension,
            TokenKind::Event,
            TokenKind::Foreign,
            TokenKind::Language,
            TokenKind::Policy,
            TokenKind::Property,
            TokenKind::Rule,
            TokenKind::Server,
            TokenKind::Statistics,
            TokenKind::TextP,
            TokenKind::Trigger,
            TokenKind::Publication,
        ]);
        match self.peek_kind() {
            TokenKind::Database => self.parse_drop_database(),
            TokenKind::Cast => self.parse_drop_special(ObjectType::Cast),
            TokenKind::Transform => self.parse_drop_special(ObjectType::Transform),
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                self.parse_drop_operator_family(ObjectType::Opclass)
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.parse_drop_operator_family(ObjectType::Opfamily)
            }
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_drop_user_mapping()
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_drop_role(),
            TokenKind::Owned => self.parse_drop_owned(),
            TokenKind::Tablespace => self.parse_drop_tablespace(),
            TokenKind::Subscription => self.parse_drop_subscription(),
            _ => self.parse_drop_stmt(),
        }
    }

    // PostgreSQL 18 Synopsis subset — generic DROP
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-commands.html
    // - https://www.postgresql.org/docs/18/sql-droptable.html
    // - https://www.postgresql.org/docs/18/sql-dropfunction.html
    // - https://www.postgresql.org/docs/18/sql-dropoperator.html
    //
    // Normalized across the object-specific DROP command pages:
    // DROP object_type [ CONCURRENTLY ] [ IF EXISTS ] object_identity [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // object_identity is parsed according to object_type:
    //     name
    //     name [ ( [ argument [, ...] ] ) ]
    //     aggregate_name ( aggregate_signature )
    //     operator_name ( { left_type | NONE }, { right_type | NONE } )
    //     { policy | rule | trigger }_name ON relation_name
    fn parse_drop_stmt(&mut self) -> PResult<Node> {
        let remove_type = self
            .consume_object_type()
            .ok_or_else(|| self.error_here("DROP requires an object type"))?;
        let concurrent = remove_type == ObjectType::Index && self.consume(TokenKind::Concurrently);
        let missing_ok = self.consume_if_exists()?;
        let stops = [
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        let object_slot = completion::object_type_slot(remove_type);
        self.record_completion_slot(object_slot);
        if matches!(
            remove_type,
            ObjectType::Policy | ObjectType::Rule | ObjectType::Trigger
        ) {
            self.record_completion_qualified_name_slot(object_slot, &[TokenKind::On]);
        } else {
            self.record_completion_qualified_name_slot(object_slot, &stops);
        }
        let objects = match remove_type {
            ObjectType::Policy | ObjectType::Rule | ObjectType::Trigger => {
                let object_name = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("DROP requires an object name"))?;
                self.expect(TokenKind::On)?;
                self.record_completion_slot(completion::GrammarSlot::Table);
                let mut parts = self.consume_name_parts();
                if parts.is_empty() {
                    return Err(self.error_here("ON requires an object name"));
                }
                parts.push(object_name);
                vec![name_list_node(
                    parts.into_iter().map(make_string_node).collect(),
                )]
            }
            ObjectType::Operator => self.parse_operator_with_args_list_until(&stops)?,
            ObjectType::Aggregate => self.parse_aggregate_with_args_list_until(&stops)?,
            ObjectType::Function | ObjectType::Procedure | ObjectType::Routine => {
                self.parse_object_with_args_list_until_with_slot(&stops, object_slot)?
            }
            ObjectType::Type | ObjectType::Domain => {
                parse_type_node_list(self.take_until_top_level(&stops))?
            }
            ObjectType::AccessMethod
            | ObjectType::EventTrigger
            | ObjectType::Extension
            | ObjectType::Fdw
            | ObjectType::Language
            | ObjectType::Publication
            | ObjectType::Schema
            | ObjectType::ForeignServer => {
                self.parse_simple_name_list_until(&stops, object_slot)?
            }
            _ => self.parse_any_name_list_until_with_slot(&stops, object_slot)?,
        };
        if objects.is_empty() {
            return Err(self.error_here("DROP requires at least one object name"));
        }
        let behavior = self.parse_drop_behavior();
        Ok(Node::DropStmt(DropStmt {
            objects,
            remove_type,
            behavior,
            missing_ok,
            concurrent,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropcast.html
    // DROP CAST [ IF EXISTS ] (source_type AS target_type) [ CASCADE | RESTRICT ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-droptransform.html
    // DROP TRANSFORM [ IF EXISTS ] FOR type_name LANGUAGE lang_name [ CASCADE | RESTRICT ]
    fn parse_drop_special(&mut self, remove_type: ObjectType) -> PResult<Node> {
        self.advance();
        let missing_ok = self.consume_if_exists()?;
        let object = if remove_type == ObjectType::Cast {
            self.expect(TokenKind::Char('('))?;
            let source = self
                .parse_type_name_until(&[TokenKind::As])
                .ok_or_else(|| self.error_here("DROP CAST requires a source type"))?;
            self.expect(TokenKind::As)?;
            let target = self
                .parse_type_name_until(&[TokenKind::Char(')')])
                .ok_or_else(|| self.error_here("DROP CAST requires a target type"))?;
            self.expect(TokenKind::Char(')'))?;
            name_list_node(vec![Node::TypeName(source), Node::TypeName(target)])
        } else {
            self.expect(TokenKind::For)?;
            let type_name = self
                .parse_type_name_until(&[TokenKind::Language])
                .ok_or_else(|| self.error_here("DROP TRANSFORM FOR requires a type"))?;
            self.expect(TokenKind::Language)?;
            self.record_completion_slot(completion::GrammarSlot::Language);
            let language = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("LANGUAGE requires a name"))?;
            name_list_node(vec![Node::TypeName(type_name), make_string_node(language)])
        };
        let behavior = self.parse_drop_behavior();
        self.expect_statement_end()?;
        Ok(Node::DropStmt(DropStmt {
            objects: vec![object],
            remove_type,
            behavior,
            missing_ok,
            concurrent: false,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropopclass.html
    // DROP OPERATOR CLASS [ IF EXISTS ] name USING index_method [ CASCADE | RESTRICT ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropopfamily.html
    // DROP OPERATOR FAMILY [ IF EXISTS ] name USING index_method [ CASCADE | RESTRICT ]
    fn parse_drop_operator_family(&mut self, remove_type: ObjectType) -> PResult<Node> {
        self.expect(TokenKind::Operator)?;
        if remove_type == ObjectType::Opclass {
            self.expect(TokenKind::Class)?;
        } else {
            self.expect(TokenKind::Family)?;
        }
        let missing_ok = self.consume_if_exists()?;
        let name_stops = [TokenKind::Using];
        let slot = completion::object_type_slot(remove_type);
        self.record_completion_slot(slot);
        self.record_completion_qualified_name_slot(slot, &name_stops);
        let mut names = self.parse_name_list_until_keywords(&name_stops);
        if names.is_empty() {
            return Err(self.error_here("operator class or family requires a name"));
        }
        self.expect(TokenKind::Using)?;
        self.record_completion_slot(completion::GrammarSlot::AccessMethod);
        let amname = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("USING requires an access method"))?;
        names.insert(0, make_string_node(amname));
        let objects = vec![name_list_node(names)];
        let behavior = self.parse_drop_behavior();
        self.expect_statement_end()?;
        Ok(Node::DropStmt(DropStmt {
            objects,
            remove_type,
            behavior,
            missing_ok,
            concurrent: false,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropusermapping.html
    // DROP USER MAPPING [ IF EXISTS ] FOR { user_name | USER | CURRENT_ROLE | CURRENT_USER | PUBLIC
    // } SERVER server_name
    fn parse_drop_user_mapping(&mut self) -> PResult<Node> {
        self.expect(TokenKind::User)?;
        self.expect(TokenKind::Mapping)?;
        let missing_ok = self.consume_if_exists()?;
        self.expect(TokenKind::For)?;
        self.record_completion_slot(completion::GrammarSlot::Role);
        let user =
            Some(Box::new(self.consume_auth_ident().ok_or_else(|| {
                self.error_here("DROP USER MAPPING requires a user")
            })?));
        self.expect(TokenKind::Server)?;
        self.record_completion_slot(completion::GrammarSlot::ForeignServer);
        let servername = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("SERVER requires a name"))?,
        );
        self.expect_statement_end()?;
        Ok(Node::DropUserMappingStmt(DropUserMappingStmt {
            user,
            servername,
            missing_ok,
        }))
    }
}
