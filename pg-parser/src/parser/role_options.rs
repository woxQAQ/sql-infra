//! Role lifecycle, ownership, and role-option grammar.
//!
//! Create/alter/drop roles, owned-object statements, password and capability
//! options, and role-scoped setting clauses share this module.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis subset — CREATE ROLE aliases
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-createrole.html
    // - https://www.postgresql.org/docs/18/sql-createuser.html
    // - https://www.postgresql.org/docs/18/sql-creategroup.html
    //
    // CREATE { ROLE | USER | GROUP } name [ [ WITH ] option [ ... ] ]
    //
    // where option can be:
    //     SUPERUSER | NOSUPERUSER | CREATEDB | NOCREATEDB
    //     CREATEROLE | NOCREATEROLE | INHERIT | NOINHERIT
    //     LOGIN | NOLOGIN | REPLICATION | NOREPLICATION
    //     BYPASSRLS | NOBYPASSRLS | CONNECTION LIMIT connlimit
    //     [ ENCRYPTED ] PASSWORD 'password' | PASSWORD NULL
    //     VALID UNTIL 'timestamp'
    //     IN { ROLE | GROUP } role_name [, ...]
    //     { ROLE | USER | ADMIN } role_name [, ...] | SYSID uid
    pub(super) fn parse_create_role(&mut self) -> PResult<Node> {
        let stmt_type = match self.advance().kind {
            TokenKind::User => RoleStmtType::User,
            TokenKind::GroupP => RoleStmtType::Group,
            _ => RoleStmtType::Role,
        };
        if stmt_type == RoleStmtType::User {
            self.record_completion_tokens(&[TokenKind::Mapping]);
        }
        let role = Some(
            self.consume_role_id()?
                .ok_or_else(|| self.error_here("CREATE ROLE requires a role name"))?,
        );
        self.consume(TokenKind::With);
        let options = self.parse_create_role_options()?;
        Ok(Node::CreateRoleStmt(CreateRoleStmt {
            stmt_type,
            role,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis subset — ALTER ROLE aliases
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-alterrole.html
    // - https://www.postgresql.org/docs/18/sql-alteruser.html
    // - https://www.postgresql.org/docs/18/sql-altergroup.html
    //
    // ALTER { ROLE | USER } role_specification [ WITH ] option [ ... ]
    // ALTER { ROLE | USER } { role_specification | ALL }
    //     [ IN DATABASE database_name ] { SET | RESET } configuration_parameter ...
    // ALTER GROUP role_specification { ADD | DROP } USER user_name [, ...]
    //
    // RENAME forms are handled by alter_identity.
    pub(super) fn parse_alter_role(&mut self) -> PResult<Node> {
        let role_kind = self.advance().kind;
        if role_kind == TokenKind::User {
            self.record_completion_tokens(&[TokenKind::Mapping]);
        }
        let all_roles = role_kind != TokenKind::GroupP && self.at(TokenKind::All);
        let role = if all_roles {
            self.advance();
            None
        } else {
            self.consume_role_spec().map(Box::new)
        };
        if role.is_some() {
            self.record_completion_tokens(&[TokenKind::Rename]);
        }
        if self.at_completion() && role.is_none() && !all_roles {
            return Err(self.error_here("expected an ALTER ROLE target"));
        }
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
                role: Some(role),
                options: vec![make_def_elem(
                    "rolemembers",
                    Some(name_list_node(members)),
                    members_location,
                )],
                action,
            }));
        }
        if role.is_none() {
            self.record_completion_tokens(&[TokenKind::InP, TokenKind::Set, TokenKind::Reset]);
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
        self.record_completion_tokens(&[TokenKind::Set, TokenKind::Reset]);
        if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            let setstmt = Some(Box::new(self.parse_variable_set_like(false)?));
            return Ok(Node::AlterRoleSetStmt(AlterRoleSetStmt {
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
            role,
            options,
            action: 1,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-droprole.html
    // DROP ROLE [ IF EXISTS ] name [, ...]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropuser.html
    // DROP USER [ IF EXISTS ] name [, ...]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropgroup.html
    // DROP GROUP [ IF EXISTS ] name [, ...]
    pub(super) fn parse_drop_role(&mut self) -> PResult<Node> {
        self.advance();
        let missing_ok = self.consume_if_exists()?;
        let mut roles = Vec::new();
        loop {
            let role = self
                .consume_role_spec_without_special_suggestions()
                .ok_or_else(|| self.error_here("DROP ROLE requires a role name"))?;
            roles.push(Node::RoleSpec(role));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(Node::DropRoleStmt(DropRoleStmt { roles, missing_ok }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-drop-owned.html
    // DROP OWNED BY { name | CURRENT_ROLE | CURRENT_USER | SESSION_USER } [, ...] [ CASCADE |
    // RESTRICT ]
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
        Ok(Node::DropOwnedStmt(DropOwnedStmt { roles, behavior }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-reassign-owned.html
    // REASSIGN OWNED BY { old_role | CURRENT_ROLE | CURRENT_USER | SESSION_USER } [, ...]
    //                TO { new_role | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
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
            roles,
            newrole,
        }))
    }

    pub(super) fn parse_create_role_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Password => {
                    self.advance();
                    let arg = if self.consume(TokenKind::NullP) {
                        None
                    } else {
                        Some(make_string_node(self.consume_required_string(
                            "PASSWORD requires a string or NULL",
                        )?))
                    };
                    options.push(make_def_elem("password", arg, location));
                }
                TokenKind::Encrypted => {
                    self.advance();
                    self.expect(TokenKind::Password)?;
                    let password =
                        self.consume_required_string("ENCRYPTED PASSWORD requires a string")?;
                    options.push(make_def_elem(
                        "password",
                        Some(make_string_node(password)),
                        location,
                    ));
                }
                TokenKind::Inherit => {
                    self.advance();
                    options.push(make_def_elem(
                        "inherit",
                        Some(Node::Boolean(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Connection => {
                    self.advance();
                    self.expect(TokenKind::Limit)?;
                    let value = self.parse_signed_integer()?;
                    options.push(make_def_elem("connectionlimit", Some(value), location));
                }
                TokenKind::Valid => {
                    self.advance();
                    self.expect(TokenKind::Until)?;
                    let value = self.consume_required_string("VALID UNTIL requires a string")?;
                    options.push(make_def_elem(
                        "validUntil",
                        Some(make_string_node(value)),
                        location,
                    ));
                }
                TokenKind::Sysid => {
                    self.advance();
                    let token = self.expect(TokenKind::IConst)?;
                    let Some(TokenValue::Integer(value)) = token.value else {
                        return Err(ParseError::ranged(token.range, "SYSID requires an integer"));
                    };
                    options.push(make_def_elem(
                        "sysid",
                        Some(Node::Integer(Integer::new(value))),
                        location,
                    ));
                }
                TokenKind::InP | TokenKind::Role | TokenKind::Admin | TokenKind::User => {
                    let defname = match self.peek_kind() {
                        TokenKind::InP => {
                            self.advance();
                            if !self.consume(TokenKind::Role) {
                                self.expect(TokenKind::GroupP)?;
                            }
                            "addroleto"
                        }
                        TokenKind::Admin => {
                            self.advance();
                            "adminmembers"
                        }
                        TokenKind::Role | TokenKind::User => {
                            self.advance();
                            "rolemembers"
                        }
                        _ => unreachable!(),
                    };
                    let roles = self
                        .parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
                    options.push(make_def_elem(
                        defname,
                        Some(Node::AArrayExpr(AArrayExpr {
                            elements: roles,
                            ..AArrayExpr::default()
                        })),
                        location,
                    ));
                }
                _ => {
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
                        _ => {
                            return Err(ParseError::syntax_exit(
                                location,
                                "invalid CREATE ROLE option",
                            ));
                        }
                    };
                    options.push(make_def_elem(
                        defname,
                        Some(Node::Boolean(Boolean::new(value))),
                        location,
                    ));
                }
            }
        }
        Ok(options)
    }

    pub(super) fn parse_alter_role_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Password => {
                    self.advance();
                    let arg = if self.consume(TokenKind::NullP) {
                        None
                    } else {
                        Some(make_string_node(self.consume_required_string(
                            "PASSWORD requires a string or NULL",
                        )?))
                    };
                    options.push(make_def_elem("password", arg, location));
                }
                TokenKind::Encrypted => {
                    self.advance();
                    self.expect(TokenKind::Password)?;
                    let password =
                        self.consume_required_string("ENCRYPTED PASSWORD requires a string")?;
                    options.push(make_def_elem(
                        "password",
                        Some(make_string_node(password)),
                        location,
                    ));
                }
                TokenKind::Inherit => {
                    self.advance();
                    options.push(make_def_elem(
                        "inherit",
                        Some(Node::Boolean(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Connection => {
                    self.advance();
                    self.expect(TokenKind::Limit)?;
                    let value = self.parse_signed_integer()?;
                    options.push(make_def_elem("connectionlimit", Some(value), location));
                }
                TokenKind::Valid => {
                    self.advance();
                    self.expect(TokenKind::Until)?;
                    let value = self.consume_required_string("VALID UNTIL requires a string")?;
                    options.push(make_def_elem(
                        "validUntil",
                        Some(make_string_node(value)),
                        location,
                    ));
                }
                TokenKind::User => {
                    self.advance();
                    let members = self
                        .parse_role_specs_until(&[TokenKind::Char(';'), TokenKind::Eof], false)?;
                    if members.is_empty() {
                        return Err(self.error_here("USER requires at least one role"));
                    }
                    options.push(make_def_elem(
                        "rolemembers",
                        Some(name_list_node(members)),
                        location,
                    ));
                }
                _ => {
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
                            return Err(ParseError::syntax_exit(
                                location,
                                "UNENCRYPTED PASSWORD is not supported",
                            ));
                        }
                        _ => {
                            return Err(ParseError::syntax_exit(
                                location,
                                "invalid ALTER ROLE option",
                            ));
                        }
                    };
                    options.push(make_def_elem(
                        name,
                        Some(Node::Boolean(Boolean::new(value))),
                        location,
                    ));
                }
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
            kind,
            name,
            args,
            location,
            ..VariableSetStmt::default()
        })
    }
}
