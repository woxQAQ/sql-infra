//! Shared identity-changing `ALTER` operations across PostgreSQL object kinds.
//!
//! Rename, owner, schema, and dependency actions are parsed here together with
//! capability checks that prevent unsupported object/action combinations.

use super::*;

#[derive(Default)]
pub(super) struct AlterIdentity {
    object_type: ObjectType,
    relation: Option<Box<RangeVar>>,
    object: Option<Box<Node>>,
    subname: Option<std::string::String>,
    missing_ok: bool,
    location: usize,
}

impl Parser {
    pub(super) fn record_alter_identity_actions(&self, identity: &AlterIdentity) {
        if supports_rename(identity.object_type) {
            self.record_completion_tokens(&[TokenKind::Rename]);
        }
        if supports_depends(identity.object_type) {
            self.record_completion_tokens(&[TokenKind::No, TokenKind::Depends]);
        }
        if supports_set_schema(identity.object_type) {
            self.record_completion_tokens(&[TokenKind::Set]);
        }
        if supports_owner(identity.object_type) {
            self.record_completion_tokens(&[TokenKind::Owner]);
        }
    }

    pub(super) fn record_alter_identity_action_continuation(&mut self, identity: &AlterIdentity) {
        if supports_set_schema(identity.object_type) && self.consume(TokenKind::Set) {
            self.record_completion_tokens(&[TokenKind::Schema]);
        } else if supports_depends(identity.object_type) && self.consume(TokenKind::No) {
            self.record_completion_tokens(&[TokenKind::Depends]);
        }
    }

    fn parse_alter_object_kind(&mut self) -> PResult<ObjectType> {
        self.record_completion_lookahead_tokens(&[
            TokenKind::Aggregate,
            TokenKind::Collation,
            TokenKind::ConversionP,
            TokenKind::Database,
            TokenKind::DomainP,
            TokenKind::Event,
            TokenKind::Extension,
            TokenKind::Foreign,
            TokenKind::Function,
            TokenKind::GroupP,
            TokenKind::Index,
            TokenKind::Language,
            TokenKind::LargeP,
            TokenKind::Materialized,
            TokenKind::Operator,
            TokenKind::Policy,
            TokenKind::Procedure,
            TokenKind::Procedural,
            TokenKind::Property,
            TokenKind::Publication,
            TokenKind::Role,
            TokenKind::Routine,
            TokenKind::Rule,
            TokenKind::Schema,
            TokenKind::Sequence,
            TokenKind::Server,
            TokenKind::Statistics,
            TokenKind::Subscription,
            TokenKind::Table,
            TokenKind::Tablespace,
            TokenKind::TextP,
            TokenKind::Trigger,
            TokenKind::TypeP,
            TokenKind::User,
            TokenKind::View,
        ]);
        let kind = match self.peek_kind() {
            TokenKind::Access => {
                self.advance();
                self.expect(TokenKind::Method)?;
                ObjectType::AccessMethod
            }
            TokenKind::Aggregate => ObjectType::Aggregate,
            TokenKind::Collation => ObjectType::Collation,
            TokenKind::ConversionP => ObjectType::Conversion,
            TokenKind::Database => ObjectType::Database,
            TokenKind::DomainP => ObjectType::Domain,
            TokenKind::Event => {
                self.advance();
                self.expect(TokenKind::Trigger)?;
                ObjectType::EventTrigger
            }
            TokenKind::Extension => ObjectType::Extension,
            TokenKind::Foreign => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::DataP, TokenKind::Table]);
                if self.consume(TokenKind::Table) {
                    ObjectType::ForeignTable
                } else {
                    self.expect(TokenKind::DataP)?;
                    self.expect(TokenKind::Wrapper)?;
                    ObjectType::Fdw
                }
            }
            TokenKind::Function => ObjectType::Function,
            TokenKind::GroupP | TokenKind::Role | TokenKind::User => ObjectType::Role,
            TokenKind::Index => ObjectType::Index,
            TokenKind::Language => {
                self.advance();
                ObjectType::Language
            }
            TokenKind::LargeP => {
                self.advance();
                self.expect(TokenKind::ObjectP)?;
                ObjectType::Largeobject
            }
            TokenKind::Materialized => {
                self.advance();
                self.expect(TokenKind::View)?;
                ObjectType::Matview
            }
            TokenKind::Operator => {
                self.advance();
                self.record_completion_lookahead_tokens(&[TokenKind::Class, TokenKind::Family]);
                if self.consume(TokenKind::Class) {
                    ObjectType::Opclass
                } else if self.consume(TokenKind::Family) {
                    ObjectType::Opfamily
                } else {
                    ObjectType::Operator
                }
            }
            TokenKind::Policy => ObjectType::Policy,
            TokenKind::Procedure => ObjectType::Procedure,
            TokenKind::Property => {
                self.advance();
                self.expect(TokenKind::Graph)?;
                ObjectType::Propgraph
            }
            TokenKind::Publication => ObjectType::Publication,
            TokenKind::Routine => ObjectType::Routine,
            TokenKind::Rule => ObjectType::Rule,
            TokenKind::Schema => ObjectType::Schema,
            TokenKind::Sequence => ObjectType::Sequence,
            TokenKind::Server => ObjectType::ForeignServer,
            TokenKind::Statistics => ObjectType::StatisticExt,
            TokenKind::Subscription => ObjectType::Subscription,
            TokenKind::Table => ObjectType::Table,
            TokenKind::Tablespace => ObjectType::Tablespace,
            TokenKind::TextP => {
                self.advance();
                self.expect(TokenKind::Search)?;
                self.record_completion_tokens(&[
                    TokenKind::Parser,
                    TokenKind::Dictionary,
                    TokenKind::Template,
                    TokenKind::Configuration,
                ]);
                match self.peek_kind() {
                    TokenKind::Parser => {
                        self.advance();
                        ObjectType::Tsparser
                    }
                    TokenKind::Dictionary => {
                        self.advance();
                        ObjectType::Tsdictionary
                    }
                    TokenKind::Template => {
                        self.advance();
                        ObjectType::Tstemplate
                    }
                    TokenKind::Configuration => {
                        self.advance();
                        ObjectType::Tsconfiguration
                    }
                    _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
                }
            }
            TokenKind::Trigger => ObjectType::Trigger,
            TokenKind::TypeP => ObjectType::Type,
            TokenKind::View => ObjectType::View,
            TokenKind::Procedural => {
                self.advance();
                self.expect(TokenKind::Language)?;
                ObjectType::Language
            }
            other => {
                return Err(self.error_here(format!("unsupported ALTER object type {other:?}")));
            }
        };
        if !matches!(
            kind,
            ObjectType::AccessMethod
                | ObjectType::EventTrigger
                | ObjectType::Fdw
                | ObjectType::ForeignTable
                | ObjectType::Largeobject
                | ObjectType::Matview
                | ObjectType::Opclass
                | ObjectType::Opfamily
                | ObjectType::Propgraph
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Language
                | ObjectType::Operator
        ) {
            self.advance();
        }
        Ok(kind)
    }

    pub(super) fn parse_alter_identity(
        &mut self,
        action_stops: &[TokenKind],
    ) -> PResult<AlterIdentity> {
        let object_type = self.parse_alter_object_kind()?;
        let action_stops = action_stops
            .iter()
            .copied()
            .filter(|stop| match stop {
                TokenKind::Rename => supports_rename(object_type),
                TokenKind::Depends | TokenKind::No => supports_depends(object_type),
                TokenKind::Set => supports_set_schema(object_type),
                TokenKind::Owner => supports_owner(object_type),
                TokenKind::Completion => true,
                _ => true,
            })
            .collect::<Vec<_>>();
        let missing_ok = if matches!(
            object_type,
            ObjectType::Table
                | ObjectType::Sequence
                | ObjectType::View
                | ObjectType::Matview
                | ObjectType::Index
                | ObjectType::ForeignTable
                | ObjectType::Policy
                | ObjectType::Propgraph
        ) {
            self.consume_if_exists()?
        } else {
            false
        };
        let object_slot = object_type_slot(object_type);
        self.record_completion_slot(object_slot);
        self.record_completion_qualified_name_slot(object_slot, &action_stops);
        let mut identity = AlterIdentity {
            object_type,
            missing_ok,
            location: self.location(),
            ..AlterIdentity::default()
        };
        if relation_object_type(object_type) {
            let relation = if matches!(object_type, ObjectType::Table | ObjectType::ForeignTable) {
                self.parse_relation_expr()?
            } else {
                self.try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("ALTER object requires a relation name"))?
            };
            identity.relation = Some(Box::new(relation));
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Policy | ObjectType::Rule | ObjectType::Trigger
        ) {
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("ALTER object requires a name"))?;
            self.expect(TokenKind::On)?;
            identity.relation = Some(Box::new(
                self.try_parse_qualified_range_var_with_slot(GrammarSlot::Table)
                    .ok_or_else(|| self.error_here("ON requires a relation name"))?,
            ));
            identity.subname = Some(name.clone());
            identity.object = Some(Box::new(name_list_node(vec![make_string_node(name)])));
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Aggregate
                | ObjectType::Function
                | ObjectType::Procedure
                | ObjectType::Routine
                | ObjectType::Operator
        ) {
            let object = if object_type == ObjectType::Operator {
                self.parse_operator_with_args_until(&action_stops)?
            } else if object_type == ObjectType::Aggregate {
                self.parse_aggregate_with_args_structured()?
            } else {
                self.parse_routine_with_args_with_slot(object_type_slot(object_type))?
            };
            identity.object = Some(Box::new(Node::ObjectWithArgs(object)));
            return Ok(identity);
        }
        if matches!(object_type, ObjectType::Opclass | ObjectType::Opfamily) {
            let mut names = self.parse_name_list_until_keywords(&[TokenKind::Using]);
            if names.is_empty() {
                return Err(self.error_here("operator class or family requires a name"));
            }
            self.expect(TokenKind::Using)?;
            let amname = self.parse_access_method_name()?;
            names.insert(0, make_string_node(amname));
            identity.object = Some(Box::new(name_list_node(names)));
            return Ok(identity);
        }
        if object_type == ObjectType::Largeobject {
            identity.object = Some(Box::new(self.parse_numeric_only()?));
            return Ok(identity);
        }
        if object_type == ObjectType::Role {
            let name = self
                .consume_role_id()?
                .ok_or_else(|| self.error_here("ALTER ROLE requires a role name"))?;
            identity.object = Some(Box::new(make_string_node(name.clone())));
            identity.subname = Some(name);
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::Domain
                | ObjectType::StatisticExt
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Type
        ) {
            let names = self.parse_name_list_until_keywords_allow_initial_stop(&action_stops);
            if names.is_empty() {
                return Err(self.error_here("ALTER object requires a qualified name"));
            }
            identity.object = Some(Box::new(name_list_node(names)));
            return Ok(identity);
        }
        let name = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("ALTER object requires a name"))?;
        identity.object = Some(Box::new(make_string_node(name.clone())));
        if matches!(
            object_type,
            ObjectType::Database | ObjectType::Role | ObjectType::Schema | ObjectType::Tablespace
        ) {
            identity.subname = Some(name);
        }
        Ok(identity)
    }

    // PostgreSQL 18 Synopsis subset — RENAME
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-commands.html
    // - https://www.postgresql.org/docs/18/sql-altertable.html
    // - https://www.postgresql.org/docs/18/sql-alterfunction.html
    // - https://www.postgresql.org/docs/18/sql-altertype.html
    //
    // Normalized across the object-specific ALTER command pages:
    // ALTER object_type object_identity RENAME TO new_name
    // ALTER { TABLE | VIEW | MATERIALIZED VIEW | FOREIGN TABLE } [ IF EXISTS ] relation
    //     RENAME [ COLUMN ] column_name TO new_name
    // ALTER { TABLE | DOMAIN } object_identity
    //     RENAME CONSTRAINT constraint_name TO new_name
    // ALTER TYPE type_name
    //     RENAME ATTRIBUTE attribute_name TO new_name [ CASCADE | RESTRICT ]
    pub(super) fn parse_rename(&mut self) -> PResult<Node> {
        let mut identity = self.parse_alter_identity(&[TokenKind::Rename])?;
        if !supports_rename(identity.object_type) {
            return Err(self.error_here("this object type does not support RENAME"));
        }
        if identity.missing_ok
            && !matches!(
                identity.object_type,
                ObjectType::Table
                    | ObjectType::Sequence
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::Index
                    | ObjectType::ForeignTable
                    | ObjectType::Policy
            )
        {
            return Err(self.error_here("IF EXISTS is not supported for this RENAME object"));
        }
        self.expect(TokenKind::Rename)?;
        self.record_completion_tokens(&[TokenKind::To]);
        match identity.object_type {
            ObjectType::Table
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::ForeignTable => {
                self.record_completion_tokens(&[TokenKind::Column]);
                self.record_completion_slot(GrammarSlot::Column);
                if identity.object_type == ObjectType::Table {
                    self.record_completion_tokens(&[TokenKind::Constraint]);
                }
            }
            ObjectType::Domain => {
                self.record_completion_tokens(&[TokenKind::Constraint]);
            }
            ObjectType::Type => {
                self.record_completion_tokens(&[TokenKind::Attribute]);
            }
            _ => {}
        }
        let mut relation_type = ObjectType::default();
        let mut rename_type = identity.object_type;
        let mut behavior = DropBehavior::Restrict;
        match self.peek_kind() {
            TokenKind::Column => {
                self.advance();
                if !matches!(
                    identity.object_type,
                    ObjectType::Table
                        | ObjectType::View
                        | ObjectType::Matview
                        | ObjectType::ForeignTable
                ) {
                    return Err(self.error_here("RENAME COLUMN is not valid for this object type"));
                }
                rename_type = ObjectType::Column;
                relation_type = identity.object_type;
                self.record_completion_slot(GrammarSlot::Column);
                identity.subname = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RENAME COLUMN requires a column name"))?,
                );
            }
            TokenKind::Constraint => {
                self.advance();
                rename_type = if identity.object_type == ObjectType::Domain {
                    ObjectType::Domconstraint
                } else if identity.object_type == ObjectType::Table {
                    ObjectType::Tabconstraint
                } else {
                    return Err(
                        self.error_here("RENAME CONSTRAINT is only valid for a table or domain")
                    );
                };
                self.record_completion_slot(GrammarSlot::Constraint);
                identity.subname = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RENAME CONSTRAINT requires a name"))?,
                );
            }
            TokenKind::Attribute => {
                self.advance();
                if identity.object_type != ObjectType::Type {
                    return Err(self.error_here("RENAME ATTRIBUTE is only valid for a type"));
                }
                rename_type = ObjectType::Attribute;
                relation_type = ObjectType::Type;
                self.record_completion_slot(GrammarSlot::Attribute);
                identity.subname = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RENAME ATTRIBUTE requires a name"))?,
                );
                let Some(Node::AArrayExpr(names)) = identity.object.as_deref() else {
                    return Err(self.error_here("type name is not representable as a relation"));
                };
                identity.relation = Some(Box::new(range_var_from_parts(
                    list_to_names(&names.elements),
                    identity.location,
                )));
                identity.object = None;
            }
            _ if matches!(
                identity.object_type,
                ObjectType::Table
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::ForeignTable
            ) && !self.at(TokenKind::To) =>
            {
                // COLUMN is optional in PostgreSQL's RENAME [COLUMN] syntax.
                rename_type = ObjectType::Column;
                relation_type = identity.object_type;
                self.record_completion_slot(GrammarSlot::Column);
                identity.subname = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RENAME requires a column name or TO"))?,
                );
            }
            _ => {}
        }
        self.expect(TokenKind::To)?;
        let newname = Some(if identity.object_type == ObjectType::Role {
            self.consume_new_role_id()?
                .ok_or_else(|| self.error_here("RENAME TO requires a new role name"))?
        } else {
            self.record_completion_slot(GrammarSlot::AnyName);
            self.consume_col_id()
                .ok_or_else(|| self.error_here("RENAME TO requires a new name"))?
        });
        if rename_type == ObjectType::Attribute {
            behavior = self.parse_drop_behavior();
        }
        if matches!(
            identity.object_type,
            ObjectType::Database
                | ObjectType::Role
                | ObjectType::Schema
                | ObjectType::Tablespace
                | ObjectType::Policy
                | ObjectType::Rule
                | ObjectType::Trigger
        ) {
            identity.object = None;
        }
        self.expect_statement_end()?;
        Ok(node!(RenameStmt {
            rename_type,
            relation_type,
            relation: identity.relation,
            object: identity.object,
            subname: identity.subname,
            newname,
            behavior,
            missing_ok: identity.missing_ok,
        }))
    }

    // PostgreSQL 18 Synopsis subset — [ NO ] DEPENDS ON EXTENSION
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-alterfunction.html
    // - https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // - https://www.postgresql.org/docs/18/sql-alterroutine.html
    // - https://www.postgresql.org/docs/18/sql-altertrigger.html
    // - https://www.postgresql.org/docs/18/sql-altermaterializedview.html
    // - https://www.postgresql.org/docs/18/sql-alterindex.html
    //
    // Normalized across the object-specific ALTER command pages:
    // ALTER { FUNCTION | PROCEDURE | ROUTINE } routine_identity
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER { TRIGGER trigger_name ON table_name | MATERIALIZED VIEW name | INDEX name }
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    pub(super) fn parse_alter_object_depends(&mut self) -> PResult<Node> {
        let mut identity = self.parse_alter_identity(&[TokenKind::No, TokenKind::Depends])?;
        if identity.missing_ok {
            return Err(self.error_here("IF EXISTS is not supported with DEPENDS ON EXTENSION"));
        }
        if !supports_depends(identity.object_type) {
            return Err(self.error_here("this object type does not support DEPENDS ON EXTENSION"));
        }
        let remove = self.consume(TokenKind::No);
        self.expect(TokenKind::Depends)?;
        self.expect(TokenKind::On)?;
        self.expect(TokenKind::Extension)?;
        self.record_completion_slot(GrammarSlot::Extension);
        let extname = Some(Box::new(String::new(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("EXTENSION requires a name"))?,
        )));
        self.expect_statement_end()?;
        Ok(node!(AlterObjectDependsStmt {
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object.take(),
            extname,
            remove,
        }))
    }

    // PostgreSQL 18 Synopsis subset — SET SCHEMA
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-commands.html
    // - https://www.postgresql.org/docs/18/sql-altertable.html
    // - https://www.postgresql.org/docs/18/sql-alterfunction.html
    // - https://www.postgresql.org/docs/18/sql-altertype.html
    //
    // Normalized across the object-specific ALTER command pages:
    // ALTER object_type object_identity SET SCHEMA new_schema
    // ALTER { TABLE | SEQUENCE | VIEW | MATERIALIZED VIEW | FOREIGN TABLE }
    //     [ IF EXISTS ] name SET SCHEMA new_schema
    //
    // The supported object types are enumerated by this function below.
    pub(super) fn parse_alter_object_schema(&mut self) -> PResult<Node> {
        let identity = self.parse_alter_identity(&[TokenKind::Set])?;
        if identity.missing_ok
            && !matches!(
                identity.object_type,
                ObjectType::Propgraph
                    | ObjectType::Table
                    | ObjectType::Sequence
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::ForeignTable
            )
        {
            return Err(self.error_here("IF EXISTS is not supported for this SET SCHEMA object"));
        }
        if !supports_set_schema(identity.object_type) {
            return Err(self.error_here("this object type does not support SET SCHEMA"));
        }
        self.expect(TokenKind::Set)?;
        self.expect(TokenKind::Schema)?;
        self.record_completion_slot(GrammarSlot::Schema);
        let newschema = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("SET SCHEMA requires a schema name"))?,
        );
        self.expect_statement_end()?;
        Ok(node!(AlterObjectSchemaStmt {
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object,
            newschema,
            missing_ok: identity.missing_ok,
        }))
    }

    // PostgreSQL 18 Synopsis subset — OWNER TO
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-commands.html
    // - https://www.postgresql.org/docs/18/sql-alterdatabase.html
    // - https://www.postgresql.org/docs/18/sql-alterfunction.html
    // - https://www.postgresql.org/docs/18/sql-alterschema.html
    // - https://www.postgresql.org/docs/18/sql-altertype.html
    //
    // Normalized across the object-specific ALTER command pages:
    // ALTER object_type object_identity
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // The supported object types are enumerated by this function below.
    pub(super) fn parse_alter_owner(&mut self) -> PResult<Node> {
        let identity = self.parse_alter_identity(&[TokenKind::Owner])?;
        if identity.missing_ok {
            return Err(self.error_here("IF EXISTS is not supported with OWNER TO"));
        }
        if !supports_owner(identity.object_type) {
            return Err(self.error_here("this object type does not support OWNER TO"));
        }
        self.expect(TokenKind::Owner)?;
        self.expect(TokenKind::To)?;
        self.record_completion_slot(GrammarSlot::Role);
        let newowner =
            Some(Box::new(self.consume_role_spec().ok_or_else(|| {
                self.error_here("OWNER TO requires a role")
            })?));
        self.expect_statement_end()?;
        Ok(node!(AlterOwnerStmt {
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object,
            newowner,
        }))
    }
}

fn supports_rename(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Aggregate
            | ObjectType::Collation
            | ObjectType::Conversion
            | ObjectType::Database
            | ObjectType::Domain
            | ObjectType::Fdw
            | ObjectType::Function
            | ObjectType::Role
            | ObjectType::Language
            | ObjectType::Opclass
            | ObjectType::Opfamily
            | ObjectType::Policy
            | ObjectType::Procedure
            | ObjectType::Propgraph
            | ObjectType::Publication
            | ObjectType::Routine
            | ObjectType::Schema
            | ObjectType::ForeignServer
            | ObjectType::Subscription
            | ObjectType::Table
            | ObjectType::Sequence
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::Index
            | ObjectType::ForeignTable
            | ObjectType::Rule
            | ObjectType::Trigger
            | ObjectType::EventTrigger
            | ObjectType::Tablespace
            | ObjectType::StatisticExt
            | ObjectType::Tsparser
            | ObjectType::Tsdictionary
            | ObjectType::Tstemplate
            | ObjectType::Tsconfiguration
            | ObjectType::Type
    )
}

fn supports_depends(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Function
            | ObjectType::Procedure
            | ObjectType::Routine
            | ObjectType::Trigger
            | ObjectType::Matview
            | ObjectType::Index
    )
}

fn supports_set_schema(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Aggregate
            | ObjectType::Collation
            | ObjectType::Conversion
            | ObjectType::Domain
            | ObjectType::Extension
            | ObjectType::Function
            | ObjectType::Operator
            | ObjectType::Opclass
            | ObjectType::Opfamily
            | ObjectType::Procedure
            | ObjectType::Propgraph
            | ObjectType::Routine
            | ObjectType::Table
            | ObjectType::StatisticExt
            | ObjectType::Tsparser
            | ObjectType::Tsdictionary
            | ObjectType::Tstemplate
            | ObjectType::Tsconfiguration
            | ObjectType::Sequence
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::ForeignTable
            | ObjectType::Type
    )
}

fn supports_owner(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Aggregate
            | ObjectType::Collation
            | ObjectType::Conversion
            | ObjectType::Database
            | ObjectType::Domain
            | ObjectType::Function
            | ObjectType::Language
            | ObjectType::Largeobject
            | ObjectType::Operator
            | ObjectType::Opclass
            | ObjectType::Opfamily
            | ObjectType::Procedure
            | ObjectType::Propgraph
            | ObjectType::Routine
            | ObjectType::Schema
            | ObjectType::Type
            | ObjectType::Tablespace
            | ObjectType::StatisticExt
            | ObjectType::Tsdictionary
            | ObjectType::Tsconfiguration
            | ObjectType::Fdw
            | ObjectType::ForeignServer
            | ObjectType::EventTrigger
            | ObjectType::Publication
            | ObjectType::Subscription
    )
}

pub(super) fn relation_object_type(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Table
            | ObjectType::Sequence
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::Index
            | ObjectType::ForeignTable
            | ObjectType::Propgraph
    )
}
