use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set.html
    // SET [ SESSION | LOCAL ] configuration_parameter { TO | = } { value | 'value' | DEFAULT }
    // SET [ SESSION | LOCAL ] TIME ZONE { value | 'value' | LOCAL | DEFAULT }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set-constraints.html
    // SET CONSTRAINTS { ALL | name [, ...] } { DEFERRED | IMMEDIATE }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set-role.html
    // SET [ SESSION | LOCAL ] ROLE role_name
    // SET [ SESSION | LOCAL ] ROLE NONE
    // RESET ROLE
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set-session-authorization.html
    // SET [ SESSION | LOCAL ] SESSION AUTHORIZATION user_name
    // SET [ SESSION | LOCAL ] SESSION AUTHORIZATION DEFAULT
    // RESET SESSION AUTHORIZATION
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set-transaction.html
    // SET TRANSACTION transaction_mode [, ...]
    // SET TRANSACTION SNAPSHOT snapshot_id
    // SET SESSION CHARACTERISTICS AS TRANSACTION transaction_mode [, ...]
    //
    // where transaction_mode is one of:
    //
    //     ISOLATION LEVEL { SERIALIZABLE | REPEATABLE READ | READ COMMITTED | READ UNCOMMITTED }
    //     READ WRITE | READ ONLY
    //     [ NOT ] DEFERRABLE
    pub(super) fn parse_set_or_constraints(&mut self) -> PResult<Node> {
        if self.peek_kind_n(1) == TokenKind::Constraints {
            self.expect(TokenKind::Set)?;
            self.expect(TokenKind::Constraints)?;
            let constraints = if self.consume(TokenKind::All) {
                Vec::new()
            } else {
                let mut constraints = Vec::new();
                loop {
                    constraints.push(Node::RangeVar(
                        self.try_parse_qualified_range_var().ok_or_else(|| {
                            self.error_here("SET CONSTRAINTS requires a constraint name or ALL")
                        })?,
                    ));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at_any(&[
                        TokenKind::Deferred,
                        TokenKind::Immediate,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]) {
                        return Err(self.error_here("expected a constraint name after ','"));
                    }
                }
                constraints
            };
            let deferred = if self.consume(TokenKind::Deferred) {
                true
            } else {
                self.expect(TokenKind::Immediate)?;
                false
            };
            self.expect_statement_end()?;
            Ok(Node::ConstraintsSetStmt(ConstraintsSetStmt {
                node_tag: NodeTag::ConstraintsSetStmt,
                constraints,
                deferred,
            }))
        } else {
            Ok(Node::VariableSetStmt(self.parse_variable_set_like(true)?))
        }
    }

    pub(super) fn parse_variable_set_like(
        &mut self,
        allow_scope: bool,
    ) -> PResult<VariableSetStmt> {
        if self.consume(TokenKind::Reset) {
            let (kind, name) = if self.consume(TokenKind::All) {
                (VariableSetKind::ResetAll, None)
            } else if self.consume(TokenKind::Time) {
                self.expect(TokenKind::Zone)?;
                (VariableSetKind::Reset, Some("timezone".to_owned()))
            } else if self.consume(TokenKind::Transaction) {
                self.expect(TokenKind::Isolation)?;
                self.expect(TokenKind::Level)?;
                (
                    VariableSetKind::Reset,
                    Some("transaction_isolation".to_owned()),
                )
            } else if self.consume(TokenKind::Session) {
                self.expect(TokenKind::Authorization)?;
                (
                    VariableSetKind::Reset,
                    Some("session_authorization".to_owned()),
                )
            } else {
                (
                    VariableSetKind::Reset,
                    Some(
                        self.consume_setting_name()
                            .ok_or_else(|| self.error_here("RESET requires a parameter name"))?,
                    ),
                )
            };
            self.expect_statement_end()?;
            return Ok(VariableSetStmt {
                node_tag: NodeTag::VariableSetStmt,
                kind,
                name,
                location: -1,
                ..VariableSetStmt::default()
            });
        }

        self.expect(TokenKind::Set)?;
        let is_local = allow_scope && self.consume(TokenKind::Local);
        if allow_scope
            && self.at(TokenKind::Session)
            && !matches!(
                self.peek_kind_n(1),
                TokenKind::Characteristics | TokenKind::Authorization
            )
        {
            self.advance();
        }
        let mut stmt = VariableSetStmt {
            node_tag: NodeTag::VariableSetStmt,
            kind: VariableSetKind::SetValue,
            is_local,
            location: -1,
            ..VariableSetStmt::default()
        };
        if self.at(TokenKind::Transaction) && self.peek_kind_n(1) != TokenKind::Snapshot {
            self.advance();
            stmt.kind = VariableSetKind::SetMulti;
            stmt.name = Some("TRANSACTION".to_owned());
            stmt.args = self.parse_transaction_modes()?;
            if stmt.args.is_empty() {
                return Err(self.error_here("SET TRANSACTION requires at least one mode"));
            }
            stmt.jumble_args = true;
        } else if self.at(TokenKind::Session) && self.peek_kind_n(1) == TokenKind::Characteristics {
            self.advance();
            self.expect(TokenKind::Characteristics)?;
            self.expect(TokenKind::As)?;
            self.expect(TokenKind::Transaction)?;
            stmt.kind = VariableSetKind::SetMulti;
            stmt.name = Some("SESSION CHARACTERISTICS".to_owned());
            stmt.args = self.parse_transaction_modes()?;
            if stmt.args.is_empty() {
                return Err(self.error_here("SESSION CHARACTERISTICS requires transaction modes"));
            }
            stmt.jumble_args = true;
        } else if self.consume(TokenKind::Time) {
            self.expect(TokenKind::Zone)?;
            stmt.name = Some("timezone".to_owned());
            if self.consume(TokenKind::Default) || self.consume(TokenKind::Local) {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let tokens = self.take_until_top_level(&[TokenKind::Char(';'), TokenKind::Eof]);
                stmt.args = vec![parse_time_zone_value_tokens(tokens)?];
            }
            stmt.jumble_args = true;
        } else if self.consume(TokenKind::Schema) {
            stmt.name = Some("search_path".to_owned());
            stmt.args = vec![Node::AConst(AConst::string(
                self.consume_required_string("SET SCHEMA requires a string")?,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
        } else if self.consume(TokenKind::Names) {
            stmt.name = Some("client_encoding".to_owned());
            if self.consume(TokenKind::Default) {
                stmt.kind = VariableSetKind::SetDefault;
                stmt.location = self.previous_location() as ParseLoc;
            } else if self.at_statement_end() {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let value = self
                    .at(TokenKind::SConst)
                    .then(|| self.consume_string_like())
                    .flatten()
                    .ok_or_else(|| self.error_here("SET NAMES requires an encoding"))?;
                stmt.args = vec![Node::AConst(AConst::string(
                    value,
                    self.previous_location() as ParseLoc,
                ))];
                stmt.location = self.previous_location() as ParseLoc;
            }
        } else if self.consume(TokenKind::Role) {
            stmt.name = Some("role".to_owned());
            let value = self
                .consume_non_reserved_word_or_sconst()
                .ok_or_else(|| self.error_here("SET ROLE requires a role"))?;
            stmt.args = vec![Node::AConst(AConst::string(
                value,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
        } else if self.consume(TokenKind::Session) {
            self.expect(TokenKind::Authorization)?;
            stmt.name = Some("session_authorization".to_owned());
            if self.consume(TokenKind::Default) {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let value = self
                    .consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("SET SESSION AUTHORIZATION requires a role"))?;
                stmt.args = vec![Node::AConst(AConst::string(
                    value,
                    self.previous_location() as ParseLoc,
                ))];
                stmt.location = self.previous_location() as ParseLoc;
            }
        } else if self.consume(TokenKind::XmlP) {
            self.expect(TokenKind::Option)?;
            let (value, value_location) = if self.consume(TokenKind::DocumentP) {
                ("DOCUMENT", self.previous_location() as ParseLoc)
            } else {
                self.expect(TokenKind::ContentP)?;
                ("CONTENT", self.previous_location() as ParseLoc)
            };
            stmt.name = Some("xmloption".to_owned());
            stmt.args = vec![Node::AConst(AConst::string(value, value_location))];
            stmt.jumble_args = true;
        } else if self.consume(TokenKind::Transaction) {
            self.expect(TokenKind::Snapshot)?;
            stmt.kind = VariableSetKind::SetMulti;
            stmt.name = Some("TRANSACTION SNAPSHOT".to_owned());
            stmt.args = vec![Node::AConst(AConst::string(
                self.consume_required_string("TRANSACTION SNAPSHOT requires a string")?,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
        } else {
            stmt.name = Some(
                self.consume_setting_name()
                    .ok_or_else(|| self.error_here("SET requires a parameter name"))?,
            );
            if self.consume(TokenKind::From) {
                self.expect(TokenKind::CurrentP)?;
                stmt.kind = VariableSetKind::SetCurrent;
            } else {
                if !self.consume(TokenKind::To) && !self.consume(TokenKind::Char('=')) {
                    return Err(self.error_here("SET parameter requires TO, '=', or FROM CURRENT"));
                }
                let value_location = self.location() as ParseLoc;
                if self.consume(TokenKind::Default) {
                    stmt.kind = VariableSetKind::SetDefault;
                    stmt.location = -1;
                } else if self.consume(TokenKind::NullP) {
                    stmt.args = vec![Node::AConst(AConst::null(
                        self.previous_location() as ParseLoc
                    ))];
                    stmt.location = value_location;
                } else {
                    stmt.args = self.parse_setting_value_list()?;
                    stmt.location = value_location;
                }
            }
        }
        self.expect_statement_end()?;
        Ok(stmt)
    }

    pub(super) fn parse_setting_value_list(&mut self) -> PResult<NodeList> {
        let mut args = Vec::new();
        loop {
            let tokens = self.take_until_top_level(&[
                TokenKind::Char(','),
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            args.push(parse_setting_value_tokens(tokens)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_statement_end() {
                return Err(self.error_here("expected a SET value after ','"));
            }
        }
        Ok(args)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-reset.html
    // RESET configuration_parameter
    // RESET ALL
    pub(super) fn parse_variable_reset(&mut self) -> PResult<Node> {
        Ok(Node::VariableSetStmt(self.parse_variable_set_like(true)?))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-show.html
    // SHOW name
    // SHOW ALL
    pub(super) fn parse_variable_show(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Show)?;
        let name = if self.consume(TokenKind::All) {
            Some("all".to_owned())
        } else if self.consume(TokenKind::Time) {
            self.expect(TokenKind::Zone)?;
            Some("timezone".to_owned())
        } else if self.consume(TokenKind::Transaction) {
            self.expect(TokenKind::Isolation)?;
            self.expect(TokenKind::Level)?;
            Some("transaction_isolation".to_owned())
        } else if self.consume(TokenKind::Session) {
            self.expect(TokenKind::Authorization)?;
            Some("session_authorization".to_owned())
        } else {
            Some(
                self.consume_setting_name()
                    .ok_or_else(|| self.error_here("SHOW requires a parameter name"))?,
            )
        };
        self.expect_statement_end()?;
        Ok(Node::VariableShowStmt(VariableShowStmt {
            node_tag: NodeTag::VariableShowStmt,
            name,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-begin.html
    // BEGIN [ WORK | TRANSACTION ] [ transaction_mode [, ...] ]
    //
    // where transaction_mode is one of:
    //
    //     ISOLATION LEVEL { SERIALIZABLE | REPEATABLE READ | READ COMMITTED | READ UNCOMMITTED }
    //     READ WRITE | READ ONLY
    //     [ NOT ] DEFERRABLE
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-start-transaction.html
    // START TRANSACTION [ transaction_mode [, ...] ]
    //
    // where transaction_mode is one of:
    //
    //     ISOLATION LEVEL { SERIALIZABLE | REPEATABLE READ | READ COMMITTED | READ UNCOMMITTED }
    //     READ WRITE | READ ONLY
    //     [ NOT ] DEFERRABLE
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-commit.html
    // COMMIT [ WORK | TRANSACTION ] [ AND [ NO ] CHAIN ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-end.html
    // END [ WORK | TRANSACTION ] [ AND [ NO ] CHAIN ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-commit-prepared.html
    // COMMIT PREPARED transaction_id
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-rollback.html
    // ROLLBACK [ WORK | TRANSACTION ] [ AND [ NO ] CHAIN ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-abort.html
    // ABORT [ WORK | TRANSACTION ] [ AND [ NO ] CHAIN ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-rollback-prepared.html
    // ROLLBACK PREPARED transaction_id
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-rollback-to.html
    // ROLLBACK [ WORK | TRANSACTION ] TO [ SAVEPOINT ] savepoint_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-savepoint.html
    // SAVEPOINT savepoint_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-release-savepoint.html
    // RELEASE [ SAVEPOINT ] savepoint_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-prepare-transaction.html
    // PREPARE TRANSACTION transaction_id
    pub(super) fn parse_transaction(&mut self) -> PResult<Node> {
        let first = self.advance().kind;
        let mut stmt = TransactionStmt {
            node_tag: NodeTag::TransactionStmt,
            location: -1,
            ..TransactionStmt::default()
        };
        match first {
            TokenKind::BeginP => {
                stmt.kind = TransactionStmtKind::Begin;
                self.consume_opt_transaction();
                stmt.options = self.parse_transaction_modes()?;
            }
            TokenKind::Start => {
                stmt.kind = TransactionStmtKind::Start;
                self.expect(TokenKind::Transaction)?;
                stmt.options = self.parse_transaction_modes()?;
            }
            TokenKind::Commit => {
                if self.consume(TokenKind::Prepared) {
                    stmt.kind = TransactionStmtKind::CommitPrepared;
                    let location = self.location();
                    stmt.gid =
                        Some(self.consume_required_string("COMMIT PREPARED requires a string")?);
                    stmt.location = location as ParseLoc;
                } else {
                    stmt.kind = TransactionStmtKind::Commit;
                    self.consume_opt_transaction();
                    stmt.chain = self.parse_transaction_chain()?;
                }
            }
            TokenKind::EndP => {
                stmt.kind = TransactionStmtKind::Commit;
                self.consume_opt_transaction();
                stmt.chain = self.parse_transaction_chain()?;
            }
            TokenKind::Rollback => {
                if self.consume(TokenKind::Prepared) {
                    stmt.kind = TransactionStmtKind::RollbackPrepared;
                    let location = self.location();
                    stmt.gid =
                        Some(self.consume_required_string("ROLLBACK PREPARED requires a string")?);
                    stmt.location = location as ParseLoc;
                } else {
                    self.consume_opt_transaction();
                    if self.consume(TokenKind::To) {
                        stmt.kind = TransactionStmtKind::RollbackTo;
                        self.consume(TokenKind::Savepoint);
                        let location = self.location();
                        stmt.savepoint_name =
                            Some(self.consume_col_id().ok_or_else(|| {
                                self.error_here("ROLLBACK TO requires a savepoint")
                            })?);
                        stmt.location = location as ParseLoc;
                    } else {
                        stmt.kind = TransactionStmtKind::Rollback;
                        stmt.chain = self.parse_transaction_chain()?;
                    }
                }
            }
            TokenKind::AbortP => {
                stmt.kind = TransactionStmtKind::Rollback;
                self.consume_opt_transaction();
                stmt.chain = self.parse_transaction_chain()?;
            }
            TokenKind::Savepoint => {
                stmt.kind = TransactionStmtKind::Savepoint;
                let location = self.location();
                stmt.savepoint_name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("SAVEPOINT requires a name"))?,
                );
                stmt.location = location as ParseLoc;
            }
            TokenKind::Release => {
                stmt.kind = TransactionStmtKind::Release;
                self.consume(TokenKind::Savepoint);
                let location = self.location();
                stmt.savepoint_name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RELEASE requires a savepoint"))?,
                );
                stmt.location = location as ParseLoc;
            }
            TokenKind::Prepare => {
                stmt.kind = TransactionStmtKind::Prepare;
                self.expect(TokenKind::Transaction)?;
                let location = self.location();
                stmt.gid =
                    Some(self.consume_required_string("PREPARE TRANSACTION requires a string")?);
                stmt.location = location as ParseLoc;
            }
            _ => return Err(self.error_here("invalid transaction statement")),
        }
        self.expect_statement_end()?;
        Ok(Node::TransactionStmt(stmt))
    }

    fn parse_transaction_modes(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            let option = if self.consume(TokenKind::Isolation) {
                self.expect(TokenKind::Level)?;
                let value_location = self.location();
                let value = if self.consume(TokenKind::Read) {
                    if self.consume(TokenKind::Uncommitted) {
                        "read uncommitted"
                    } else {
                        self.expect(TokenKind::Committed)?;
                        "read committed"
                    }
                } else if self.consume(TokenKind::Repeatable) {
                    self.expect(TokenKind::Read)?;
                    "repeatable read"
                } else if self.consume(TokenKind::Serializable) {
                    "serializable"
                } else {
                    return Err(self.error_here("invalid transaction isolation level"));
                };
                make_def_elem(
                    "transaction_isolation",
                    Some(Node::AConst(AConst::string(
                        value,
                        value_location as ParseLoc,
                    ))),
                    location,
                )
            } else if self.consume(TokenKind::Read) {
                let read_only = if self.consume(TokenKind::Only) {
                    true
                } else if self.consume(TokenKind::Write) {
                    false
                } else {
                    return Err(self.error_here("READ requires ONLY or WRITE"));
                };
                make_def_elem(
                    "transaction_read_only",
                    Some(Node::AConst(AConst::integer(
                        i32::from(read_only),
                        location as ParseLoc,
                    ))),
                    location,
                )
            } else if self.consume(TokenKind::Deferrable) {
                make_def_elem(
                    "transaction_deferrable",
                    Some(Node::AConst(AConst::integer(1, location as ParseLoc))),
                    location,
                )
            } else if self.consume(TokenKind::Not) {
                self.expect(TokenKind::Deferrable)?;
                make_def_elem(
                    "transaction_deferrable",
                    Some(Node::AConst(AConst::integer(0, location as ParseLoc))),
                    location,
                )
            } else {
                return Err(self.error_here("invalid transaction mode"));
            };
            options.push(option);
            if self.consume(TokenKind::Char(',')) && self.at_statement_end() {
                return Err(self.error_here("expected a transaction mode after ','"));
            }
        }
        Ok(options)
    }

    fn consume_opt_transaction(&mut self) {
        if !self.consume(TokenKind::Work) {
            self.consume(TokenKind::Transaction);
        }
    }

    fn parse_transaction_chain(&mut self) -> PResult<bool> {
        if !self.consume(TokenKind::And) {
            return Ok(false);
        }
        let no = self.consume(TokenKind::No);
        self.expect(TokenKind::Chain)?;
        Ok(!no)
    }
}
pub(super) fn parse_setting_value_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location);
    if tokens.is_empty() {
        return Err(ParseError::new(location, "SET requires a value"));
    }
    if tokens.len() == 1 {
        if matches!(tokens[0].kind, TokenKind::IConst | TokenKind::FConst) {
            return parse_expression_tokens(tokens);
        }
        let is_setting_word = matches!(
            tokens[0].kind,
            TokenKind::SConst | TokenKind::TrueP | TokenKind::FalseP | TokenKind::On
        ) || token_name_in_categories(
            &tokens[0],
            &[
                KeywordCategory::Unreserved,
                KeywordCategory::ColName,
                KeywordCategory::TypeFuncName,
            ],
        )
        .is_some();
        if is_setting_word && let Some(value) = token_name(&tokens[0]) {
            return Ok(Node::AConst(AConst::string(
                value,
                tokens[0].location as ParseLoc,
            )));
        }
    }
    if matches!(
        tokens.as_slice(),
        [
            Token {
                kind: TokenKind::Char('+') | TokenKind::Char('-'),
                ..
            },
            Token {
                kind: TokenKind::IConst | TokenKind::FConst,
                ..
            }
        ]
    ) {
        return parse_expression_tokens(tokens);
    }
    Err(ParseError::new(location, "invalid SET value"))
}

pub(super) fn parse_time_zone_value_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location);
    if tokens.len() == 1 {
        if matches!(
            tokens[0].kind,
            TokenKind::SConst
                | TokenKind::Ident
                | TokenKind::UIdent
                | TokenKind::IConst
                | TokenKind::FConst
        ) {
            return parse_setting_value_tokens(tokens);
        }
        return Err(ParseError::new(location, "invalid time zone value"));
    }
    if tokens.first().map(|token| token.kind) != Some(TokenKind::Interval) {
        return Err(ParseError::new(location, "invalid time zone value"));
    }
    if tokens.get(1).map(|token| token.kind) != Some(TokenKind::Char('(')) {
        let string_index = tokens
            .iter()
            .position(|token| token.kind == TokenKind::SConst)
            .ok_or_else(|| ParseError::new(location, "time zone interval requires a string"))?;
        let qualifier = &tokens[string_index + 1..];
        if !matches!(
            qualifier,
            [] | [Token {
                kind: TokenKind::HourP,
                ..
            }] | [
                Token {
                    kind: TokenKind::HourP,
                    ..
                },
                Token {
                    kind: TokenKind::To,
                    ..
                },
                Token {
                    kind: TokenKind::MinuteP,
                    ..
                }
            ]
        ) {
            return Err(ParseError::new(
                qualifier.first().map_or(location, |token| token.location),
                "time zone interval must be HOUR or HOUR TO MINUTE",
            ));
        }
    }
    parse_expression_tokens(tokens)
}
