//! Foreign-data wrapper, server, and user-mapping DDL.
//!
//! Handler functions, validators, generic options, authorization, and mapping
//! identities are parsed into their dedicated PostgreSQL raw nodes.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterusermapping.html
    // ALTER USER MAPPING FOR { user_name | USER | CURRENT_ROLE | CURRENT_USER | SESSION_USER |
    // PUBLIC }     SERVER server_name
    //     OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ] )
    pub(super) fn parse_alter_user_mapping(&mut self) -> PResult<Node> {
        self.expect(TokenKind::User)?;
        self.expect(TokenKind::Mapping)?;
        self.expect(TokenKind::For)?;
        self.record_completion_slot(completion::GrammarSlot::Role);
        let user =
            Some(Box::new(self.consume_auth_ident().ok_or_else(|| {
                self.error_here("ALTER USER MAPPING requires a user")
            })?));
        self.expect(TokenKind::Server)?;
        self.record_completion_slot(completion::GrammarSlot::ForeignServer);
        let servername = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("SERVER requires a server name"))?,
        );
        let options = self.parse_alter_generic_options()?;
        self.expect_statement_end()?;
        Ok(Node::AlterUserMappingStmt(AlterUserMappingStmt {
            node_tag: NodeTag::AlterUserMappingStmt,
            user,
            servername,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterforeigndatawrapper.html
    // ALTER FOREIGN DATA WRAPPER name
    //     [ HANDLER handler_function | NO HANDLER ]
    //     [ VALIDATOR validator_function | NO VALIDATOR ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ]) ]
    // ALTER FOREIGN DATA WRAPPER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER |
    // SESSION_USER } ALTER FOREIGN DATA WRAPPER name RENAME TO new_name
    pub(super) fn parse_alter_fdw(&mut self) -> PResult<Node> {
        self.record_completion_slot(completion::GrammarSlot::ForeignDataWrapper);
        let fdwname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER FOREIGN DATA WRAPPER requires a name"))?,
        );
        let func_options = self.parse_fdw_function_options()?;
        let options = if self.at(TokenKind::Options) {
            self.parse_alter_generic_options()?
        } else {
            Vec::new()
        };
        if func_options.is_empty() && options.is_empty() {
            return Err(self.error_here("ALTER FOREIGN DATA WRAPPER requires an option"));
        }
        self.expect_statement_end()?;
        Ok(Node::AlterFdwStmt(AlterFdwStmt {
            node_tag: NodeTag::AlterFdwStmt,
            fdwname,
            func_options,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterserver.html
    // ALTER SERVER name [ VERSION 'new_version' ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ] ) ]
    // ALTER SERVER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SERVER name RENAME TO new_name
    pub(super) fn parse_alter_foreign_server(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Server)?;
        self.record_completion_slot(completion::GrammarSlot::ForeignServer);
        let servername = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER SERVER requires a server name"))?,
        );
        let mut version = None;
        let mut has_version = false;
        if self.consume(TokenKind::VersionP) {
            has_version = true;
            version = if self.consume(TokenKind::NullP) {
                None
            } else {
                Some(self.consume_required_string("VERSION requires a string or NULL")?)
            };
        }
        let options = if self.at(TokenKind::Options) {
            self.parse_alter_generic_options()?
        } else {
            Vec::new()
        };
        if !has_version && options.is_empty() {
            return Err(self.error_here("ALTER SERVER requires VERSION or OPTIONS"));
        }
        self.expect_statement_end()?;
        Ok(Node::AlterForeignServerStmt(AlterForeignServerStmt {
            node_tag: NodeTag::AlterForeignServerStmt,
            servername,
            version,
            options,
            has_version,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createforeigndatawrapper.html
    // CREATE FOREIGN DATA WRAPPER name
    //     [ HANDLER handler_function | NO HANDLER ]
    //     [ VALIDATOR validator_function | NO VALIDATOR ]
    //     [ OPTIONS ( option 'value' [, ... ] ) ]
    pub(super) fn parse_create_fdw(&mut self) -> PResult<Node> {
        self.record_completion_slot(completion::GrammarSlot::ForeignDataWrapper);
        let fdwname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE FOREIGN DATA WRAPPER requires a name"))?,
        );
        let func_options = self.parse_fdw_function_options()?;
        let options = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };
        Ok(Node::CreateFdwStmt(CreateFdwStmt {
            node_tag: NodeTag::CreateFdwStmt,
            fdwname,
            func_options,
            options,
        }))
    }

    pub(super) fn parse_fdw_function_options(&mut self) -> PResult<NodeList> {
        let mut func_options = Vec::new();
        while !self.at_statement_end() && !self.at(TokenKind::Options) {
            self.record_completion_lookahead_tokens(&[
                TokenKind::Handler,
                TokenKind::Validator,
                TokenKind::Connection,
                TokenKind::No,
                TokenKind::Options,
            ]);
            let location = self.location();
            let (name, arg) = match self.peek_kind() {
                kind @ (TokenKind::Handler | TokenKind::Validator | TokenKind::Connection) => {
                    self.advance();
                    self.record_completion_slot(completion::GrammarSlot::Function);
                    let name = match kind {
                        TokenKind::Handler => "handler",
                        TokenKind::Validator => "validator",
                        TokenKind::Connection => "connection",
                        _ => unreachable!(),
                    };
                    let function = self.parse_name_list();
                    if function.is_empty() {
                        return Err(self.error_here(format!(
                            "{} requires a function name",
                            name.to_ascii_uppercase()
                        )));
                    }
                    (name, Some(name_list_node(function)))
                }
                TokenKind::No => {
                    self.advance();
                    self.record_completion_tokens(&[
                        TokenKind::Handler,
                        TokenKind::Validator,
                        TokenKind::Connection,
                    ]);
                    let name = match self.peek_kind() {
                        TokenKind::Handler => "handler",
                        TokenKind::Validator => "validator",
                        TokenKind::Connection => "connection",
                        _ => {
                            return Err(self.error_here(
                                "expected HANDLER, VALIDATOR, or CONNECTION after NO",
                            ));
                        }
                    };
                    self.advance();
                    (name, None)
                }
                _ => return Err(self.error_here("invalid FOREIGN DATA WRAPPER option")),
            };
            func_options.push(make_def_elem(name, arg, location));
        }
        Ok(func_options)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createserver.html
    // CREATE SERVER [ IF NOT EXISTS ] server_name [ TYPE 'server_type' ] [ VERSION 'server_version'
    // ]     FOREIGN DATA WRAPPER fdw_name
    //     [ OPTIONS ( option 'value' [, ... ] ) ]
    pub(super) fn parse_create_server(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Server)?;
        let if_not_exists = self.consume_if_not_exists()?;
        self.record_completion_slot(completion::GrammarSlot::ForeignServer);
        let servername = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE SERVER requires a name"))?,
        );
        let mut servertype = None;
        let mut version = None;
        if self.consume(TokenKind::TypeP) {
            servertype = Some(self.consume_required_string("SERVER TYPE requires a string")?);
        }
        if self.consume(TokenKind::VersionP) {
            if self.consume(TokenKind::NullP) {
                version = None;
            } else {
                version =
                    Some(self.consume_required_string("SERVER VERSION requires a string or NULL")?);
            }
        }
        self.expect(TokenKind::Foreign)?;
        self.expect(TokenKind::DataP)?;
        self.expect(TokenKind::Wrapper)?;
        self.record_completion_slot(completion::GrammarSlot::ForeignDataWrapper);
        let fdwname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("FOREIGN DATA WRAPPER requires a name"))?,
        );
        let options = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };
        Ok(Node::CreateForeignServerStmt(CreateForeignServerStmt {
            node_tag: NodeTag::CreateForeignServerStmt,
            servername,
            servertype,
            version,
            fdwname,
            if_not_exists,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createusermapping.html
    // CREATE USER MAPPING [ IF NOT EXISTS ] FOR { user_name | USER | CURRENT_ROLE | CURRENT_USER |
    // PUBLIC }     SERVER server_name
    //     [ OPTIONS ( option 'value' [ , ... ] ) ]
    pub(super) fn parse_create_user_mapping(&mut self) -> PResult<Node> {
        self.expect(TokenKind::User)?;
        self.expect(TokenKind::Mapping)?;
        let if_not_exists = self.consume_if_not_exists()?;
        self.expect(TokenKind::For)?;
        self.record_completion_slot(completion::GrammarSlot::Role);
        let user = if self.consume(TokenKind::User) {
            Some(Box::new(RoleSpec {
                node_tag: NodeTag::RoleSpec,
                roletype: RoleSpecType::CurrentUser,
                location: self.previous_location() as ParseLoc,
                ..RoleSpec::default()
            }))
        } else {
            Some(Box::new(self.consume_role_spec().ok_or_else(|| {
                self.error_here("USER MAPPING requires a user")
            })?))
        };
        self.expect(TokenKind::Server)?;
        self.record_completion_slot(completion::GrammarSlot::ForeignServer);
        let servername = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("USER MAPPING requires a server"))?,
        );
        let options = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };
        Ok(Node::CreateUserMappingStmt(CreateUserMappingStmt {
            node_tag: NodeTag::CreateUserMappingStmt,
            user,
            servername,
            if_not_exists,
            options,
        }))
    }
}
