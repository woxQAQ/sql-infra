//! Routine alteration and routine-scoped setting actions.
//!
//! Function, procedure, routine, and aggregate identities feed a common action
//! parser without erasing their distinct object types.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis subset — routine actions
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-alterfunction.html
    // - https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // - https://www.postgresql.org/docs/18/sql-alterroutine.html
    //
    // ALTER { FUNCTION | PROCEDURE | ROUTINE } name
    //     [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    //
    // where action is one of:
    //     CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT
    //     IMMUTABLE | STABLE | VOLATILE | [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY { INVOKER | DEFINER }
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost | ROWS result_rows | SUPPORT support_function
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter | RESET ALL
    //
    // RENAME, OWNER, SET SCHEMA, and DEPENDS forms are handled by alter_identity.
    pub(super) fn parse_alter_function(&mut self) -> PResult<Node> {
        let objtype = match self.advance().kind {
            TokenKind::Procedure => ObjectType::Procedure,
            TokenKind::Routine => ObjectType::Routine,
            TokenKind::Aggregate => {
                return Err(self.error_here("ALTER AGGREGATE only supports generic ALTER actions"));
            }
            _ => ObjectType::Function,
        };
        let func = Some(Box::new(self.parse_routine_with_args_with_slot(
            completion::object_type_slot(objtype),
        )?));
        let actions = self.parse_alter_function_actions()?;
        if actions.is_empty() {
            return Err(self.error_here("ALTER FUNCTION requires at least one option"));
        }
        self.consume(TokenKind::Restrict);
        self.expect_statement_end()?;
        Ok(Node::AlterFunctionStmt(AlterFunctionStmt {
            node_tag: NodeTag::AlterFunctionStmt,
            objtype,
            func,
            actions,
        }))
    }

    fn alter_function_action_starts() -> [TokenKind; 23] {
        [
            TokenKind::Called,
            TokenKind::Returns,
            TokenKind::StrictP,
            TokenKind::Immutable,
            TokenKind::Stable,
            TokenKind::Volatile,
            TokenKind::External,
            TokenKind::Security,
            TokenKind::Leakproof,
            TokenKind::Not,
            TokenKind::Cost,
            TokenKind::Rows,
            TokenKind::Support,
            TokenKind::Set,
            TokenKind::Reset,
            TokenKind::Parallel,
            TokenKind::Rename,
            TokenKind::Owner,
            TokenKind::Depends,
            TokenKind::No,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]
    }

    fn parse_alter_function_actions(&mut self) -> PResult<NodeList> {
        let mut actions = Vec::new();
        while !self.at_any(&[TokenKind::Restrict, TokenKind::Char(';'), TokenKind::Eof]) {
            self.record_completion_tokens(&[
                TokenKind::Called,
                TokenKind::Returns,
                TokenKind::StrictP,
                TokenKind::Immutable,
                TokenKind::Stable,
                TokenKind::Volatile,
                TokenKind::External,
                TokenKind::Security,
                TokenKind::Leakproof,
                TokenKind::Not,
                TokenKind::Cost,
                TokenKind::Rows,
                TokenKind::Support,
                TokenKind::Set,
                TokenKind::Reset,
                TokenKind::Parallel,
                TokenKind::Rename,
                TokenKind::Owner,
                TokenKind::Depends,
                TokenKind::No,
            ]);
            if !actions.is_empty() {
                self.record_completion_tokens(&[TokenKind::Restrict, TokenKind::Char(';')]);
            }
            let location = self.location();
            let (name, arg) = match self.peek_kind() {
                TokenKind::Called => {
                    self.advance();
                    self.expect(TokenKind::On)?;
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::InputP)?;
                    ("strict", Some(Node::Boolean(Boolean::new(false))))
                }
                TokenKind::Returns => {
                    self.advance();
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::On)?;
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::InputP)?;
                    ("strict", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::StrictP => {
                    self.advance();
                    ("strict", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::Immutable | TokenKind::Stable | TokenKind::Volatile => {
                    let value = match self.advance().kind {
                        TokenKind::Immutable => "immutable",
                        TokenKind::Stable => "stable",
                        TokenKind::Volatile => "volatile",
                        _ => return Err(self.error_here("invalid volatility option")),
                    };
                    ("volatility", Some(make_string_node(value)))
                }
                TokenKind::External => {
                    self.advance();
                    self.expect(TokenKind::Security)?;
                    let value = self.parse_routine_security()?;
                    ("security", Some(Node::Boolean(Boolean::new(value))))
                }
                TokenKind::Security => {
                    self.advance();
                    let value = self.parse_routine_security()?;
                    ("security", Some(Node::Boolean(Boolean::new(value))))
                }
                TokenKind::Leakproof => {
                    self.advance();
                    ("leakproof", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::Not => {
                    self.advance();
                    self.expect(TokenKind::Leakproof)?;
                    ("leakproof", Some(Node::Boolean(Boolean::new(false))))
                }
                TokenKind::Cost => {
                    self.advance();
                    ("cost", Some(self.parse_numeric_only()?))
                }
                TokenKind::Rows => {
                    self.advance();
                    ("rows", Some(self.parse_numeric_only()?))
                }
                TokenKind::Support => {
                    self.advance();
                    self.record_completion_slot(completion::GrammarSlot::Function);
                    let names = self.parse_name_list();
                    if names.is_empty() {
                        return Err(self.error_here("SUPPORT requires a function name"));
                    }
                    ("support", Some(name_list_node(names)))
                }
                TokenKind::Set | TokenKind::Reset => {
                    let action_starts = Self::alter_function_action_starts();
                    let setstmt = self.parse_function_set_reset_clause_until(&action_starts)?;
                    ("set", Some(Node::VariableSetStmt(setstmt)))
                }
                TokenKind::Parallel => {
                    self.advance();
                    let value = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("PARALLEL requires a mode"))?;
                    ("parallel", Some(make_string_node(value)))
                }
                TokenKind::No => {
                    self.advance();
                    self.expect(TokenKind::Depends)?;
                    return Err(self.error_here("NO DEPENDS is handled by generic ALTER"));
                }
                _ => return Err(self.error_here("invalid ALTER FUNCTION option")),
            };
            actions.push(make_def_elem(name, arg, location));
        }
        Ok(actions)
    }

    fn parse_routine_security(&mut self) -> PResult<bool> {
        self.record_completion_tokens(&[TokenKind::Definer, TokenKind::Invoker]);
        match self.peek_kind() {
            TokenKind::Definer => {
                self.advance();
                Ok(true)
            }
            TokenKind::Invoker => {
                self.advance();
                Ok(false)
            }
            _ => Err(self.error_here("SECURITY requires DEFINER or INVOKER")),
        }
    }

    pub(super) fn parse_function_set_reset_clause_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<VariableSetStmt> {
        if self.consume(TokenKind::Reset) {
            let (kind, name) = match self.peek_kind() {
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
                    Some(
                        self.consume_setting_name()
                            .ok_or_else(|| self.error_here("RESET requires a parameter name"))?,
                    ),
                ),
            };
            return Ok(VariableSetStmt {
                node_tag: NodeTag::VariableSetStmt,
                kind,
                name,
                location: -1,
                ..VariableSetStmt::default()
            });
        }
        self.expect(TokenKind::Set)?;
        let mut stmt = VariableSetStmt {
            node_tag: NodeTag::VariableSetStmt,
            kind: VariableSetKind::SetValue,
            location: -1,
            ..VariableSetStmt::default()
        };
        if self.consume(TokenKind::Time) {
            self.expect(TokenKind::Zone)?;
            stmt.name = Some("timezone".to_owned());
            stmt.jumble_args = true;
            if self.consume(TokenKind::Default) || self.consume(TokenKind::Local) {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let tokens = self.take_until_top_level(stops);
                stmt.args = vec![parse_time_zone_value_tokens(tokens)?];
            }
            return Ok(stmt);
        }
        if self.consume(TokenKind::CatalogP) {
            return Err(self.error_here("current database cannot be changed"));
        }
        if self.at(TokenKind::Schema) && self.peek_kind_n(1) == TokenKind::SConst {
            self.advance();
            stmt.name = Some("search_path".to_owned());
            let value = self.consume_required_string("SET SCHEMA requires a string")?;
            stmt.args = vec![Node::AConst(AConst::string(
                value,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
            return Ok(stmt);
        }
        if self.consume(TokenKind::Names) {
            stmt.name = Some("client_encoding".to_owned());
            if self.consume(TokenKind::Default) {
                stmt.kind = VariableSetKind::SetDefault;
                stmt.location = self.previous_location() as ParseLoc;
            } else if self.at_any(stops) {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let value = self
                    .consume_string_like()
                    .ok_or_else(|| self.error_here("SET NAMES requires an encoding"))?;
                stmt.args = vec![Node::AConst(AConst::string(
                    value,
                    self.previous_location() as ParseLoc,
                ))];
                stmt.location = self.previous_location() as ParseLoc;
            }
            return Ok(stmt);
        }
        if self.consume(TokenKind::Role) {
            stmt.name = Some("role".to_owned());
            self.record_completion_slot(completion::GrammarSlot::Role);
            let value = self
                .consume_string_like()
                .ok_or_else(|| self.error_here("SET ROLE requires a role"))?;
            stmt.args = vec![Node::AConst(AConst::string(
                value,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
            return Ok(stmt);
        }
        if self.consume(TokenKind::Session) {
            self.expect(TokenKind::Authorization)?;
            stmt.name = Some("session_authorization".to_owned());
            if self.consume(TokenKind::Default) {
                stmt.kind = VariableSetKind::SetDefault;
            } else {
                let value = self
                    .consume_string_like()
                    .ok_or_else(|| self.error_here("SET SESSION AUTHORIZATION requires a role"))?;
                stmt.args = vec![Node::AConst(AConst::string(
                    value,
                    self.previous_location() as ParseLoc,
                ))];
                stmt.location = self.previous_location() as ParseLoc;
            }
            return Ok(stmt);
        }
        if self.consume(TokenKind::XmlP) {
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
            return Ok(stmt);
        }
        if self.consume(TokenKind::Transaction) {
            self.expect(TokenKind::Snapshot)?;
            stmt.kind = VariableSetKind::SetMulti;
            stmt.name = Some("TRANSACTION SNAPSHOT".to_owned());
            let value = self.consume_required_string("TRANSACTION SNAPSHOT requires a string")?;
            stmt.args = vec![Node::AConst(AConst::string(
                value,
                self.previous_location() as ParseLoc,
            ))];
            stmt.location = self.previous_location() as ParseLoc;
            return Ok(stmt);
        }
        let name = Some(
            self.consume_setting_name()
                .ok_or_else(|| self.error_here("SET requires a parameter name"))?,
        );
        self.record_completion_tokens(&[TokenKind::To, TokenKind::Char('=')]);
        if self.consume(TokenKind::From) {
            self.expect(TokenKind::CurrentP)?;
            return Ok(VariableSetStmt {
                node_tag: NodeTag::VariableSetStmt,
                kind: VariableSetKind::SetCurrent,
                name,
                location: -1,
                ..VariableSetStmt::default()
            });
        }
        if !self.consume(TokenKind::To) && !self.consume(TokenKind::Char('=')) {
            return Err(self.error_here("SET parameter requires TO, '=', or FROM CURRENT"));
        }
        let value_location = self.location() as ParseLoc;
        let (kind, args, location) = if self.consume(TokenKind::Default) {
            (VariableSetKind::SetDefault, Vec::new(), -1)
        } else if self.consume(TokenKind::NullP) {
            (
                VariableSetKind::SetValue,
                vec![Node::AConst(AConst::null(
                    self.previous_location() as ParseLoc
                ))],
                value_location,
            )
        } else {
            let args = self.parse_function_setting_value_list()?;
            if args.is_empty() {
                return Err(self.error_here("SET parameter requires a value"));
            }
            (VariableSetKind::SetValue, args, value_location)
        };
        Ok(VariableSetStmt {
            node_tag: NodeTag::VariableSetStmt,
            kind,
            name,
            args,
            location,
            ..VariableSetStmt::default()
        })
    }

    fn parse_function_setting_value_list(&mut self) -> PResult<NodeList> {
        let mut args = Vec::new();
        loop {
            if self.at_completion() {
                self.record_completion_tokens(&[TokenKind::Default]);
                self.record_completion_slot(completion::GrammarSlot::AnyName);
            }
            let value_start = self.pos;
            match self.peek_kind() {
                TokenKind::Char('+') | TokenKind::Char('-')
                    if matches!(self.peek_kind_n(1), TokenKind::IConst | TokenKind::FConst) =>
                {
                    self.advance();
                    self.advance();
                }
                TokenKind::IConst
                | TokenKind::FConst
                | TokenKind::SConst
                | TokenKind::TrueP
                | TokenKind::FalseP
                | TokenKind::On => {
                    self.advance();
                }
                _ if self.consume_non_reserved_word().is_some() => {}
                _ => return Err(self.error_here("SET requires a value")),
            }
            args.push(parse_setting_value_tokens(
                self.tokens[value_start..self.pos].to_vec(),
            )?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(args)
    }
}
