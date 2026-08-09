//! Extension creation, alteration, update, and membership parsing.
//!
//! Extension member identities reuse the object-specific identity parsers used by
//! regular DDL instead of accepting arbitrary token text.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createextension.html
    // CREATE EXTENSION [ IF NOT EXISTS ] extension_name
    //     [ WITH ] [ SCHEMA schema_name ]
    //              [ VERSION version ]
    //              [ CASCADE ]
    pub(super) fn parse_create_extension(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Extension)?;
        let if_not_exists = self.consume_if_not_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Extension);
        let extname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE EXTENSION requires a name"))?,
        );
        self.consume(TokenKind::With);
        let mut options = Vec::new();
        while !self.at_statement_end() {
            self.record_completion_lookahead_tokens(&[
                TokenKind::Schema,
                TokenKind::VersionP,
                TokenKind::Cascade,
            ]);
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Schema => {
                    self.advance();
                    self.record_completion_slot(completion::GrammarSlot::Schema);
                    let schema = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("SCHEMA requires a name"))?;
                    options.push(make_def_elem(
                        "schema",
                        Some(make_string_node(schema)),
                        location,
                    ));
                }
                TokenKind::VersionP => {
                    self.advance();
                    let version = self
                        .consume_non_reserved_word_or_sconst()
                        .ok_or_else(|| self.error_here("VERSION requires a value"))?;
                    options.push(make_def_elem(
                        "new_version",
                        Some(make_string_node(version)),
                        location,
                    ));
                }
                TokenKind::Cascade => {
                    self.advance();
                    options.push(make_def_elem(
                        "cascade",
                        Some(node!(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::From => {
                    return Err(self.error_here("CREATE EXTENSION FROM is no longer supported"));
                }
                _ => return Err(self.error_here("invalid CREATE EXTENSION option")),
            }
        }
        Ok(node!(CreateExtensionStmt {
            extname,
            if_not_exists,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterextension.html
    // ALTER EXTENSION name UPDATE [ TO new_version ]
    // ALTER EXTENSION name SET SCHEMA new_schema
    // ALTER EXTENSION name ADD member_object
    // ALTER EXTENSION name DROP member_object
    //
    // where member_object is:
    //
    //   ACCESS METHOD object_name |
    //   AGGREGATE aggregate_name ( aggregate_signature ) |
    //   CAST (source_type AS target_type) |
    //   COLLATION object_name |
    //   CONVERSION object_name |
    //   DOMAIN object_name |
    //   EVENT TRIGGER object_name |
    //   FOREIGN DATA WRAPPER object_name |
    //   FOREIGN TABLE object_name |
    //   FUNCTION function_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   MATERIALIZED VIEW object_name |
    //   OPERATOR operator_name (left_type, right_type) |
    //   OPERATOR CLASS object_name USING index_method |
    //   OPERATOR FAMILY object_name USING index_method |
    //   [ PROCEDURAL ] LANGUAGE object_name |
    //   PROCEDURE procedure_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   ROUTINE routine_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   SCHEMA object_name |
    //   SEQUENCE object_name |
    //   SERVER object_name |
    //   TABLE object_name |
    //   TEXT SEARCH CONFIGURATION object_name |
    //   TEXT SEARCH DICTIONARY object_name |
    //   TEXT SEARCH PARSER object_name |
    //   TEXT SEARCH TEMPLATE object_name |
    //   TRANSFORM FOR type_name LANGUAGE lang_name |
    //   TYPE object_name |
    //   VIEW object_name
    //
    // and aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ ,
    // ... ]
    pub(super) fn parse_alter_extension(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Extension)?;
        self.record_completion_slot(completion::GrammarSlot::Extension);
        let extname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER EXTENSION requires a name"))?,
        );
        self.record_completion_tokens(&[
            TokenKind::Update,
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Set,
        ]);
        if matches!(self.peek_kind(), TokenKind::AddP | TokenKind::Drop) {
            let action = if self.consume(TokenKind::AddP) {
                1
            } else {
                self.expect(TokenKind::Drop)?;
                -1
            };
            let (objtype, object) = self.parse_extension_member_object()?;
            self.expect_statement_end()?;
            Ok(node!(AlterExtensionContentsStmt {
                extname,
                action,
                objtype,
                object: Some(Box::new(object)),
            }))
        } else {
            self.expect(TokenKind::Update)?;
            let mut options = Vec::new();
            while self.consume(TokenKind::To) {
                let location = self.previous_location();
                let version = self
                    .consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("TO requires an extension version"))?;
                options.push(make_def_elem(
                    "new_version",
                    Some(make_string_node(version)),
                    location,
                ));
            }
            self.expect_statement_end()?;
            Ok(node!(AlterExtensionStmt { extname, options }))
        }
    }

    fn parse_extension_member_object(&mut self) -> PResult<(ObjectType, Node)> {
        self.record_completion_tokens(&[
            TokenKind::Access,
            TokenKind::Aggregate,
            TokenKind::Cast,
            TokenKind::Collation,
            TokenKind::ConversionP,
            TokenKind::Database,
            TokenKind::DomainP,
            TokenKind::Event,
            TokenKind::Extension,
            TokenKind::Foreign,
            TokenKind::Function,
            TokenKind::Index,
            TokenKind::Language,
            TokenKind::Materialized,
            TokenKind::Operator,
            TokenKind::Procedure,
            TokenKind::Procedural,
            TokenKind::Property,
            TokenKind::Publication,
            TokenKind::Role,
            TokenKind::Routine,
            TokenKind::Schema,
            TokenKind::Sequence,
            TokenKind::Server,
            TokenKind::Statistics,
            TokenKind::Subscription,
            TokenKind::Table,
            TokenKind::Tablespace,
            TokenKind::TextP,
            TokenKind::Transform,
            TokenKind::TypeP,
            TokenKind::View,
        ]);
        if self.at(TokenKind::Operator)
            && !matches!(self.peek_kind_n(1), TokenKind::Class | TokenKind::Family)
        {
            self.advance();
            self.record_completion_tokens(&[TokenKind::Class, TokenKind::Family]);
            self.record_completion_slot(completion::GrammarSlot::Operator);
            let object =
                self.parse_operator_with_args_until(&[TokenKind::Char(';'), TokenKind::Eof])?;
            return Ok((ObjectType::Operator, Node::ObjectWithArgs(object)));
        }
        let function_type = match self.peek_kind() {
            TokenKind::Aggregate => Some(ObjectType::Aggregate),
            TokenKind::Function => Some(ObjectType::Function),
            TokenKind::Procedure => Some(ObjectType::Procedure),
            TokenKind::Routine => Some(ObjectType::Routine),
            _ => None,
        };
        if let Some(objtype) = function_type {
            self.advance();
            let object = if objtype == ObjectType::Operator {
                self.parse_operator_with_args_until(&[TokenKind::Char(';'), TokenKind::Eof])?
            } else if objtype == ObjectType::Aggregate {
                self.parse_aggregate_with_args_until(&[TokenKind::Char(';'), TokenKind::Eof])?
            } else {
                self.parse_object_with_args_until_with_slot(
                    &[TokenKind::Char(';'), TokenKind::Eof],
                    completion::object_type_slot(objtype),
                )?
            };
            return Ok((objtype, Node::ObjectWithArgs(object)));
        }

        if self.consume(TokenKind::Cast) {
            self.expect(TokenKind::Char('('))?;
            let source = self
                .parse_type_name_until(&[TokenKind::As])
                .ok_or_else(|| self.error_here("CAST requires a source type"))?;
            self.expect(TokenKind::As)?;
            let target = self
                .parse_type_name_until(&[TokenKind::Char(')')])
                .ok_or_else(|| self.error_here("CAST requires a target type"))?;
            self.expect(TokenKind::Char(')'))?;
            return Ok((
                ObjectType::Cast,
                name_list_node(vec![Node::TypeName(source), Node::TypeName(target)]),
            ));
        }
        if self.consume(TokenKind::DomainP) || self.consume(TokenKind::TypeP) {
            let objtype = if self.tokens[self.pos.saturating_sub(1)].kind == TokenKind::DomainP {
                ObjectType::Domain
            } else {
                ObjectType::Type
            };
            let type_name = self
                .parse_type_name_until(&[TokenKind::Char(';'), TokenKind::Eof])
                .ok_or_else(|| self.error_here("object requires a type name"))?;
            return Ok((objtype, Node::TypeName(type_name)));
        }
        if self.consume(TokenKind::Transform) {
            self.expect(TokenKind::For)?;
            let type_name = self
                .parse_type_name_until(&[TokenKind::Language])
                .ok_or_else(|| self.error_here("TRANSFORM FOR requires a type"))?;
            self.expect(TokenKind::Language)?;
            self.record_completion_slot(completion::GrammarSlot::Language);
            let language = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("LANGUAGE requires a name"))?;
            return Ok((
                ObjectType::Transform,
                name_list_node(vec![Node::TypeName(type_name), make_string_node(language)]),
            ));
        }
        if self.consume(TokenKind::Operator) {
            let objtype = if self.consume(TokenKind::Class) {
                ObjectType::Opclass
            } else {
                self.expect(TokenKind::Family)?;
                ObjectType::Opfamily
            };
            let mut names = self.parse_name_list_until_keywords(&[
                TokenKind::Using,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            if names.is_empty() {
                return Err(self.error_here("operator class or family requires a name"));
            }
            self.expect(TokenKind::Using)?;
            self.record_completion_slot(completion::GrammarSlot::AccessMethod);
            let amname = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?;
            names.insert(0, make_string_node(amname));
            return Ok((objtype, name_list_node(names)));
        }

        let objtype = match self.peek_kind() {
            TokenKind::Access => {
                self.advance();
                self.expect(TokenKind::Method)?;
                ObjectType::AccessMethod
            }
            TokenKind::Event => {
                self.advance();
                self.expect(TokenKind::Trigger)?;
                ObjectType::EventTrigger
            }
            TokenKind::Foreign => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::DataP, TokenKind::Table]);
                match self.peek_kind() {
                    TokenKind::DataP => {
                        self.advance();
                        self.expect(TokenKind::Wrapper)?;
                        ObjectType::Fdw
                    }
                    TokenKind::Table => {
                        self.advance();
                        ObjectType::ForeignTable
                    }
                    _ => return Err(self.error_here("FOREIGN requires DATA WRAPPER or TABLE")),
                }
            }
            TokenKind::Procedural => {
                self.advance();
                self.expect(TokenKind::Language)?;
                ObjectType::Language
            }
            TokenKind::Language => {
                self.advance();
                ObjectType::Language
            }
            TokenKind::Materialized => {
                self.advance();
                self.expect(TokenKind::View)?;
                ObjectType::Matview
            }
            TokenKind::Property => {
                self.advance();
                self.expect(TokenKind::Graph)?;
                ObjectType::Propgraph
            }
            TokenKind::TextP => {
                self.advance();
                self.expect(TokenKind::Search)?;
                self.record_completion_tokens(&[
                    TokenKind::Parser,
                    TokenKind::Dictionary,
                    TokenKind::Template,
                    TokenKind::Configuration,
                ]);
                match self.advance().kind {
                    TokenKind::Parser => ObjectType::Tsparser,
                    TokenKind::Dictionary => ObjectType::Tsdictionary,
                    TokenKind::Template => ObjectType::Tstemplate,
                    TokenKind::Configuration => ObjectType::Tsconfiguration,
                    _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
                }
            }
            _ => {
                let objtype = self
                    .consume_object_type()
                    .ok_or_else(|| self.error_here("unsupported extension member object type"))?;
                if !matches!(
                    objtype,
                    ObjectType::Table
                        | ObjectType::Sequence
                        | ObjectType::View
                        | ObjectType::Index
                        | ObjectType::Collation
                        | ObjectType::Conversion
                        | ObjectType::StatisticExt
                        | ObjectType::Database
                        | ObjectType::Role
                        | ObjectType::Subscription
                        | ObjectType::Extension
                        | ObjectType::Publication
                        | ObjectType::Schema
                        | ObjectType::ForeignServer
                        | ObjectType::Tablespace
                ) {
                    return Err(self.error_here("unsupported extension member object type"));
                }
                objtype
            }
        };

        self.record_completion_slot(completion::object_type_slot(objtype));
        let uses_any_name = matches!(
            objtype,
            ObjectType::Table
                | ObjectType::Sequence
                | ObjectType::View
                | ObjectType::Matview
                | ObjectType::Index
                | ObjectType::ForeignTable
                | ObjectType::Propgraph
                | ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::StatisticExt
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
        );
        if uses_any_name {
            let names =
                self.parse_name_list_until_keywords(&[TokenKind::Char(';'), TokenKind::Eof]);
            if names.is_empty() {
                return Err(self.error_here("extension member requires an object name"));
            }
            Ok((objtype, name_list_node(names)))
        } else {
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("extension member requires an object name"))?;
            Ok((objtype, make_string_node(name)))
        }
    }
}
