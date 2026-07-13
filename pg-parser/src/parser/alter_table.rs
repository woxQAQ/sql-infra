use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis subset — ALTER relation
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-altertable.html
    // - https://www.postgresql.org/docs/18/sql-alterindex.html
    // - https://www.postgresql.org/docs/18/sql-alterview.html
    // - https://www.postgresql.org/docs/18/sql-altermaterializedview.html
    // - https://www.postgresql.org/docs/18/sql-alterforeigntable.html
    //
    // Normalized across the relation-specific ALTER command pages:
    // ALTER { TABLE | INDEX | VIEW | MATERIALIZED VIEW | FOREIGN TABLE }
    //     [ IF EXISTS ] [ ONLY ] name [ * ] action [, ... ]
    //
    // Object-specific actions are parsed by parse_alter_table_cmds.
    pub(super) fn parse_alter_table(&mut self, objtype: ObjectType) -> PResult<Node> {
        self.advance();
        self.parse_alter_table_after_kind(objtype)
    }

    pub(super) fn parse_alter_table_move_all(&mut self, objtype: ObjectType) -> PResult<Node> {
        self.expect(TokenKind::All)?;
        self.expect(TokenKind::InP)?;
        self.expect(TokenKind::Tablespace)?;
        let orig_tablespacename = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected the source tablespace name"))?;
        let mut roles = Vec::new();
        if self.consume(TokenKind::Owned) {
            self.expect(TokenKind::By)?;
            loop {
                let role = self
                    .consume_role_spec()
                    .ok_or_else(|| self.error_here("expected a role after OWNED BY"))?;
                roles.push(Node::RoleSpec(role));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
        }
        self.expect(TokenKind::Set)?;
        self.expect(TokenKind::Tablespace)?;
        let new_tablespacename = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected the destination tablespace name"))?;
        let nowait = self.consume(TokenKind::Nowait);
        Ok(Node::AlterTableMoveAllStmt(AlterTableMoveAllStmt {
            node_tag: NodeTag::AlterTableMoveAllStmt,
            orig_tablespacename: Some(orig_tablespacename),
            objtype,
            roles,
            new_tablespacename: Some(new_tablespacename),
            nowait,
        }))
    }

    pub(super) fn parse_alter_table_after_kind(&mut self, objtype: ObjectType) -> PResult<Node> {
        let missing_ok = self.consume_if_exists()?;
        let relation = Some(Box::new(self.try_parse_qualified_range_var().ok_or_else(
            || self.error_here("ALTER relation requires a relation name"),
        )?));
        let cmds = self.parse_alter_table_cmds(objtype)?;
        Ok(Node::AlterTableStmt(AlterTableStmt {
            node_tag: NodeTag::AlterTableStmt,
            relation,
            cmds,
            objtype,
            missing_ok,
        }))
    }

    pub(super) fn parse_alter_table_cmds(&mut self, objtype: ObjectType) -> PResult<NodeList> {
        let mut cmds = Vec::new();
        if self.at_statement_end() {
            return Err(self.error_here("ALTER relation requires at least one command"));
        }
        loop {
            cmds.push(Node::AlterTableCmd(self.parse_alter_table_cmd(objtype)?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_statement_end() {
                return Err(self.error_here("expected an ALTER command after ','"));
            }
        }
        self.expect_statement_end()?;
        Ok(cmds)
    }

    fn parse_alter_table_add_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::AddP)?;
        self.consume(TokenKind::Column);
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        if self.at(TokenKind::Constraint)
            || self.at(TokenKind::Check)
            || self.at(TokenKind::Unique)
            || self.at(TokenKind::Primary)
            || self.at(TokenKind::Foreign)
            || self.at(TokenKind::Exclude)
        {
            cmd.subtype = AlterTableType::AddConstraint;
            cmd.def = Some(Box::new(Node::Constraint(self.parse_table_constraint()?)));
        } else {
            cmd.subtype = AlterTableType::AddColumn;
            cmd.missing_ok = self.consume_if_not_exists()?;
            let tokens = self.take_until_top_level(&[
                TokenKind::Char(','),
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            let Node::ColumnDef(column) = parse_table_element_tokens(tokens)? else {
                return Err(self.error_here("ADD COLUMN requires a column definition"));
            };
            cmd.def = Some(Box::new(Node::ColumnDef(column)));
        }
        Ok(cmd)
    }

    fn parse_alter_table_drop_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::Drop)?;
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        if self.consume(TokenKind::Column) {
            cmd.subtype = AlterTableType::DropColumn;
        } else if self.consume(TokenKind::Constraint) {
            cmd.subtype = AlterTableType::DropConstraint;
        } else {
            cmd.subtype = AlterTableType::DropColumn;
        }
        cmd.missing_ok = self.consume_if_exists()?;
        cmd.name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DROP COLUMN/CONSTRAINT requires a name"))?,
        );
        cmd.behavior = self.parse_drop_behavior();
        Ok(cmd)
    }

    fn parse_alter_table_set_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::Set)?;
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        if self.consume(TokenKind::Tablespace) {
            cmd.subtype = AlterTableType::SetTableSpace;
            cmd.name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("SET TABLESPACE requires a name"))?,
            );
        } else if self.consume(TokenKind::Logged) {
            cmd.subtype = AlterTableType::SetLogged;
        } else if self.consume(TokenKind::Unlogged) {
            cmd.subtype = AlterTableType::SetUnLogged;
        } else if self.consume(TokenKind::Access) {
            self.expect(TokenKind::Method)?;
            cmd.subtype = AlterTableType::SetAccessMethod;
            if !self.consume(TokenKind::Default) {
                cmd.name = Some(self.consume_col_id().ok_or_else(|| {
                    self.error_here("SET ACCESS METHOD requires a method name or DEFAULT")
                })?);
            }
        } else if self.consume(TokenKind::Without) {
            if self.consume(TokenKind::Oids) {
                cmd.subtype = AlterTableType::DropOids;
            } else if self.consume(TokenKind::Cluster) {
                cmd.subtype = AlterTableType::DropCluster;
            } else {
                return Err(self.error_here("SET WITHOUT requires OIDS or CLUSTER"));
            }
        } else if self.at(TokenKind::Char('(')) {
            cmd.subtype = AlterTableType::SetRelOptions;
            cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements: self.parse_parenthesized_reloptions()?,
                ..AArrayExpr::default()
            })));
        } else {
            return Err(self.error_here("unsupported ALTER TABLE SET action"));
        }
        Ok(cmd)
    }

    fn parse_alter_table_reset_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::Reset)?;
        Ok(AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            subtype: AlterTableType::ResetRelOptions,
            def: Some(Box::new(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements: self.parse_parenthesized_reloptions()?,
                ..AArrayExpr::default()
            }))),
            ..AlterTableCmd::default()
        })
    }

    fn parse_alter_table_enable_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::EnableP)?;
        let qualifier = if self.consume(TokenKind::Always) {
            Some(TokenKind::Always)
        } else if self.consume(TokenKind::Replica) {
            Some(TokenKind::Replica)
        } else {
            None
        };
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        if self.consume(TokenKind::Trigger) {
            if qualifier.is_some() && matches!(self.peek_kind(), TokenKind::All | TokenKind::User) {
                return Err(self.error_here("ALWAYS/REPLICA TRIGGER requires a trigger name"));
            }
            if qualifier.is_none() && self.consume(TokenKind::All) {
                cmd.subtype = AlterTableType::EnableTrigAll;
            } else if qualifier.is_none() && self.consume(TokenKind::User) {
                cmd.subtype = AlterTableType::EnableTrigUser;
            } else {
                cmd.subtype = match qualifier {
                    Some(TokenKind::Always) => AlterTableType::EnableAlwaysTrig,
                    Some(TokenKind::Replica) => AlterTableType::EnableReplicaTrig,
                    _ => AlterTableType::EnableTrig,
                };
                cmd.name =
                    Some(self.consume_col_id().ok_or_else(|| {
                        self.error_here("ENABLE TRIGGER requires a trigger name")
                    })?);
            }
        } else if self.consume(TokenKind::Rule) {
            cmd.subtype = match qualifier {
                Some(TokenKind::Always) => AlterTableType::EnableAlwaysRule,
                Some(TokenKind::Replica) => AlterTableType::EnableReplicaRule,
                _ => AlterTableType::EnableRule,
            };
            cmd.name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("ENABLE RULE requires a rule name"))?,
            );
        } else if qualifier.is_none() && self.consume(TokenKind::Row) {
            self.expect(TokenKind::Level)?;
            self.expect(TokenKind::Security)?;
            cmd.subtype = AlterTableType::EnableRowSecurity;
        } else {
            return Err(self.error_here("ENABLE requires TRIGGER, RULE, or ROW LEVEL SECURITY"));
        }
        Ok(cmd)
    }

    fn parse_alter_table_disable_cmd(&mut self) -> PResult<AlterTableCmd> {
        self.expect(TokenKind::DisableP)?;
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        if self.consume(TokenKind::Trigger) {
            if self.consume(TokenKind::All) {
                cmd.subtype = AlterTableType::DisableTrigAll;
            } else if self.consume(TokenKind::User) {
                cmd.subtype = AlterTableType::DisableTrigUser;
            } else {
                cmd.subtype = AlterTableType::DisableTrig;
                cmd.name =
                    Some(self.consume_col_id().ok_or_else(|| {
                        self.error_here("DISABLE TRIGGER requires a trigger name")
                    })?);
            }
        } else if self.consume(TokenKind::Rule) {
            cmd.subtype = AlterTableType::DisableRule;
            cmd.name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("DISABLE RULE requires a rule name"))?,
            );
        } else if self.consume(TokenKind::Row) {
            self.expect(TokenKind::Level)?;
            self.expect(TokenKind::Security)?;
            cmd.subtype = AlterTableType::DisableRowSecurity;
        } else {
            return Err(self.error_here("DISABLE requires TRIGGER, RULE, or ROW LEVEL SECURITY"));
        }
        Ok(cmd)
    }

    fn parse_alter_table_cmd(&mut self, objtype: ObjectType) -> PResult<AlterTableCmd> {
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        match self.peek_kind() {
            TokenKind::AddP => cmd = self.parse_alter_table_add_cmd()?,
            TokenKind::Drop => cmd = self.parse_alter_table_drop_cmd()?,
            TokenKind::Alter => {
                self.advance();
                if self.consume(TokenKind::Constraint) {
                    let conname = Some(self.consume_col_id().ok_or_else(|| {
                        self.error_here("ALTER CONSTRAINT requires a constraint name")
                    })?);
                    let mut altered = AtAlterConstraint {
                        node_tag: NodeTag::AtAlterConstraint,
                        conname,
                        ..AtAlterConstraint::default()
                    };
                    if self.consume(TokenKind::Inherit) {
                        altered.alter_inheritability = true;
                    } else {
                        let mut saw_attribute = false;
                        while !self.at_any(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ]) {
                            if self.consume(TokenKind::Deferrable) {
                                altered.alter_deferrability = true;
                                altered.deferrable = true;
                            } else if self.consume(TokenKind::Initially) {
                                altered.alter_deferrability = true;
                                altered.initdeferred = if self.consume(TokenKind::Deferred) {
                                    true
                                } else {
                                    self.expect(TokenKind::Immediate)?;
                                    false
                                };
                            } else if self.consume(TokenKind::Enforced) {
                                altered.alter_enforceability = true;
                                altered.is_enforced = true;
                            } else if self.consume(TokenKind::Not) {
                                if self.consume(TokenKind::Deferrable) {
                                    altered.alter_deferrability = true;
                                    altered.deferrable = false;
                                } else if self.consume(TokenKind::Enforced) {
                                    altered.alter_enforceability = true;
                                    altered.is_enforced = false;
                                } else if self.consume(TokenKind::Valid) {
                                    return Err(self.error_here(
                                        "constraints cannot be altered to be NOT VALID",
                                    ));
                                } else {
                                    return Err(
                                        self.error_here("NOT requires DEFERRABLE or ENFORCED")
                                    );
                                }
                            } else if self.consume(TokenKind::No) {
                                self.expect(TokenKind::Inherit)?;
                                altered.alter_inheritability = true;
                                altered.noinherit = true;
                            } else {
                                return Err(self.error_here("invalid ALTER CONSTRAINT attribute"));
                            }
                            saw_attribute = true;
                        }
                        if !saw_attribute {
                            return Err(
                                self.error_here("ALTER CONSTRAINT requires a constraint attribute")
                            );
                        }
                    }
                    cmd.subtype = AlterTableType::AlterConstraint;
                    cmd.def = Some(Box::new(Node::AtAlterConstraint(altered)));
                    return Ok(cmd);
                }
                self.consume(TokenKind::Column);
                let column_location = self.location();
                if self.at(TokenKind::IConst) {
                    let token = self.advance().clone();
                    let Some(TokenValue::Integer(value)) = token.value else {
                        return Err(ParseError::ranged(token.range, "expected a column number"));
                    };
                    if value <= 0 || value > i32::from(i16::MAX) {
                        return Err(ParseError::ranged(
                            token.range,
                            "column number must be in range from 1 to 32767",
                        ));
                    }
                    cmd.num = value as i16;
                } else {
                    cmd.name =
                        Some(self.consume_col_id().ok_or_else(|| {
                            self.error_here("ALTER COLUMN requires a column name")
                        })?);
                }

                let saw_set = self.consume(TokenKind::Set);
                let set_data_type = if saw_set {
                    if self.consume(TokenKind::DataP) {
                        self.expect(TokenKind::TypeP)?;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if set_data_type || self.consume(TokenKind::TypeP) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    cmd.subtype = AlterTableType::AlterColumnType;
                    let type_name = self
                        .parse_type_name_until(&[
                            TokenKind::Collate,
                            TokenKind::Using,
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ])
                        .ok_or_else(|| self.error_here("ALTER COLUMN TYPE requires a type"))?;
                    let coll_clause = if self.consume(TokenKind::Collate) {
                        let location = self.previous_location();
                        let collname = self.parse_name_list();
                        if collname.is_empty() {
                            return Err(self.error_here("COLLATE requires a collation name"));
                        }
                        Some(Box::new(CollateClause {
                            node_tag: NodeTag::CollateClause,
                            collname,
                            location: location as ParseLoc,
                            ..CollateClause::default()
                        }))
                    } else {
                        None
                    };
                    let raw_default = if self.consume(TokenKind::Using) {
                        Some(self.parse_expr_box_strict_until(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ])?)
                    } else {
                        None
                    };
                    cmd.def = Some(Box::new(Node::ColumnDef(ColumnDef {
                        node_tag: NodeTag::ColumnDef,
                        type_name: Some(Box::new(type_name)),
                        coll_clause,
                        raw_default,
                        location: column_location as ParseLoc,
                        ..ColumnDef::default()
                    })));
                } else if saw_set {
                    if self.consume(TokenKind::Default) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        cmd.subtype = AlterTableType::ColumnDefault;
                        cmd.def = Some(self.parse_expr_box_strict_until(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ])?);
                    } else if self.consume(TokenKind::Not) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        self.expect(TokenKind::NullP)?;
                        cmd.subtype = AlterTableType::SetNotNull;
                    } else if self.consume(TokenKind::Expression) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        self.expect(TokenKind::As)?;
                        self.expect(TokenKind::Char('('))?;
                        cmd.subtype = AlterTableType::SetExpression;
                        cmd.def = Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
                        self.expect(TokenKind::Char(')'))?;
                    } else if self.consume(TokenKind::Statistics) {
                        cmd.subtype = AlterTableType::SetStatistics;
                        if !self.consume(TokenKind::Default) {
                            cmd.def = Some(Box::new(self.parse_numeric_only()?));
                        }
                    } else if self.at(TokenKind::Char('(')) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        cmd.subtype = AlterTableType::SetOptions;
                        cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: self.parse_parenthesized_reloptions()?,
                            ..AArrayExpr::default()
                        })));
                    } else if self.consume(TokenKind::Storage) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        cmd.subtype = AlterTableType::SetStorage;
                        let storage = if self.consume(TokenKind::Default) {
                            "default".to_owned()
                        } else {
                            self.consume_col_id()
                                .ok_or_else(|| self.error_here("SET STORAGE requires a mode"))?
                        };
                        cmd.def = Some(Box::new(make_string_node(storage)));
                    } else if self.consume(TokenKind::Compression) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        cmd.subtype = AlterTableType::SetCompression;
                        let compression = if self.consume(TokenKind::Default) {
                            "default".to_owned()
                        } else {
                            self.consume_col_id().ok_or_else(|| {
                                self.error_here("SET COMPRESSION requires a method")
                            })?
                        };
                        cmd.def = Some(Box::new(make_string_node(compression)));
                    } else if self.consume(TokenKind::Generated) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        let generated_when = if self.consume(TokenKind::Always) {
                            b'a'
                        } else if self.consume(TokenKind::By) {
                            self.expect(TokenKind::Default)?;
                            b'd'
                        } else {
                            return Err(
                                self.error_here("SET GENERATED requires ALWAYS or BY DEFAULT")
                            );
                        };
                        cmd.subtype = AlterTableType::SetIdentity;
                        cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: vec![make_def_elem(
                                "generated",
                                Some(Node::Integer(Integer::new(i32::from(generated_when)))),
                                self.previous_location(),
                            )],
                            ..AArrayExpr::default()
                        })));
                    } else if matches!(
                        self.peek_kind(),
                        TokenKind::Cache
                            | TokenKind::Cycle
                            | TokenKind::Increment
                            | TokenKind::Logged
                            | TokenKind::Maxvalue
                            | TokenKind::Minvalue
                            | TokenKind::No
                            | TokenKind::Start
                            | TokenKind::Unlogged
                    ) {
                        if cmd.num != 0 {
                            return Err(self
                                .error_here("column numbers are only valid with SET STATISTICS"));
                        }
                        cmd.subtype = AlterTableType::SetIdentity;
                        cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: self.parse_sequence_options()?,
                            ..AArrayExpr::default()
                        })));
                    } else {
                        return Err(self.error_here("unsupported ALTER COLUMN SET action"));
                    }
                } else if self.consume(TokenKind::AddP) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    self.expect(TokenKind::Generated)?;
                    let generated_location = self.previous_location();
                    let generated_when = if self.consume(TokenKind::Always) {
                        b'a'
                    } else if self.consume(TokenKind::By) {
                        self.expect(TokenKind::Default)?;
                        b'd'
                    } else {
                        return Err(self.error_here("ADD GENERATED requires ALWAYS or BY DEFAULT"));
                    };
                    self.expect(TokenKind::As)?;
                    self.expect(TokenKind::IdentityP)?;
                    let options = if self.consume(TokenKind::Char('(')) {
                        self.parse_parenthesized_sequence_options_body()?
                    } else {
                        Vec::new()
                    };
                    cmd.subtype = AlterTableType::AddIdentity;
                    cmd.def = Some(Box::new(Node::Constraint(Constraint {
                        node_tag: NodeTag::Constraint,
                        contype: ConstrType::Identity,
                        generated_when,
                        options,
                        location: generated_location as ParseLoc,
                        ..Constraint::default()
                    })));
                } else if self.at(TokenKind::Restart) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    cmd.subtype = AlterTableType::SetIdentity;
                    cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements: self.parse_sequence_options()?,
                        ..AArrayExpr::default()
                    })));
                } else if self.consume(TokenKind::Reset) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    cmd.subtype = AlterTableType::ResetOptions;
                    cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements: self.parse_parenthesized_reloptions()?,
                        ..AArrayExpr::default()
                    })));
                } else if self.consume(TokenKind::Drop) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    if self.consume(TokenKind::Default) {
                        cmd.subtype = AlterTableType::ColumnDefault;
                    } else if self.consume(TokenKind::Not) {
                        self.expect(TokenKind::NullP)?;
                        cmd.subtype = AlterTableType::DropNotNull;
                    } else if self.consume(TokenKind::Expression) {
                        cmd.subtype = AlterTableType::DropExpression;
                        cmd.missing_ok = self.consume_if_exists()?;
                    } else if self.consume(TokenKind::IdentityP) {
                        cmd.subtype = AlterTableType::DropIdentity;
                        cmd.missing_ok = self.consume_if_exists()?;
                    } else {
                        return Err(self.error_here("unsupported ALTER COLUMN DROP action"));
                    }
                } else if self.at(TokenKind::Options) {
                    if cmd.num != 0 {
                        return Err(
                            self.error_here("column numbers are only valid with SET STATISTICS")
                        );
                    }
                    cmd.subtype = AlterTableType::AlterColumnGenericOptions;
                    cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements: self.parse_alter_generic_options()?,
                        ..AArrayExpr::default()
                    })));
                } else {
                    return Err(self.error_here("unsupported ALTER COLUMN action"));
                }
            }
            TokenKind::Set => cmd = self.parse_alter_table_set_cmd()?,
            TokenKind::Reset => cmd = self.parse_alter_table_reset_cmd()?,
            TokenKind::Validate => {
                self.advance();
                self.expect(TokenKind::Constraint)?;
                cmd.subtype = AlterTableType::ValidateConstraint;
                cmd.name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("VALIDATE CONSTRAINT requires a name"))?,
                );
            }
            TokenKind::EnableP => cmd = self.parse_alter_table_enable_cmd()?,
            TokenKind::DisableP => cmd = self.parse_alter_table_disable_cmd()?,
            TokenKind::Cluster => {
                self.advance();
                self.expect(TokenKind::On)?;
                cmd.subtype = AlterTableType::ClusterOn;
                cmd.name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("CLUSTER ON requires an index name"))?,
                );
            }
            TokenKind::Replica => {
                self.advance();
                self.expect(TokenKind::IdentityP)?;
                let (identity_type, name) = if self.consume(TokenKind::Nothing) {
                    (b'n', None)
                } else if self.consume(TokenKind::Full) {
                    (b'f', None)
                } else if self.consume(TokenKind::Default) {
                    (b'd', None)
                } else if self.consume(TokenKind::Using) {
                    self.expect(TokenKind::Index)?;
                    (
                        b'i',
                        Some(self.consume_col_id().ok_or_else(|| {
                            self.error_here("REPLICA IDENTITY USING INDEX requires an index name")
                        })?),
                    )
                } else {
                    return Err(self.error_here(
                        "REPLICA IDENTITY requires NOTHING, FULL, DEFAULT, or USING INDEX",
                    ));
                };
                cmd.subtype = AlterTableType::ReplicaIdentity;
                cmd.def = Some(Box::new(Node::ReplicaIdentityStmt(ReplicaIdentityStmt {
                    node_tag: NodeTag::ReplicaIdentityStmt,
                    identity_type,
                    name,
                })));
            }
            TokenKind::Owner => {
                self.advance();
                self.expect(TokenKind::To)?;
                cmd.subtype = AlterTableType::ChangeOwner;
                cmd.newowner =
                    Some(Box::new(self.consume_role_spec().ok_or_else(|| {
                        self.error_here("OWNER TO requires a role")
                    })?));
            }
            TokenKind::Attach | TokenKind::Detach | TokenKind::Split | TokenKind::Merge => {
                cmd = self.parse_alter_table_partition_cmd(objtype)?;
            }
            TokenKind::Inherit => {
                self.advance();
                cmd.subtype = AlterTableType::AddInherit;
                cmd.def = Some(Box::new(Node::RangeVar(
                    self.try_parse_qualified_range_var()
                        .ok_or_else(|| self.error_here("INHERIT requires a parent table"))?,
                )));
            }
            TokenKind::No => {
                self.advance();
                if self.consume(TokenKind::Inherit) {
                    cmd.subtype = AlterTableType::DropInherit;
                    cmd.def = Some(Box::new(Node::RangeVar(
                        self.try_parse_qualified_range_var()
                            .ok_or_else(|| self.error_here("NO INHERIT requires a parent table"))?,
                    )));
                } else if self.consume(TokenKind::Force) {
                    self.expect(TokenKind::Row)?;
                    self.expect(TokenKind::Level)?;
                    self.expect(TokenKind::Security)?;
                    cmd.subtype = AlterTableType::NoForceRowSecurity;
                } else {
                    return Err(self.error_here("NO requires INHERIT or FORCE ROW LEVEL SECURITY"));
                }
            }
            TokenKind::Of => {
                let location = self.advance().location();
                let names = self.parse_name_list();
                if names.is_empty() {
                    return Err(self.error_here("OF requires a type name"));
                }
                cmd.subtype = AlterTableType::AddOf;
                cmd.def = Some(Box::new(Node::TypeName(TypeName {
                    node_tag: NodeTag::TypeName,
                    names,
                    location: location as ParseLoc,
                    ..TypeName::default()
                })));
            }
            TokenKind::Not => {
                self.advance();
                self.expect(TokenKind::Of)?;
                cmd.subtype = AlterTableType::DropOf;
            }
            TokenKind::Force => {
                self.advance();
                self.expect(TokenKind::Row)?;
                self.expect(TokenKind::Level)?;
                self.expect(TokenKind::Security)?;
                cmd.subtype = AlterTableType::ForceRowSecurity;
            }
            TokenKind::Options => {
                cmd.subtype = AlterTableType::GenericOptions;
                cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                    node_tag: NodeTag::AArrayExpr,
                    elements: self.parse_alter_generic_options()?,
                    ..AArrayExpr::default()
                })));
            }
            other => {
                return Err(self.error_here(format!(
                    "unsupported ALTER TABLE command starting with {:?}",
                    other
                )));
            }
        }
        if !self.at_any(&[TokenKind::Char(','), TokenKind::Char(';'), TokenKind::Eof]) {
            return Err(self.error_here("unexpected token after ALTER TABLE command"));
        }
        Ok(cmd)
    }
}
