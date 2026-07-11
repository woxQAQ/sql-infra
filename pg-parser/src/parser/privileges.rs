use super::*;

impl Parser {
    pub(super) fn parse_alter_default_privileges(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Default)?;
        self.expect(TokenKind::Privileges)?;
        let mut options = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Grant | TokenKind::Revoke | TokenKind::Eof
        ) {
            let location = self.location();
            match self.peek_kind() {
                TokenKind::For => {
                    self.advance();
                    if !self.consume(TokenKind::Role) {
                        self.expect(TokenKind::User)?;
                    }
                    let roles = self.parse_role_specs_until(
                        &[
                            TokenKind::InP,
                            TokenKind::Grant,
                            TokenKind::Revoke,
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ],
                        false,
                    )?;
                    if roles.is_empty() {
                        return Err(self.error_here("FOR ROLE/USER requires at least one role"));
                    }
                    options.push(Node::DefElem(DefElem {
                        node_tag: NodeTag::DefElem,
                        defname: Some("roles".to_owned()),
                        arg: Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: roles,
                            ..AArrayExpr::default()
                        }))),
                        location: location as ParseLoc,
                        ..DefElem::default()
                    }));
                }
                TokenKind::InP => {
                    self.advance();
                    self.expect(TokenKind::Schema)?;
                    let schemas = self.parse_simple_name_list_until(&[
                        TokenKind::Grant,
                        TokenKind::Revoke,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])?;
                    if schemas.is_empty() {
                        return Err(self.error_here("IN SCHEMA requires at least one schema"));
                    }
                    options.push(Node::DefElem(DefElem {
                        node_tag: NodeTag::DefElem,
                        defname: Some("schemas".to_owned()),
                        arg: Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: schemas,
                            ..AArrayExpr::default()
                        }))),
                        location: location as ParseLoc,
                        ..DefElem::default()
                    }));
                }
                _ => {
                    return Err(self.error_here("invalid ALTER DEFAULT PRIVILEGES option"));
                }
            }
        }
        let action = if matches!(self.peek_kind(), TokenKind::Grant | TokenKind::Revoke) {
            Some(Box::new(self.parse_default_privileges_action()?))
        } else {
            return Err(self.error_here("ALTER DEFAULT PRIVILEGES requires GRANT or REVOKE"));
        };
        Ok(Node::AlterDefaultPrivilegesStmt(
            AlterDefaultPrivilegesStmt {
                node_tag: NodeTag::AlterDefaultPrivilegesStmt,
                options,
                action,
            },
        ))
    }

    fn parse_default_privileges_action(&mut self) -> PResult<GrantStmt> {
        let is_grant = self.consume(TokenKind::Grant);
        if !is_grant {
            self.expect(TokenKind::Revoke)?;
        }
        let grant_option = if !is_grant && self.consume(TokenKind::Grant) {
            self.expect(TokenKind::Option)?;
            self.expect(TokenKind::For)?;
            true
        } else {
            false
        };
        let privileges = self.parse_access_privileges()?;
        self.expect(TokenKind::On)?;
        let objtype = if self.consume(TokenKind::Tables) {
            ObjectType::Table
        } else if self.consume(TokenKind::Functions) || self.consume(TokenKind::Routines) {
            ObjectType::Function
        } else if self.consume(TokenKind::Sequences) {
            ObjectType::Sequence
        } else if self.consume(TokenKind::TypesP) {
            ObjectType::Type
        } else if self.consume(TokenKind::Schemas) {
            ObjectType::Schema
        } else if self.consume(TokenKind::LargeP) {
            self.expect(TokenKind::ObjectsP)?;
            ObjectType::Largeobject
        } else {
            return Err(self.error_here("invalid default privilege target"));
        };
        self.expect(if is_grant {
            TokenKind::To
        } else {
            TokenKind::From
        })?;
        let grantees = self.parse_role_specs_until(
            &[
                TokenKind::With,
                TokenKind::Cascade,
                TokenKind::Restrict,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
            true,
        )?;
        let (grant_option, behavior) = if is_grant {
            let with_grant_option = if self.consume(TokenKind::With) {
                self.expect(TokenKind::Grant)?;
                self.expect(TokenKind::Option)?;
                true
            } else {
                false
            };
            (with_grant_option, DropBehavior::Restrict)
        } else {
            (grant_option, self.parse_drop_behavior())
        };
        self.expect_statement_end()?;
        Ok(GrantStmt {
            node_tag: NodeTag::GrantStmt,
            is_grant,
            targtype: GrantTargetType::Defaults,
            objtype,
            objects: Vec::new(),
            privileges,
            grantees,
            grant_option,
            behavior,
            ..GrantStmt::default()
        })
    }

    pub(super) fn parse_grant(&mut self, is_grant: bool) -> PResult<Node> {
        self.advance();
        let mut revoke_grant_option = false;
        if !is_grant && self.consume(TokenKind::Grant) {
            self.expect(TokenKind::Option)?;
            self.expect(TokenKind::For)?;
            revoke_grant_option = true;
        }

        if self.has_top_level_token_before(
            TokenKind::On,
            &[TokenKind::To, TokenKind::From, TokenKind::Eof],
        ) {
            return self.parse_object_grant(is_grant, revoke_grant_option);
        }
        self.parse_role_grant(is_grant)
    }

    fn parse_object_grant(&mut self, is_grant: bool, revoke_grant_option: bool) -> PResult<Node> {
        let privileges = self.parse_access_privileges()?;
        self.expect(TokenKind::On)?;
        let (targtype, objtype, objects) = self.parse_privilege_target()?;
        self.expect(if is_grant {
            TokenKind::To
        } else {
            TokenKind::From
        })?;
        let grantees = self.parse_role_specs_until(
            &[
                TokenKind::With,
                TokenKind::Granted,
                TokenKind::Cascade,
                TokenKind::Restrict,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
            true,
        )?;
        let grant_option = if is_grant && self.consume(TokenKind::With) {
            self.expect(TokenKind::Grant)?;
            self.expect(TokenKind::Option)?;
            true
        } else {
            revoke_grant_option
        };
        let grantor = if self.consume(TokenKind::Granted) {
            self.expect(TokenKind::By)?;
            Some(Box::new(
                self.consume_role_spec()
                    .ok_or_else(|| self.error_here("expected a grantor role"))?,
            ))
        } else {
            None
        };
        let behavior = if is_grant {
            DropBehavior::Restrict
        } else {
            self.parse_drop_behavior()
        };
        Ok(Node::GrantStmt(GrantStmt {
            node_tag: NodeTag::GrantStmt,
            is_grant,
            targtype,
            objtype,
            objects,
            privileges,
            grantees,
            grant_option,
            grantor,
            behavior,
        }))
    }

    fn parse_role_grant(&mut self, is_grant: bool) -> PResult<Node> {
        let mut opt = Vec::new();
        if !is_grant
            && self.peek_kind_n(1) == TokenKind::Option
            && self.peek_kind_n(2) == TokenKind::For
        {
            let location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a role option name"))?;
            self.expect(TokenKind::Option)?;
            self.expect(TokenKind::For)?;
            opt.push(make_def_elem(
                &name,
                Some(Node::Boolean(Boolean::new(false))),
                location,
            ));
        }
        let separator = if is_grant {
            TokenKind::To
        } else {
            TokenKind::From
        };
        if self.at(TokenKind::All) {
            return Err(self.error_here("GRANT/REVOKE ROLE requires an explicit role list"));
        }
        let granted_roles = self.parse_access_privileges_until(separator)?;
        self.expect(separator)?;
        let grantee_roles = self.parse_role_specs_until(
            &[
                TokenKind::With,
                TokenKind::Granted,
                TokenKind::Cascade,
                TokenKind::Restrict,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
            false,
        )?;
        if is_grant && self.consume(TokenKind::With) {
            loop {
                let location = self.location();
                let name = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("expected a role option name"))?;
                let value = if self.consume(TokenKind::Option) || self.consume(TokenKind::TrueP) {
                    true
                } else if self.consume(TokenKind::FalseP) {
                    false
                } else {
                    return Err(self.error_here("expected OPTION, TRUE, or FALSE"));
                };
                opt.push(make_def_elem(
                    &name,
                    Some(Node::Boolean(Boolean::new(value))),
                    location,
                ));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
        }
        let grantor = if self.consume(TokenKind::Granted) {
            self.expect(TokenKind::By)?;
            Some(Box::new(
                self.consume_role_spec()
                    .ok_or_else(|| self.error_here("expected a grantor role"))?,
            ))
        } else {
            None
        };
        let behavior = if is_grant {
            DropBehavior::Restrict
        } else {
            self.parse_drop_behavior()
        };
        Ok(Node::GrantRoleStmt(GrantRoleStmt {
            node_tag: NodeTag::GrantRoleStmt,
            granted_roles,
            grantee_roles,
            is_grant,
            opt,
            grantor,
            behavior,
        }))
    }

    pub(super) fn parse_access_privileges(&mut self) -> PResult<NodeList> {
        self.parse_access_privileges_until(TokenKind::On)
    }

    fn parse_access_privileges_until(&mut self, stop: TokenKind) -> PResult<NodeList> {
        if self.consume(TokenKind::All) {
            self.consume(TokenKind::Privileges);
            let cols = self.parse_optional_column_name_list()?;
            return if cols.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![Node::AccessPriv(AccessPriv {
                    node_tag: NodeTag::AccessPriv,
                    cols,
                    ..AccessPriv::default()
                })])
            };
        }
        let mut privileges = Vec::new();
        while !self.at(stop) {
            let (name, allow_columns) = if self.consume(TokenKind::Alter) {
                self.expect(TokenKind::SystemP)?;
                ("alter system".to_owned(), false)
            } else if matches!(
                self.peek_kind(),
                TokenKind::Select | TokenKind::References | TokenKind::Create
            ) {
                let token = self.advance().clone();
                (
                    token_name(&token).unwrap_or_else(|| token_text(&token)),
                    true,
                )
            } else {
                (
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected a privilege or role name"))?,
                    true,
                )
            };
            let cols = if allow_columns {
                self.parse_optional_column_name_list()?
            } else {
                Vec::new()
            };
            privileges.push(Node::AccessPriv(AccessPriv {
                node_tag: NodeTag::AccessPriv,
                priv_name: Some(name),
                cols,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(stop) {
                return Err(self.error_here("expected a privilege or role after ','"));
            }
        }
        if privileges.is_empty() {
            return Err(self.error_here("expected at least one privilege or role"));
        }
        Ok(privileges)
    }

    pub(super) fn parse_optional_column_name_list(&mut self) -> PResult<NodeList> {
        if !self.consume(TokenKind::Char('(')) {
            return Ok(Vec::new());
        }
        let mut columns = Vec::new();
        loop {
            let column = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a column name"))?;
            columns.push(make_string_node(column));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(columns)
    }

    fn parse_privilege_target(&mut self) -> PResult<(GrantTargetType, ObjectType, NodeList)> {
        if self.consume(TokenKind::All) {
            let objtype = if self.consume(TokenKind::Tables) {
                ObjectType::Table
            } else if self.consume(TokenKind::Sequences) {
                ObjectType::Sequence
            } else if self.consume(TokenKind::Functions) {
                ObjectType::Function
            } else if self.consume(TokenKind::Procedures) {
                ObjectType::Procedure
            } else if self.consume(TokenKind::Routines) {
                ObjectType::Routine
            } else {
                return Err(self.error_here("expected an object class after ON ALL"));
            };
            self.expect(TokenKind::InP)?;
            self.expect(TokenKind::Schema)?;
            let objects = self.parse_simple_name_list_until(&[
                TokenKind::To,
                TokenKind::From,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
            if objects.is_empty() {
                return Err(self.error_here("IN SCHEMA requires at least one schema"));
            }
            return Ok((GrantTargetType::AllInSchema, objtype, objects));
        }

        let objtype = match self.peek_kind() {
            TokenKind::Table => {
                self.advance();
                ObjectType::Table
            }
            TokenKind::Sequence => {
                self.advance();
                ObjectType::Sequence
            }
            TokenKind::Function => {
                self.advance();
                ObjectType::Function
            }
            TokenKind::Procedure => {
                self.advance();
                ObjectType::Procedure
            }
            TokenKind::Routine => {
                self.advance();
                ObjectType::Routine
            }
            TokenKind::Database => {
                self.advance();
                ObjectType::Database
            }
            TokenKind::DomainP => {
                self.advance();
                ObjectType::Domain
            }
            TokenKind::Language => {
                self.advance();
                ObjectType::Language
            }
            TokenKind::Schema => {
                self.advance();
                ObjectType::Schema
            }
            TokenKind::Tablespace => {
                self.advance();
                ObjectType::Tablespace
            }
            TokenKind::TypeP => {
                self.advance();
                ObjectType::Type
            }
            TokenKind::Parameter => {
                self.advance();
                ObjectType::ParameterAcl
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.advance();
                self.expect(TokenKind::Wrapper)?;
                ObjectType::Fdw
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Server => {
                self.advance();
                self.advance();
                ObjectType::ForeignServer
            }
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.advance();
                ObjectType::Propgraph
            }
            TokenKind::LargeP => {
                self.advance();
                self.expect(TokenKind::ObjectP)?;
                ObjectType::Largeobject
            }
            _ => ObjectType::Table,
        };
        let stops = [
            TokenKind::To,
            TokenKind::From,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        let objects = match objtype {
            ObjectType::Function | ObjectType::Procedure | ObjectType::Routine => {
                self.parse_object_with_args_list_until(&stops)?
            }
            ObjectType::Table | ObjectType::Sequence | ObjectType::Propgraph => {
                self.parse_privilege_qualified_name_list(&stops)?
            }
            ObjectType::Domain | ObjectType::Type => self.parse_any_name_list_until(&stops)?,
            ObjectType::Largeobject => self.parse_privilege_numeric_list(&stops)?,
            ObjectType::ParameterAcl => self.parse_parameter_name_list_until(&stops)?,
            _ => self.parse_simple_name_list_until(&stops)?,
        };
        if objects.is_empty() {
            return Err(self.error_here("expected at least one privilege target"));
        }
        Ok((GrantTargetType::Object, objtype, objects))
    }

    fn parse_privilege_qualified_name_list(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut objects = Vec::new();
        loop {
            objects.push(Node::RangeVar(
                self.try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("expected a qualified relation name"))?,
            ));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a qualified relation name after ','"));
            }
        }
        Ok(objects)
    }

    fn parse_privilege_numeric_list(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut objects = Vec::new();
        loop {
            objects.push(self.parse_numeric_only()?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a large object ID after ','"));
            }
        }
        Ok(objects)
    }

    fn parse_parameter_name_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut objects = Vec::new();
        loop {
            let mut parts = vec![
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("expected a parameter name"))?,
            ];
            while self.consume(TokenKind::Char('.')) {
                parts.push(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected a name after '.'"))?,
                );
            }
            objects.push(make_string_node(parts.join(".")));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a parameter name after ','"));
            }
        }
        Ok(objects)
    }

    pub(super) fn parse_role_specs_until(
        &mut self,
        stops: &[TokenKind],
        allow_group_prefix: bool,
    ) -> PResult<NodeList> {
        let mut roles = Vec::new();
        while !self.at_any(stops) {
            if allow_group_prefix {
                self.consume(TokenKind::GroupP);
            }
            let role = self
                .consume_role_spec()
                .ok_or_else(|| self.error_here("expected a role"))?;
            roles.push(Node::RoleSpec(role));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a role after ','"));
            }
        }
        if roles.is_empty() {
            return Err(self.error_here("expected at least one role"));
        }
        Ok(roles)
    }
}
