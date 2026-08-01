use super::*;

const ACCESS_PRIVILEGE_STARTS: &[TokenKind] = &[
    TokenKind::All,
    TokenKind::Alter,
    TokenKind::Create,
    TokenKind::DeleteP,
    TokenKind::Execute,
    TokenKind::Insert,
    TokenKind::References,
    TokenKind::Select,
    TokenKind::Set,
    TokenKind::Temp,
    TokenKind::Temporary,
    TokenKind::Trigger,
    TokenKind::Truncate,
    TokenKind::Update,
];

const PRIVILEGE_TARGET_STARTS: &[TokenKind] = &[
    TokenKind::All,
    TokenKind::Table,
    TokenKind::Sequence,
    TokenKind::Function,
    TokenKind::Procedure,
    TokenKind::Routine,
    TokenKind::Database,
    TokenKind::DomainP,
    TokenKind::Language,
    TokenKind::Schema,
    TokenKind::Tablespace,
    TokenKind::TypeP,
    TokenKind::Parameter,
    TokenKind::Foreign,
    TokenKind::Property,
    TokenKind::LargeP,
];

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum GrantKind {
    Grant,
    Revoke,
}

impl GrantKind {
    fn is_grant(self) -> bool {
        self == Self::Grant
    }

    fn grantee_separator(self) -> TokenKind {
        match self {
            Self::Grant => TokenKind::To,
            Self::Revoke => TokenKind::From,
        }
    }
}

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html
    // ALTER DEFAULT PRIVILEGES
    //     [ FOR { ROLE | USER } target_role [, ...] ]
    //     [ IN SCHEMA schema_name [, ...] ]
    //     abbreviated_grant_or_revoke
    //
    // where abbreviated_grant_or_revoke is one of:
    //
    // GRANT { { SELECT | INSERT | UPDATE | DELETE | TRUNCATE | REFERENCES | TRIGGER | MAINTAIN }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON TABLES
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // GRANT { { USAGE | SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON SEQUENCES
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // GRANT { EXECUTE | ALL [ PRIVILEGES ] }
    //     ON { FUNCTIONS | ROUTINES }
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON TYPES
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // GRANT { { USAGE | CREATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON SCHEMAS
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // GRANT { { SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON LARGE OBJECTS
    //     TO { [ GROUP ] role_name | PUBLIC } [, ...] [ WITH GRANT OPTION ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SELECT | INSERT | UPDATE | DELETE | TRUNCATE | REFERENCES | TRIGGER | MAINTAIN }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON TABLES
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { USAGE | SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON SEQUENCES
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { EXECUTE | ALL [ PRIVILEGES ] }
    //     ON { FUNCTIONS | ROUTINES }
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON TYPES
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { USAGE | CREATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON SCHEMAS
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON LARGE OBJECTS
    //     FROM { [ GROUP ] role_name | PUBLIC } [, ...]
    //     [ CASCADE | RESTRICT ]
    pub(super) fn parse_alter_default_privileges(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Default)?;
        self.expect(TokenKind::Privileges)?;
        let mut options = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Grant | TokenKind::Revoke | TokenKind::Eof
        ) {
            self.record_completion_tokens(&[
                TokenKind::For,
                TokenKind::InP,
                TokenKind::Grant,
                TokenKind::Revoke,
            ]);
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
                    self.record_completion_slot(completion::GrammarSlot::Schema);
                    let schemas = self.parse_simple_name_list_until(
                        &[
                            TokenKind::Grant,
                            TokenKind::Revoke,
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ],
                        completion::GrammarSlot::Schema,
                    )?;
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
        self.record_completion_tokens(ACCESS_PRIVILEGE_STARTS);
        let privileges = self.parse_access_privileges()?;
        self.expect(TokenKind::On)?;
        self.record_completion_tokens(&[
            TokenKind::Tables,
            TokenKind::Functions,
            TokenKind::Routines,
            TokenKind::Sequences,
            TokenKind::TypesP,
            TokenKind::Schemas,
            TokenKind::LargeP,
        ]);
        let objtype = match self.peek_kind() {
            TokenKind::Tables => ObjectType::Table,
            TokenKind::Functions | TokenKind::Routines => ObjectType::Function,
            TokenKind::Sequences => ObjectType::Sequence,
            TokenKind::TypesP => ObjectType::Type,
            TokenKind::Schemas => ObjectType::Schema,
            TokenKind::LargeP => {
                self.advance();
                self.expect(TokenKind::ObjectsP)?;
                ObjectType::Largeobject
            }
            _ => return Err(self.error_here("invalid default privilege target")),
        };
        if objtype != ObjectType::Largeobject {
            self.advance();
        }
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-grant.html
    // GRANT { { SELECT | INSERT | UPDATE | DELETE | TRUNCATE | REFERENCES | TRIGGER | MAINTAIN }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON { [ TABLE ] table_name [, ...]
    //          | ALL TABLES IN SCHEMA schema_name [, ...] }
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { SELECT | INSERT | UPDATE | REFERENCES } ( column_name [, ...] )
    //     [, ...] | ALL [ PRIVILEGES ] ( column_name [, ...] ) }
    //     ON [ TABLE ] table_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { USAGE | SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON { SEQUENCE sequence_name [, ...]
    //          | ALL SEQUENCES IN SCHEMA schema_name [, ...] }
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { CREATE | CONNECT | TEMPORARY | TEMP } [, ...] | ALL [ PRIVILEGES ] }
    //     ON DATABASE database_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON DOMAIN domain_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON FOREIGN DATA WRAPPER fdw_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON FOREIGN SERVER server_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { EXECUTE | ALL [ PRIVILEGES ] }
    //     ON { { FUNCTION | PROCEDURE | ROUTINE } routine_name [ ( [ [ argmode ] [ arg_name ] arg_type [, ...] ] ) ] [, ...]
    //          | ALL { FUNCTIONS | PROCEDURES | ROUTINES } IN SCHEMA schema_name [, ...] }
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON LANGUAGE lang_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { SELECT | UPDATE } [, ...] | ALL [ PRIVILEGES ] }
    //     ON LARGE OBJECT loid [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { SET | ALTER SYSTEM } [, ... ] | ALL [ PRIVILEGES ] }
    //     ON PARAMETER configuration_parameter [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { { CREATE | USAGE } [, ...] | ALL [ PRIVILEGES ] }
    //     ON SCHEMA schema_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { CREATE | ALL [ PRIVILEGES ] }
    //     ON TABLESPACE tablespace_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT { USAGE | ALL [ PRIVILEGES ] }
    //     ON TYPE type_name [, ...]
    //     TO role_specification [, ...] [ WITH GRANT OPTION ]
    //     [ GRANTED BY role_specification ]
    //
    // GRANT role_name [, ...] TO role_specification [, ...]
    //     [ WITH { ADMIN | INHERIT | SET } { OPTION | TRUE | FALSE } ]
    //     [ GRANTED BY role_specification ]
    //
    // where role_specification can be:
    //
    //     [ GROUP ] role_name
    //   | PUBLIC
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-revoke.html
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SELECT | INSERT | UPDATE | DELETE | TRUNCATE | REFERENCES | TRIGGER | MAINTAIN }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON { [ TABLE ] table_name [, ...]
    //          | ALL TABLES IN SCHEMA schema_name [, ...] }
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SELECT | INSERT | UPDATE | REFERENCES } ( column_name [, ...] )
    //     [, ...] | ALL [ PRIVILEGES ] ( column_name [, ...] ) }
    //     ON [ TABLE ] table_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { USAGE | SELECT | UPDATE }
    //     [, ...] | ALL [ PRIVILEGES ] }
    //     ON { SEQUENCE sequence_name [, ...]
    //          | ALL SEQUENCES IN SCHEMA schema_name [, ...] }
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { CREATE | CONNECT | TEMPORARY | TEMP } [, ...] | ALL [ PRIVILEGES ] }
    //     ON DATABASE database_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON DOMAIN domain_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON FOREIGN DATA WRAPPER fdw_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON FOREIGN SERVER server_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { EXECUTE | ALL [ PRIVILEGES ] }
    //     ON { { FUNCTION | PROCEDURE | ROUTINE } function_name [ ( [ [ argmode ] [ arg_name ] arg_type [, ...] ] ) ] [, ...]
    //          | ALL { FUNCTIONS | PROCEDURES | ROUTINES } IN SCHEMA schema_name [, ...] }
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON LANGUAGE lang_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SELECT | UPDATE } [, ...] | ALL [ PRIVILEGES ] }
    //     ON LARGE OBJECT loid [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { SET | ALTER SYSTEM } [, ...] | ALL [ PRIVILEGES ] }
    //     ON PARAMETER configuration_parameter [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { { CREATE | USAGE } [, ...] | ALL [ PRIVILEGES ] }
    //     ON SCHEMA schema_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { CREATE | ALL [ PRIVILEGES ] }
    //     ON TABLESPACE tablespace_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ GRANT OPTION FOR ]
    //     { USAGE | ALL [ PRIVILEGES ] }
    //     ON TYPE type_name [, ...]
    //     FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // REVOKE [ { ADMIN | INHERIT | SET } OPTION FOR ]
    //     role_name [, ...] FROM role_specification [, ...]
    //     [ GRANTED BY role_specification ]
    //     [ CASCADE | RESTRICT ]
    //
    // where role_specification can be:
    //
    //     [ GROUP ] role_name
    //   | PUBLIC
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    pub(super) fn parse_grant(&mut self, kind: GrantKind) -> PResult<Node> {
        self.advance();
        self.record_completion_tokens(ACCESS_PRIVILEGE_STARTS);
        self.record_completion_slot(completion::GrammarSlot::Privilege);
        self.record_completion_slot(completion::GrammarSlot::Role);
        let mut revoke_grant_option = false;
        if kind == GrantKind::Revoke && self.consume(TokenKind::Grant) {
            self.expect(TokenKind::Option)?;
            self.expect(TokenKind::For)?;
            revoke_grant_option = true;
        }

        if self.has_top_level_token_before(
            TokenKind::On,
            &[TokenKind::To, TokenKind::From, TokenKind::Eof],
        ) {
            return self.parse_object_grant(kind, revoke_grant_option);
        }
        self.parse_role_grant(kind)
    }

    fn parse_object_grant(&mut self, kind: GrantKind, revoke_grant_option: bool) -> PResult<Node> {
        self.record_completion_tokens(ACCESS_PRIVILEGE_STARTS);
        self.record_completion_slot(completion::GrammarSlot::Privilege);
        let privileges = self.parse_access_privileges()?;
        self.expect(TokenKind::On)?;
        let (targtype, objtype, objects) = self.parse_privilege_target()?;
        self.expect(kind.grantee_separator())?;
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
        let grant_option = if kind.is_grant() && self.consume(TokenKind::With) {
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
        let behavior = if kind.is_grant() {
            DropBehavior::Restrict
        } else {
            self.parse_drop_behavior()
        };
        Ok(Node::GrantStmt(GrantStmt {
            node_tag: NodeTag::GrantStmt,
            is_grant: kind.is_grant(),
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

    fn parse_role_grant(&mut self, kind: GrantKind) -> PResult<Node> {
        let mut role_options = Vec::new();
        if kind == GrantKind::Revoke && self.peek_kind_n(1) == TokenKind::Completion {
            self.advance();
            self.record_completion_tokens(&[TokenKind::Option, TokenKind::On, TokenKind::From]);
            return Err(self.error_here("expected a REVOKE continuation"));
        }
        if kind == GrantKind::Revoke
            && self.peek_kind_n(1) == TokenKind::Option
            && matches!(self.peek_kind_n(2), TokenKind::For | TokenKind::Completion)
        {
            let location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a role option name"))?;
            self.expect(TokenKind::Option)?;
            self.expect(TokenKind::For)?;
            role_options.push(make_def_elem(
                &name,
                Some(Node::Boolean(Boolean::new(false))),
                location,
            ));
        }
        let separator = kind.grantee_separator();
        if self.peek_kind() == TokenKind::All && !self.top_level_contains(TokenKind::Completion) {
            return Err(self.error_here("GRANT/REVOKE ROLE requires an explicit role list"));
        }
        self.record_completion_slot(completion::GrammarSlot::Role);
        let granted_roles = self.parse_access_privileges_until(separator)?;
        self.record_completion_tokens(&[TokenKind::On]);
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
        if kind.is_grant() && self.consume(TokenKind::With) {
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
                role_options.push(make_def_elem(
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
        let behavior = if kind.is_grant() {
            DropBehavior::Restrict
        } else {
            self.parse_drop_behavior()
        };
        Ok(Node::GrantRoleStmt(GrantRoleStmt {
            node_tag: NodeTag::GrantRoleStmt,
            granted_roles,
            grantee_roles,
            is_grant: kind.is_grant(),
            opt: role_options,
            grantor,
            behavior,
        }))
    }

    pub(super) fn parse_access_privileges(&mut self) -> PResult<NodeList> {
        self.parse_access_privileges_until(TokenKind::On)
    }

    fn parse_access_privileges_until(&mut self, stop: TokenKind) -> PResult<NodeList> {
        self.record_completion_slot(completion::GrammarSlot::Privilege);
        if stop != TokenKind::On {
            self.record_completion_slot(completion::GrammarSlot::Role);
        }
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
        while self.at_completion() || !self.at(stop) {
            self.record_completion_slot(completion::GrammarSlot::Privilege);
            if stop != TokenKind::On {
                self.record_completion_slot(completion::GrammarSlot::Role);
            }
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
            if self.peek_kind() == stop {
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
        self.record_completion_slot(completion::GrammarSlot::Column);
        self.request_completion_membership_recovery();
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
        self.record_completion_tokens(PRIVILEGE_TARGET_STARTS);
        if self.consume(TokenKind::All) {
            self.record_completion_tokens(&[
                TokenKind::Tables,
                TokenKind::Sequences,
                TokenKind::Functions,
                TokenKind::Procedures,
                TokenKind::Routines,
            ]);
            let objtype = match self.peek_kind() {
                TokenKind::Tables => ObjectType::Table,
                TokenKind::Sequences => ObjectType::Sequence,
                TokenKind::Functions => ObjectType::Function,
                TokenKind::Procedures => ObjectType::Procedure,
                TokenKind::Routines => ObjectType::Routine,
                _ => return Err(self.error_here("expected an object class after ON ALL")),
            };
            self.advance();
            self.expect(TokenKind::InP)?;
            self.expect(TokenKind::Schema)?;
            self.record_completion_slot(completion::GrammarSlot::Schema);
            let objects = self.parse_simple_name_list_until(
                &[
                    TokenKind::To,
                    TokenKind::From,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ],
                completion::GrammarSlot::Schema,
            )?;
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
            TokenKind::Foreign => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::DataP, TokenKind::Server]);
                if self.consume(TokenKind::DataP) {
                    self.expect(TokenKind::Wrapper)?;
                    ObjectType::Fdw
                } else if self.consume(TokenKind::Server) {
                    ObjectType::ForeignServer
                } else {
                    return Err(self.error_here("expected DATA WRAPPER or SERVER after FOREIGN"));
                }
            }
            TokenKind::Property => {
                self.advance();
                self.expect(TokenKind::Graph)?;
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
        let object_slot = completion::object_type_slot(objtype);
        self.record_completion_slot(object_slot);
        self.record_completion_slot_before(object_slot, &stops);
        let owner_start = self.pos;
        let objects = match objtype {
            ObjectType::Function | ObjectType::Procedure | ObjectType::Routine => {
                self.parse_object_with_args_list_until_with_slot(&stops, object_slot)?
            }
            ObjectType::Table | ObjectType::Sequence | ObjectType::Propgraph => {
                self.parse_privilege_qualified_name_list_with_slot(&stops, object_slot)?
            }
            ObjectType::Domain | ObjectType::Type => {
                self.parse_any_name_list_until_with_slot(&stops, object_slot)?
            }
            ObjectType::Largeobject => self.parse_privilege_numeric_list(&stops)?,
            ObjectType::ParameterAcl => self.parse_parameter_name_list_until(&stops)?,
            _ => self.parse_simple_name_list_until(&stops, object_slot)?,
        };
        if objects.is_empty() {
            return Err(self.error_here("expected at least one privilege target"));
        }
        let owner_end = self.pos;
        if objtype == ObjectType::Table && objects.len() == 1 {
            self.push_completion_membership_owner_range(
                &[completion::GrammarSlot::Column],
                &[
                    ObjectType::Table,
                    ObjectType::View,
                    ObjectType::Matview,
                    ObjectType::ForeignTable,
                ],
                owner_start,
                owner_end,
            );
        }
        Ok((GrantTargetType::Object, objtype, objects))
    }

    fn parse_privilege_qualified_name_list_with_slot(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<NodeList> {
        let mut objects = Vec::new();
        loop {
            objects.push(Node::RangeVar(
                self.try_parse_qualified_range_var_with_slot(slot)
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
        while self.at_completion() || !self.at_any(stops) {
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
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected a role after ','"));
            }
        }
        if roles.is_empty() {
            return Err(self.error_here("expected at least one role"));
        }
        Ok(roles)
    }
}
