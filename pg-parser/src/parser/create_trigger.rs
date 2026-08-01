use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TriggerKind {
    Regular,
    Constraint,
}

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createeventtrigger.html
    // CREATE EVENT TRIGGER name
    //     ON event
    //     [ WHEN filter_variable IN (filter_value [, ... ]) [ AND ... ] ]
    //     EXECUTE { FUNCTION | PROCEDURE } function_name()
    pub(super) fn parse_create_event_trigger(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Trigger)?;
        self.record_completion_slot(completion::GrammarSlot::EventTrigger);
        let trigname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE EVENT TRIGGER requires a name"))?,
        );
        self.expect(TokenKind::On)?;
        self.record_completion_slot(completion::GrammarSlot::AnyName);
        let eventname = Some(
            self.consume_col_label()
                .ok_or_else(|| self.error_here("event trigger requires an event name"))?,
        );
        let mut whenclause = Vec::new();
        if self.consume(TokenKind::When) {
            loop {
                let location = self.location();
                self.record_completion_slot(completion::GrammarSlot::AnyName);
                let name = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("event trigger WHEN requires a variable"))?;
                self.expect(TokenKind::InP)?;
                self.expect(TokenKind::Char('('))?;
                let mut values = Vec::new();
                loop {
                    values.push(make_string_node(
                        self.consume_required_string("event trigger values must be strings")?,
                    ));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                whenclause.push(make_def_elem(&name, Some(name_list_node(values)), location));
                if !self.consume(TokenKind::And) {
                    break;
                }
            }
        }
        self.expect(TokenKind::Execute)?;
        if !self.consume(TokenKind::Function) {
            self.expect(TokenKind::Procedure)?;
        }
        self.record_completion_slot(completion::GrammarSlot::Function);
        let funcname = self.parse_func_name_list();
        if funcname.is_empty() {
            return Err(self.error_here("event trigger function requires a name"));
        }
        self.expect(TokenKind::Char('('))?;
        self.expect(TokenKind::Char(')'))?;
        Ok(Node::CreateEventTrigStmt(CreateEventTrigStmt {
            node_tag: NodeTag::CreateEventTrigStmt,
            trigname,
            eventname,
            whenclause,
            funcname,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altereventtrigger.html
    // ALTER EVENT TRIGGER name DISABLE
    // ALTER EVENT TRIGGER name ENABLE [ REPLICA | ALWAYS ]
    // ALTER EVENT TRIGGER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER EVENT TRIGGER name RENAME TO new_name
    pub(super) fn parse_alter_event_trigger(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Trigger)?;
        self.record_completion_slot(completion::GrammarSlot::EventTrigger);
        let trigname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER EVENT TRIGGER requires a name"))?,
        );
        self.record_completion_tokens(&[
            TokenKind::EnableP,
            TokenKind::DisableP,
            TokenKind::Rename,
            TokenKind::Owner,
        ]);
        let tgenabled = match self.peek_kind() {
            TokenKind::EnableP => {
                self.advance();
                if self.consume(TokenKind::Replica) {
                    b'R'
                } else if self.consume(TokenKind::Always) {
                    b'A'
                } else {
                    b'O'
                }
            }
            TokenKind::DisableP => {
                self.advance();
                b'D'
            }
            _ => return Err(self.error_here("event trigger requires ENABLE or DISABLE")),
        };
        self.expect_statement_end()?;
        Ok(Node::AlterEventTrigStmt(AlterEventTrigStmt {
            node_tag: NodeTag::AlterEventTrigStmt,
            trigname,
            tgenabled,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtrigger.html
    // CREATE [ OR REPLACE ] [ CONSTRAINT ] TRIGGER name { BEFORE | AFTER | INSTEAD OF } { event [ OR ... ] }
    //     ON table_name
    //     [ FROM referenced_table_name ]
    //     [ NOT DEFERRABLE | [ DEFERRABLE ] [ INITIALLY IMMEDIATE | INITIALLY DEFERRED ] ]
    //     [ REFERENCING { { OLD | NEW } TABLE [ AS ] transition_relation_name } [ ... ] ]
    //     [ FOR [ EACH ] { ROW | STATEMENT } ]
    //     [ WHEN ( condition ) ]
    //     EXECUTE { FUNCTION | PROCEDURE } function_name ( arguments )
    //
    // where event can be one of:
    //
    //     INSERT
    //     UPDATE [ OF column_name [, ... ] ]
    //     DELETE
    //     TRUNCATE
    pub(super) fn parse_create_trigger(
        &mut self,
        replace: bool,
        kind: TriggerKind,
    ) -> PResult<Node> {
        let is_constraint = kind == TriggerKind::Constraint;
        self.expect(TokenKind::Trigger)?;
        if is_constraint && replace {
            return Err(self.error_here("OR REPLACE is not supported for constraint triggers"));
        }
        self.record_completion_slot(completion::GrammarSlot::Trigger);
        let trigname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE TRIGGER requires a name"))?,
        );
        let timing = if is_constraint {
            self.expect(TokenKind::After)?;
            0
        } else {
            self.record_completion_tokens(&[
                TokenKind::Before,
                TokenKind::After,
                TokenKind::Instead,
            ]);
            match self.peek_kind() {
                TokenKind::Before => {
                    self.advance();
                    2
                }
                TokenKind::After => {
                    self.advance();
                    0
                }
                TokenKind::Instead => {
                    self.advance();
                    self.expect(TokenKind::Of)?;
                    64
                }
                _ => {
                    return Err(
                        self.error_here("trigger timing must be BEFORE, AFTER, or INSTEAD OF")
                    );
                }
            }
        };
        let (events, columns) = self.parse_trigger_events()?;
        self.expect(TokenKind::On)?;
        let owner_start = self.pos;
        let relation = Some(Box::new(
            self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                .ok_or_else(|| self.error_here("CREATE TRIGGER requires a relation"))?,
        ));
        let owner_end = self.pos;
        self.push_completion_membership_owner_range(
            &[completion::GrammarSlot::Column],
            &[
                ObjectType::Table,
                ObjectType::View,
                ObjectType::ForeignTable,
            ],
            owner_start,
            owner_end,
        );
        let constrrel = if is_constraint && self.consume(TokenKind::From) {
            Some(Box::new(
                self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                    .ok_or_else(|| self.error_here("FROM requires a relation"))?,
            ))
        } else {
            None
        };
        let mut transition_rels = Vec::new();
        if !is_constraint && self.consume(TokenKind::Referencing) {
            let transition_start = self.location();
            loop {
                self.record_completion_lookahead_tokens(&[TokenKind::Old, TokenKind::New]);
                if !matches!(self.peek_kind(), TokenKind::Old | TokenKind::New) {
                    break;
                }
                let is_new = self.consume(TokenKind::New);
                if !is_new {
                    self.expect(TokenKind::Old)?;
                }
                let is_table = if self.consume(TokenKind::Table) {
                    true
                } else {
                    self.expect(TokenKind::Row)?;
                    false
                };
                self.consume(TokenKind::As);
                let name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("trigger transition requires a name"))?,
                );
                transition_rels.push(Node::TriggerTransition(TriggerTransition {
                    node_tag: NodeTag::TriggerTransition,
                    name,
                    is_new,
                    is_table,
                }));
            }
            if transition_rels.is_empty() {
                return Err(ParseError::syntax_exit(
                    transition_start,
                    "REFERENCING requires at least one transition relation",
                ));
            }
        }
        let mut deferrable = false;
        let mut initdeferred = false;
        if is_constraint {
            let mut saw_deferrable = None;
            let mut saw_initially = None;
            loop {
                self.record_completion_lookahead_tokens(&[
                    TokenKind::Deferrable,
                    TokenKind::Not,
                    TokenKind::Initially,
                    TokenKind::Enforced,
                ]);
                match self.peek_kind() {
                    TokenKind::Deferrable => {
                        self.advance();
                        if saw_deferrable == Some(false) {
                            return Err(self.error_here("conflicting constraint properties"));
                        }
                        saw_deferrable = Some(true);
                        deferrable = true;
                    }
                    TokenKind::Not => {
                        self.advance();
                        self.expect(TokenKind::Deferrable)?;
                        if saw_deferrable == Some(true) {
                            return Err(self.error_here("conflicting constraint properties"));
                        }
                        if saw_initially == Some(true) {
                            return Err(self.error_here(
                                "constraint declared INITIALLY DEFERRED must be DEFERRABLE",
                            ));
                        }
                        saw_deferrable = Some(false);
                        deferrable = false;
                    }
                    TokenKind::Initially => {
                        self.advance();
                        let deferred = if self.consume(TokenKind::Deferred) {
                            true
                        } else {
                            self.expect(TokenKind::Immediate)?;
                            false
                        };
                        if saw_initially.is_some_and(|previous| previous != deferred) {
                            return Err(self.error_here("conflicting constraint properties"));
                        }
                        saw_initially = Some(deferred);
                        initdeferred = deferred;
                        if deferred {
                            if saw_deferrable == Some(false) {
                                return Err(self.error_here(
                                    "constraint declared INITIALLY DEFERRED must be DEFERRABLE",
                                ));
                            }
                            deferrable = true;
                        }
                    }
                    TokenKind::Enforced => {
                        self.advance();
                        // Accepted by ConstraintAttributeSpec; CreateTrigStmt has no raw field for it.
                    }
                    _ => break,
                }
            }
        }
        let row = if self.consume(TokenKind::For) {
            self.consume(TokenKind::Each);
            if self.consume(TokenKind::Row) {
                true
            } else {
                self.expect(TokenKind::Statement)?;
                false
            }
        } else {
            if is_constraint {
                return Err(self.error_here("constraint trigger requires FOR EACH ROW"));
            }
            false
        };
        let when_clause = if self.consume(TokenKind::When) {
            self.expect(TokenKind::Char('('))?;
            let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
            self.expect(TokenKind::Char(')'))?;
            Some(expr)
        } else {
            None
        };
        self.expect(TokenKind::Execute)?;
        if !self.consume(TokenKind::Function) {
            self.expect(TokenKind::Procedure)?;
        }
        self.record_completion_slot(completion::GrammarSlot::Function);
        let funcname = self.parse_func_name_list();
        if funcname.is_empty() {
            return Err(self.error_here("trigger function requires a name"));
        }
        self.expect(TokenKind::Char('('))?;
        let mut args = Vec::new();
        if !self.at(TokenKind::Char(')')) {
            loop {
                self.record_completion_tokens(&[
                    TokenKind::IConst,
                    TokenKind::FConst,
                    TokenKind::SConst,
                    TokenKind::Char(')'),
                ]);
                self.record_completion_slot(completion::GrammarSlot::AnyName);
                let token = self.peek().clone();
                let value = match (&token.kind, &token.value) {
                    (TokenKind::IConst, Some(TokenValue::Integer(value))) => {
                        self.advance();
                        value.to_string()
                    }
                    (TokenKind::FConst | TokenKind::SConst, Some(TokenValue::String(value))) => {
                        self.advance();
                        value.clone()
                    }
                    _ => self
                        .consume_col_label()
                        .ok_or_else(|| self.error_here("invalid trigger function argument"))?,
                };
                args.push(make_string_node(value));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(Node::CreateTrigStmt(CreateTrigStmt {
            node_tag: NodeTag::CreateTrigStmt,
            replace,
            isconstraint: is_constraint,
            trigname,
            relation,
            funcname,
            args,
            row,
            timing,
            events,
            columns,
            when_clause,
            transition_rels,
            deferrable,
            initdeferred,
            constrrel,
        }))
    }
}
