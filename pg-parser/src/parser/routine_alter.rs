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
        let action_starts = Self::alter_function_action_starts();
        let func = Some(Box::new(self.parse_object_with_args_until(&action_starts)?));
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

    fn alter_function_action_starts() -> [TokenKind; 19] {
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
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]
    }

    fn parse_alter_function_actions(&mut self) -> PResult<NodeList> {
        let mut actions = Vec::new();
        while !self.at_any(&[TokenKind::Restrict, TokenKind::Char(';'), TokenKind::Eof]) {
            let location = self.location();
            let (name, arg) = if self.consume(TokenKind::Called) {
                self.expect(TokenKind::On)?;
                self.expect(TokenKind::NullP)?;
                self.expect(TokenKind::InputP)?;
                ("strict", Some(Node::Boolean(Boolean::new(false))))
            } else if self.consume(TokenKind::Returns) {
                self.expect(TokenKind::NullP)?;
                self.expect(TokenKind::On)?;
                self.expect(TokenKind::NullP)?;
                self.expect(TokenKind::InputP)?;
                ("strict", Some(Node::Boolean(Boolean::new(true))))
            } else if self.consume(TokenKind::StrictP) {
                ("strict", Some(Node::Boolean(Boolean::new(true))))
            } else if self.consume(TokenKind::Immutable) {
                ("volatility", Some(make_string_node("immutable")))
            } else if self.consume(TokenKind::Stable) {
                ("volatility", Some(make_string_node("stable")))
            } else if self.consume(TokenKind::Volatile) {
                ("volatility", Some(make_string_node("volatile")))
            } else if self.consume(TokenKind::External) {
                self.expect(TokenKind::Security)?;
                let value = if self.consume(TokenKind::Definer) {
                    true
                } else if self.consume(TokenKind::Invoker) {
                    false
                } else {
                    return Err(self.error_here("SECURITY requires DEFINER or INVOKER"));
                };
                ("security", Some(Node::Boolean(Boolean::new(value))))
            } else if self.consume(TokenKind::Security) {
                let value = if self.consume(TokenKind::Definer) {
                    true
                } else if self.consume(TokenKind::Invoker) {
                    false
                } else {
                    return Err(self.error_here("SECURITY requires DEFINER or INVOKER"));
                };
                ("security", Some(Node::Boolean(Boolean::new(value))))
            } else if self.consume(TokenKind::Leakproof) {
                ("leakproof", Some(Node::Boolean(Boolean::new(true))))
            } else if self.consume(TokenKind::Not) {
                self.expect(TokenKind::Leakproof)?;
                ("leakproof", Some(Node::Boolean(Boolean::new(false))))
            } else if self.consume(TokenKind::Cost) {
                ("cost", Some(self.parse_numeric_only()?))
            } else if self.consume(TokenKind::Rows) {
                ("rows", Some(self.parse_numeric_only()?))
            } else if self.consume(TokenKind::Support) {
                let names =
                    self.parse_name_list_until_keywords(&Self::alter_function_action_starts());
                if names.is_empty() {
                    return Err(self.error_here("SUPPORT requires a function name"));
                }
                ("support", Some(name_list_node(names)))
            } else if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
                let action_starts = Self::alter_function_action_starts();
                let setstmt = self.parse_function_set_reset_clause_until(&action_starts)?;
                ("set", Some(Node::VariableSetStmt(setstmt)))
            } else if self.consume(TokenKind::Parallel) {
                let value = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("PARALLEL requires a mode"))?;
                ("parallel", Some(make_string_node(value)))
            } else {
                return Err(self.error_here("invalid ALTER FUNCTION option"));
            };
            actions.push(make_def_elem(name, arg, location));
        }
        Ok(actions)
    }

    pub(super) fn parse_function_set_reset_clause_until(
        &mut self,
        stops: &[TokenKind],
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
        if self.consume(TokenKind::Schema) {
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
            let args = self.parse_setting_value_list_until(stops)?;
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

    fn parse_setting_value_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut args = Vec::new();
        loop {
            let chunk_stops = extend_stops(stops, TokenKind::Char(','));
            let tokens = self.take_until_top_level(&chunk_stops);
            args.push(parse_setting_value_tokens(tokens)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a SET value after ','"));
            }
        }
        Ok(args)
    }
}
