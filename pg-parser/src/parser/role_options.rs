use super::*;

impl Parser {
    pub(super) fn parse_create_role(&mut self) -> PResult<Node> {
        let stmt_type = match self.advance().kind {
            TokenKind::User => RoleStmtType::User,
            TokenKind::GroupP => RoleStmtType::Group,
            _ => RoleStmtType::Role,
        };
        let role = Some(
            self.consume_role_id()?
                .ok_or_else(|| self.error_here("CREATE ROLE requires a role name"))?,
        );
        self.consume(TokenKind::With);
        let options = self.parse_create_role_options()?;
        Ok(Node::CreateRoleStmt(CreateRoleStmt {
            node_tag: NodeTag::CreateRoleStmt,
            stmt_type,
            role,
            options,
        }))
    }

    pub(super) fn parse_alter_role(&mut self) -> PResult<Node> {
        let role_kind = self.advance().kind;
        let role = if self.at(TokenKind::All) {
            self.advance();
            None
        } else {
            self.consume_role_spec().map(Box::new)
        };
        if role_kind == TokenKind::GroupP {
            let role = role.ok_or_else(|| self.error_here("ALTER GROUP requires a group name"))?;
            let action = if self.consume(TokenKind::AddP) {
                1
            } else if self.consume(TokenKind::Drop) {
                -1
            } else {
                return Err(self.error_here("ALTER GROUP requires ADD or DROP USER"));
            };
            self.expect(TokenKind::User)?;
            let members_location = self.location();
            let members =
                self.parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
            if members.is_empty() {
                return Err(self.error_here("ALTER GROUP requires at least one member"));
            }
            return Ok(Node::AlterRoleStmt(AlterRoleStmt {
                node_tag: NodeTag::AlterRoleStmt,
                role: Some(role),
                options: vec![make_def_elem(
                    "rolemembers",
                    Some(name_list_node(members)),
                    members_location,
                )],
                action,
            }));
        }
        if role.is_none()
            && !matches!(
                self.peek_kind(),
                TokenKind::InP | TokenKind::Set | TokenKind::Reset
            )
        {
            return Err(self.error_here("ALTER ROLE requires a role name"));
        }
        let database = if self.consume(TokenKind::InP) {
            self.expect(TokenKind::Database)?;
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("IN DATABASE requires a database name"))?,
            )
        } else {
            None
        };
        if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            let setstmt = Some(Box::new(self.parse_variable_set_like(false)?));
            return Ok(Node::AlterRoleSetStmt(AlterRoleSetStmt {
                node_tag: NodeTag::AlterRoleSetStmt,
                role,
                database,
                setstmt,
            }));
        }
        if database.is_some() || role.is_none() {
            return Err(self.error_here("ALTER ROLE ALL and IN DATABASE require SET or RESET"));
        }
        self.consume(TokenKind::With);
        let options = self.parse_alter_role_options()?;
        Ok(Node::AlterRoleStmt(AlterRoleStmt {
            node_tag: NodeTag::AlterRoleStmt,
            role,
            options,
            action: 1,
        }))
    }

    pub(super) fn parse_drop_role(&mut self) -> PResult<Node> {
        self.advance();
        let missing_ok = self.consume_if_exists()?;
        let roles = self.parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
        Ok(Node::DropRoleStmt(DropRoleStmt {
            node_tag: NodeTag::DropRoleStmt,
            roles,
            missing_ok,
        }))
    }

    pub(super) fn parse_drop_owned(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Owned)?;
        self.expect(TokenKind::By)?;
        let roles = self.parse_role_specs_until(
            &[
                TokenKind::Cascade,
                TokenKind::Restrict,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
            false,
        )?;
        let behavior = self.parse_drop_behavior();
        Ok(Node::DropOwnedStmt(DropOwnedStmt {
            node_tag: NodeTag::DropOwnedStmt,
            roles,
            behavior,
        }))
    }

    pub(super) fn parse_reassign_owned(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Reassign)?;
        self.expect(TokenKind::Owned)?;
        self.expect(TokenKind::By)?;
        let roles = self.parse_role_specs_until(
            &[TokenKind::To, TokenKind::Char(';'), TokenKind::Eof],
            false,
        )?;
        if roles.is_empty() {
            return Err(self.error_here("REASSIGN OWNED requires at least one source role"));
        }
        self.expect(TokenKind::To)?;
        let newrole = Some(Box::new(self.consume_role_spec().ok_or_else(|| {
            self.error_here("REASSIGN OWNED requires a destination role")
        })?));
        Ok(Node::ReassignOwnedStmt(ReassignOwnedStmt {
            node_tag: NodeTag::ReassignOwnedStmt,
            roles,
            newrole,
        }))
    }

    pub(super) fn parse_create_role_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            if self.consume(TokenKind::Password) {
                let arg = if self.consume(TokenKind::NullP) {
                    None
                } else {
                    if !self.at(TokenKind::SConst) {
                        return Err(self.error_here("PASSWORD requires a string or NULL"));
                    }
                    self.consume_string_like().map(make_string_node)
                };
                options.push(make_def_elem("password", arg, location));
            } else if self.consume(TokenKind::Encrypted) {
                self.expect(TokenKind::Password)?;
                if !self.at(TokenKind::SConst) {
                    return Err(self.error_here("ENCRYPTED PASSWORD requires a string"));
                }
                let password = self.consume_string_like().unwrap_or_default();
                options.push(make_def_elem(
                    "password",
                    Some(make_string_node(password)),
                    location,
                ));
            } else if self.consume(TokenKind::Inherit) {
                options.push(make_def_elem(
                    "inherit",
                    Some(Node::Boolean(Boolean::new(true))),
                    location,
                ));
            } else if self.consume(TokenKind::Connection) {
                self.expect(TokenKind::Limit)?;
                let value = self.parse_signed_integer()?;
                options.push(make_def_elem("connectionlimit", Some(value), location));
            } else if self.consume(TokenKind::Valid) {
                self.expect(TokenKind::Until)?;
                if !self.at(TokenKind::SConst) {
                    return Err(self.error_here("VALID UNTIL requires a string"));
                }
                let value = self.consume_string_like().unwrap_or_default();
                options.push(make_def_elem(
                    "validUntil",
                    Some(make_string_node(value)),
                    location,
                ));
            } else if self.consume(TokenKind::Sysid) {
                let token = self.expect(TokenKind::IConst)?;
                let Some(TokenValue::Integer(value)) = token.value else {
                    return Err(ParseError::new(token.location, "SYSID requires an integer"));
                };
                options.push(make_def_elem(
                    "sysid",
                    Some(Node::Integer(Integer::new(value))),
                    location,
                ));
            } else if matches!(
                self.peek_kind(),
                TokenKind::InP | TokenKind::Role | TokenKind::Admin | TokenKind::User
            ) {
                let defname = if self.consume(TokenKind::InP) {
                    if !self.consume(TokenKind::Role) {
                        self.expect(TokenKind::GroupP)?;
                    }
                    "addroleto"
                } else if self.consume(TokenKind::Admin) {
                    "adminmembers"
                } else {
                    self.advance();
                    "rolemembers"
                };
                let roles =
                    self.parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
                options.push(make_def_elem(
                    defname,
                    Some(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements: roles,
                        ..AArrayExpr::default()
                    })),
                    location,
                ));
            } else {
                let name = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("invalid CREATE ROLE option"))?;
                let (defname, value) = match name.as_str() {
                    "superuser" => ("superuser", true),
                    "nosuperuser" => ("superuser", false),
                    "createrole" => ("createrole", true),
                    "nocreaterole" => ("createrole", false),
                    "createdb" => ("createdb", true),
                    "nocreatedb" => ("createdb", false),
                    "login" => ("canlogin", true),
                    "nologin" => ("canlogin", false),
                    "replication" => ("isreplication", true),
                    "noreplication" => ("isreplication", false),
                    "bypassrls" => ("bypassrls", true),
                    "nobypassrls" => ("bypassrls", false),
                    "noinherit" => ("inherit", false),
                    _ => return Err(ParseError::new(location, "invalid CREATE ROLE option")),
                };
                options.push(make_def_elem(
                    defname,
                    Some(Node::Boolean(Boolean::new(value))),
                    location,
                ));
            }
        }
        Ok(options)
    }

    pub(super) fn parse_alter_role_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            if self.consume(TokenKind::Password) {
                let arg = if self.consume(TokenKind::NullP) {
                    None
                } else {
                    Some(make_string_node(self.consume_required_string(
                        "PASSWORD requires a string or NULL",
                    )?))
                };
                options.push(make_def_elem("password", arg, location));
            } else if self.consume(TokenKind::Encrypted) {
                self.expect(TokenKind::Password)?;
                let password =
                    self.consume_required_string("ENCRYPTED PASSWORD requires a string")?;
                options.push(make_def_elem(
                    "password",
                    Some(make_string_node(password)),
                    location,
                ));
            } else if self.consume(TokenKind::Inherit) {
                options.push(make_def_elem(
                    "inherit",
                    Some(Node::Boolean(Boolean::new(true))),
                    location,
                ));
            } else if self.consume(TokenKind::Connection) {
                self.expect(TokenKind::Limit)?;
                let value = self.parse_signed_integer()?;
                options.push(make_def_elem("connectionlimit", Some(value), location));
            } else if self.consume(TokenKind::Valid) {
                self.expect(TokenKind::Until)?;
                let value = self.consume_required_string("VALID UNTIL requires a string")?;
                options.push(make_def_elem(
                    "validUntil",
                    Some(make_string_node(value)),
                    location,
                ));
            } else if self.consume(TokenKind::User) {
                let members =
                    self.parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
                if members.is_empty() {
                    return Err(self.error_here("USER requires at least one role"));
                }
                options.push(make_def_elem(
                    "rolemembers",
                    Some(name_list_node(members)),
                    location,
                ));
            } else {
                let option = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("invalid ALTER ROLE option"))?;
                let (name, value) = match option.as_str() {
                    "superuser" => ("superuser", true),
                    "nosuperuser" => ("superuser", false),
                    "createrole" => ("createrole", true),
                    "nocreaterole" => ("createrole", false),
                    "createdb" => ("createdb", true),
                    "nocreatedb" => ("createdb", false),
                    "login" => ("canlogin", true),
                    "nologin" => ("canlogin", false),
                    "replication" => ("isreplication", true),
                    "noreplication" => ("isreplication", false),
                    "bypassrls" => ("bypassrls", true),
                    "nobypassrls" => ("bypassrls", false),
                    "noinherit" => ("inherit", false),
                    "unencrypted" => {
                        return Err(ParseError::new(
                            location,
                            "UNENCRYPTED PASSWORD is not supported",
                        ));
                    }
                    _ => return Err(ParseError::new(location, "invalid ALTER ROLE option")),
                };
                options.push(make_def_elem(
                    name,
                    Some(Node::Boolean(Boolean::new(value))),
                    location,
                ));
            }
        }
        Ok(options)
    }

    pub(super) fn parse_generic_set_reset_clause(&mut self) -> PResult<VariableSetStmt> {
        if self.consume(TokenKind::Reset) {
            let (kind, name) = if self.consume(TokenKind::All) {
                (VariableSetKind::ResetAll, None)
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
        let name = Some(
            self.consume_setting_name()
                .ok_or_else(|| self.error_here("SET requires a parameter name"))?,
        );
        if !self.consume(TokenKind::To) && !self.consume(TokenKind::Char('=')) {
            return Err(self.error_here("SET parameter requires TO or '='"));
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
            let args = self.parse_setting_value_list()?;
            if args.is_empty() {
                return Err(self.error_here("SET parameter requires a value"));
            }
            (VariableSetKind::SetValue, args, value_location)
        };
        self.expect_statement_end()?;
        Ok(VariableSetStmt {
            node_tag: NodeTag::VariableSetStmt,
            kind,
            name,
            args,
            location,
            ..VariableSetStmt::default()
        })
    }
}
