//! Session settings and transaction-control statements.
//!
//! `SET`, `RESET`, `SHOW`, and transaction modes share value-fragment parsers but
//! retain statement-specific normalization and completion behavior.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-set.html
    // SET [ SESSION | LOCAL ] configuration_parameter { TO | = } { value | 'value' | DEFAULT }
    // SET [ SESSION | LOCAL ] TIME ZONE { value | 'value' | LOCAL | DEFAULT }
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
    pub(super) fn parse_variable_set(&mut self) -> PResult<Node> {
        Ok(Node::VariableSetStmt(self.parse_variable_set_like(true)?))
    }

    pub(super) fn parse_variable_set_like(
        &mut self,
        allow_scope: bool,
    ) -> PResult<VariableSetStmt> {
        match self.peek_kind() {
            TokenKind::Reset => {
                self.advance();
                self.record_completion_tokens(&[
                    TokenKind::All,
                    TokenKind::Time,
                    TokenKind::Transaction,
                    TokenKind::Session,
                ]);
                self.record_completion_slot(GrammarSlot::AnyName);
                let (kind, name) =
                    match self.peek_kind() {
                        TokenKind::All => {
                            self.advance();
                            (VariableSetKind::ResetAll, None)
                        }
                        TokenKind::Time => {
                            self.advance();
                            self.expect(TokenKind::Zone)?;
                            (VariableSetKind::Reset, Some("timezone".to_owned()))
                        }
                        TokenKind::Transaction => {
                            self.advance();
                            self.expect(TokenKind::Isolation)?;
                            self.expect(TokenKind::Level)?;
                            (
                                VariableSetKind::Reset,
                                Some("transaction_isolation".to_owned()),
                            )
                        }
                        TokenKind::Session => {
                            self.advance();
                            self.expect(TokenKind::Authorization)?;
                            (
                                VariableSetKind::Reset,
                                Some("session_authorization".to_owned()),
                            )
                        }
                        _ => (
                            VariableSetKind::Reset,
                            Some(self.consume_setting_name().ok_or_else(|| {
                                self.error_here("RESET requires a parameter name")
                            })?),
                        ),
                    };
                self.expect_statement_end()?;
                return Ok(VariableSetStmt {
                    kind,
                    name,
                    parse_loc: -1,
                    ..VariableSetStmt::default()
                });
            }
            TokenKind::Set => {
                self.advance();
            }
            _ => return Err(self.error_here("expected SET or RESET")),
        }

        self.record_completion_tokens(&[
            TokenKind::Local,
            TokenKind::Session,
            TokenKind::Transaction,
            TokenKind::Time,
            TokenKind::Schema,
            TokenKind::Names,
            TokenKind::Role,
            TokenKind::XmlP,
        ]);
        self.record_completion_slot(GrammarSlot::AnyName);
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
            kind: VariableSetKind::SetValue,
            is_local,
            parse_loc: -1,
            ..VariableSetStmt::default()
        };
        if self.peek_kind() == TokenKind::Transaction
            && self.peek_kind_n(1) == TokenKind::Completion
        {
            self.advance();
            self.record_completion_tokens(&[
                TokenKind::Snapshot,
                TokenKind::Isolation,
                TokenKind::Read,
                TokenKind::Deferrable,
                TokenKind::Not,
            ]);
            return Err(self.error_here("completion point after SET TRANSACTION"));
        }
        match self.peek_kind() {
            TokenKind::Transaction if self.peek_kind_n(1) != TokenKind::Snapshot => {
                self.advance();
                stmt.kind = VariableSetKind::SetMulti;
                stmt.name = Some("TRANSACTION".to_owned());
                stmt.args = self.parse_transaction_modes()?;
                if stmt.args.is_empty() {
                    return Err(self.error_here("SET TRANSACTION requires at least one mode"));
                }
                stmt.jumble_args = true;
            }
            TokenKind::Session if self.peek_kind_n(1) == TokenKind::Characteristics => {
                self.advance();
                self.expect(TokenKind::Characteristics)?;
                self.expect(TokenKind::As)?;
                self.expect(TokenKind::Transaction)?;
                stmt.kind = VariableSetKind::SetMulti;
                stmt.name = Some("SESSION CHARACTERISTICS".to_owned());
                stmt.args = self.parse_transaction_modes()?;
                if stmt.args.is_empty() {
                    return Err(
                        self.error_here("SESSION CHARACTERISTICS requires transaction modes")
                    );
                }
                stmt.jumble_args = true;
            }
            TokenKind::Time => {
                self.advance();
                self.expect(TokenKind::Zone)?;
                stmt.name = Some("timezone".to_owned());
                if self.consume(TokenKind::Default) || self.consume(TokenKind::Local) {
                    stmt.kind = VariableSetKind::SetDefault;
                } else {
                    self.record_completion_tokens(&[
                        TokenKind::Default,
                        TokenKind::Local,
                        TokenKind::Interval,
                    ]);
                    self.record_completion_slot(GrammarSlot::AnyName);
                    let tokens = self.take_until_top_level(STATEMENT_END_TOKENS);
                    if self.at_completion() && tokens.first().has_kind(TokenKind::Interval) {
                        let qualifier = tokens
                            .iter()
                            .position(|token| token.kind == TokenKind::SConst)
                            .map_or(&[][..], |string_index| &tokens[string_index + 1..]);
                        match qualifier {
                            [] if tokens.iter().any(|token| token.kind == TokenKind::SConst) => {
                                self.record_completion_lookahead_tokens(&[TokenKind::HourP]);
                            }
                            [
                                Token {
                                    kind: TokenKind::HourP,
                                    ..
                                },
                            ] => self.record_completion_lookahead_tokens(&[TokenKind::To]),
                            [
                                Token {
                                    kind: TokenKind::HourP,
                                    ..
                                },
                                Token {
                                    kind: TokenKind::To,
                                    ..
                                },
                            ] => self.record_completion_tokens(&[TokenKind::MinuteP]),
                            _ => {}
                        }
                    }
                    stmt.args = vec![parse_time_zone_value_tokens(tokens)?];
                }
                stmt.jumble_args = true;
            }
            TokenKind::Schema if self.peek_kind_n(1) == TokenKind::SConst => {
                self.advance();
                stmt.name = Some("search_path".to_owned());
                stmt.args = vec![node!(AConst::string(
                    self.consume_required_string("SET SCHEMA requires a string")?,
                    self.previous_offset() as ParseLoc,
                ))];
                stmt.parse_loc = self.previous_offset() as ParseLoc;
            }
            TokenKind::Names => {
                self.advance();
                stmt.name = Some("client_encoding".to_owned());
                if self.consume(TokenKind::Default) {
                    stmt.kind = VariableSetKind::SetDefault;
                    stmt.parse_loc = self.previous_offset() as ParseLoc;
                } else if self.at_statement_end() {
                    stmt.kind = VariableSetKind::SetDefault;
                } else {
                    let value = self.consume_required_string("SET NAMES requires an encoding")?;
                    stmt.args = vec![node!(AConst::string(
                        value,
                        self.previous_offset() as ParseLoc,
                    ))];
                    stmt.parse_loc = self.previous_offset() as ParseLoc;
                }
            }
            TokenKind::Role => {
                self.advance();
                stmt.name = Some("role".to_owned());
                self.record_completion_slot(GrammarSlot::Role);
                let value = self
                    .consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("SET ROLE requires a role"))?;
                stmt.args = vec![node!(AConst::string(
                    value,
                    self.previous_offset() as ParseLoc,
                ))];
                stmt.parse_loc = self.previous_offset() as ParseLoc;
            }
            TokenKind::Session => {
                self.advance();
                self.expect(TokenKind::Authorization)?;
                stmt.name = Some("session_authorization".to_owned());
                if self.consume(TokenKind::Default) {
                    stmt.kind = VariableSetKind::SetDefault;
                } else {
                    self.record_completion_slot(GrammarSlot::Role);
                    let value = self.consume_non_reserved_word_or_sconst().ok_or_else(|| {
                        self.error_here("SET SESSION AUTHORIZATION requires a role")
                    })?;
                    stmt.args = vec![node!(AConst::string(
                        value,
                        self.previous_offset() as ParseLoc,
                    ))];
                    stmt.parse_loc = self.previous_offset() as ParseLoc;
                }
            }
            TokenKind::XmlP => {
                self.advance();
                self.expect(TokenKind::Option)?;
                self.record_completion_tokens(&[TokenKind::DocumentP, TokenKind::ContentP]);
                let (value, value_parse_loc) = match self.peek_kind() {
                    TokenKind::DocumentP => ("DOCUMENT", self.advance().offset() as ParseLoc),
                    TokenKind::ContentP => ("CONTENT", self.advance().offset() as ParseLoc),
                    _ => return Err(self.error_here("XML OPTION requires DOCUMENT or CONTENT")),
                };
                stmt.name = Some("xmloption".to_owned());
                stmt.args = vec![node!(AConst::string(value, value_parse_loc))];
                stmt.jumble_args = true;
            }
            TokenKind::Transaction => {
                self.advance();
                self.expect(TokenKind::Snapshot)?;
                stmt.kind = VariableSetKind::SetMulti;
                stmt.name = Some("TRANSACTION SNAPSHOT".to_owned());
                stmt.args = vec![node!(AConst::string(
                    self.consume_required_string("TRANSACTION SNAPSHOT requires a string")?,
                    self.previous_offset() as ParseLoc,
                ))];
                stmt.parse_loc = self.previous_offset() as ParseLoc;
            }
            _ => {
                stmt.name = Some(
                    self.consume_setting_name()
                        .ok_or_else(|| self.error_here("SET requires a parameter name"))?,
                );
                self.record_completion_tokens(&[TokenKind::To, TokenKind::Char('=')]);
                if self.consume(TokenKind::From) {
                    self.expect(TokenKind::CurrentP)?;
                    stmt.kind = VariableSetKind::SetCurrent;
                } else {
                    if !self.consume(TokenKind::To) && !self.consume(TokenKind::Char('=')) {
                        return Err(
                            self.error_here("SET parameter requires TO, '=', or FROM CURRENT")
                        );
                    }
                    let value_parse_loc = self.offset() as ParseLoc;
                    match self.peek_kind() {
                        TokenKind::Default => {
                            self.advance();
                            stmt.kind = VariableSetKind::SetDefault;
                            stmt.parse_loc = -1;
                        }
                        TokenKind::NullP => {
                            let parse_loc = self.advance().offset() as ParseLoc;
                            stmt.args = vec![node!(AConst::null(parse_loc))];
                            stmt.parse_loc = value_parse_loc;
                        }
                        _ => {
                            stmt.args = self.parse_setting_value_list()?;
                            stmt.parse_loc = value_parse_loc;
                        }
                    }
                }
            }
        }
        self.expect_statement_end()?;
        Ok(stmt)
    }

    pub(super) fn parse_setting_value_list(&mut self) -> PResult<NodeList> {
        let mut args = Vec::new();
        loop {
            let tokens = self.take_until_top_level(COMMA_OR_STATEMENT_END_TOKENS);
            if self.at_completion() && tokens.is_empty() {
                self.record_completion_tokens(&[TokenKind::Default]);
                self.record_completion_slot(GrammarSlot::AnyName);
            }
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
        self.record_completion_tokens(&[
            TokenKind::All,
            TokenKind::Time,
            TokenKind::Transaction,
            TokenKind::Session,
        ]);
        self.record_completion_slot(GrammarSlot::AnyName);
        let name = Some(match self.peek_kind() {
            TokenKind::All => {
                self.advance();
                "all".to_owned()
            }
            TokenKind::Time => {
                self.advance();
                self.expect(TokenKind::Zone)?;
                "timezone".to_owned()
            }
            TokenKind::Transaction => {
                self.advance();
                self.expect(TokenKind::Isolation)?;
                self.expect(TokenKind::Level)?;
                "transaction_isolation".to_owned()
            }
            TokenKind::Session => {
                self.advance();
                self.expect(TokenKind::Authorization)?;
                "session_authorization".to_owned()
            }
            _ => self
                .consume_setting_name()
                .ok_or_else(|| self.error_here("SHOW requires a parameter name"))?,
        });
        self.expect_statement_end()?;
        Ok(node!(VariableShowStmt { name }))
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
            parse_loc: -1,
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
                    let offset = self.offset();
                    stmt.gid =
                        Some(self.consume_required_string("COMMIT PREPARED requires a string")?);
                    stmt.parse_loc = offset as ParseLoc;
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
                    let offset = self.offset();
                    stmt.gid =
                        Some(self.consume_required_string("ROLLBACK PREPARED requires a string")?);
                    stmt.parse_loc = offset as ParseLoc;
                } else {
                    self.consume_opt_transaction();
                    if self.consume(TokenKind::To) {
                        stmt.kind = TransactionStmtKind::RollbackTo;
                        self.consume(TokenKind::Savepoint);
                        self.record_completion_slot(GrammarSlot::AnyName);
                        let offset = self.offset();
                        stmt.savepoint_name =
                            Some(self.consume_col_id().ok_or_else(|| {
                                self.error_here("ROLLBACK TO requires a savepoint")
                            })?);
                        stmt.parse_loc = offset as ParseLoc;
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
                self.record_completion_slot(GrammarSlot::AnyName);
                let offset = self.offset();
                stmt.savepoint_name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("SAVEPOINT requires a name"))?,
                );
                stmt.parse_loc = offset as ParseLoc;
            }
            TokenKind::Release => {
                stmt.kind = TransactionStmtKind::Release;
                self.consume(TokenKind::Savepoint);
                self.record_completion_slot(GrammarSlot::AnyName);
                let offset = self.offset();
                stmt.savepoint_name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("RELEASE requires a savepoint"))?,
                );
                stmt.parse_loc = offset as ParseLoc;
            }
            TokenKind::Prepare => {
                stmt.kind = TransactionStmtKind::Prepare;
                self.expect(TokenKind::Transaction)?;
                let offset = self.offset();
                stmt.gid =
                    Some(self.consume_required_string("PREPARE TRANSACTION requires a string")?);
                stmt.parse_loc = offset as ParseLoc;
            }
            _ => return Err(self.error_here("invalid transaction statement")),
        }
        self.expect_statement_end()?;
        Ok(Node::TransactionStmt(stmt))
    }

    fn parse_transaction_modes(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            self.record_completion_tokens(&[
                TokenKind::Isolation,
                TokenKind::Read,
                TokenKind::Deferrable,
                TokenKind::Not,
            ]);
            let offset = self.offset();
            let option = match self.peek_kind() {
                TokenKind::Isolation => {
                    self.advance();
                    self.expect(TokenKind::Level)?;
                    self.record_completion_tokens(&[
                        TokenKind::Read,
                        TokenKind::Repeatable,
                        TokenKind::Serializable,
                    ]);
                    let value_offset = self.offset();
                    let value = match self.peek_kind() {
                        TokenKind::Read => {
                            self.advance();
                            if self.consume(TokenKind::Uncommitted) {
                                "read uncommitted"
                            } else {
                                self.expect(TokenKind::Committed)?;
                                "read committed"
                            }
                        }
                        TokenKind::Repeatable => {
                            self.advance();
                            self.expect(TokenKind::Read)?;
                            "repeatable read"
                        }
                        TokenKind::Serializable => {
                            self.advance();
                            "serializable"
                        }
                        _ => return Err(self.error_here("invalid transaction isolation level")),
                    };
                    make_def_elem(
                        "transaction_isolation",
                        Some(node!(AConst::string(value, value_offset as ParseLoc,))),
                        offset,
                    )
                }
                TokenKind::Read => {
                    self.advance();
                    self.record_completion_tokens(&[TokenKind::Only, TokenKind::Write]);
                    let read_only = match self.peek_kind() {
                        TokenKind::Only => true,
                        TokenKind::Write => false,
                        _ => return Err(self.error_here("READ requires ONLY or WRITE")),
                    };
                    self.advance();
                    make_def_elem(
                        "transaction_read_only",
                        Some(node!(AConst::integer(
                            i32::from(read_only),
                            offset as ParseLoc,
                        ))),
                        offset,
                    )
                }
                TokenKind::Deferrable => {
                    self.advance();
                    make_def_elem(
                        "transaction_deferrable",
                        Some(node!(AConst::integer(1, offset as ParseLoc))),
                        offset,
                    )
                }
                TokenKind::Not => {
                    self.advance();
                    self.expect(TokenKind::Deferrable)?;
                    make_def_elem(
                        "transaction_deferrable",
                        Some(node!(AConst::integer(0, offset as ParseLoc))),
                        offset,
                    )
                }
                _ => return Err(self.error_here("invalid transaction mode")),
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
    let offset = tokens.first().offset_or(0);
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(offset, "SET requires a value"));
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
            return Ok(node!(
                AConst::string(value, tokens[0].offset() as ParseLoc,)
            ));
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
    Err(ParseError::syntax_exit(offset, "invalid SET value"))
}

pub(super) fn parse_time_zone_value_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let offset = tokens.first().offset_or(0);
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
        return Err(ParseError::syntax_exit(offset, "invalid time zone value"));
    }
    if !tokens.first().has_kind(TokenKind::Interval) {
        return Err(ParseError::syntax_exit(offset, "invalid time zone value"));
    }
    if !tokens.get(1).has_kind(TokenKind::Char('(')) {
        let string_index = tokens
            .iter()
            .position(|token| token.kind == TokenKind::SConst)
            .ok_or_else(|| {
                ParseError::syntax_exit(offset, "time zone interval requires a string")
            })?;
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
            return Err(ParseError::syntax_exit(
                qualifier.first().offset_or(offset),
                "time zone interval must be HOUR or HOUR TO MINUTE",
            ));
        }
    }
    parse_expression_tokens(tokens)
}
