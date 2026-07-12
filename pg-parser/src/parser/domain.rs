use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createdomain.html
    // CREATE DOMAIN name [ AS ] data_type
    //     [ COLLATE collation ]
    //     [ DEFAULT expression ]
    //     [ domain_constraint [ ... ] ]
    //
    // where domain_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL | NULL | CHECK (expression) }
    pub(super) fn parse_create_domain(&mut self) -> PResult<Node> {
        self.expect(TokenKind::DomainP)?;
        let domainname = self.parse_name_list();
        if domainname.is_empty() {
            return Err(self.error_here("CREATE DOMAIN requires a domain name"));
        }
        self.consume(TokenKind::As);
        let type_name = Some(Box::new(
            self.parse_type_name_until(&[
                TokenKind::Collate,
                TokenKind::Default,
                TokenKind::Constraint,
                TokenKind::Not,
                TokenKind::NullP,
                TokenKind::Check,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
            .ok_or_else(|| self.error_here("CREATE DOMAIN requires a base type"))?,
        ));
        let mut coll_clause = None;
        let mut constraints = Vec::new();
        while !self.at_statement_end() {
            if self.consume(TokenKind::Collate) {
                let location = self.previous_location();
                let collname = self.parse_name_list();
                if collname.is_empty() {
                    return Err(self.error_here("COLLATE requires a collation name"));
                }
                coll_clause = Some(Box::new(CollateClause {
                    node_tag: NodeTag::CollateClause,
                    collname,
                    location: location as ParseLoc,
                    ..CollateClause::default()
                }));
                continue;
            }
            let location = self.location();
            let conname = if self.consume(TokenKind::Constraint) {
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?,
                )
            } else {
                None
            };
            let mut constraint = Constraint {
                node_tag: NodeTag::Constraint,
                conname,
                location: location as ParseLoc,
                initially_valid: true,
                is_enforced: true,
                ..Constraint::default()
            };
            if self.consume(TokenKind::Default) {
                constraint.contype = ConstrType::Default;
                constraint.raw_expr = Some(self.parse_expr_box_strict_until(&[
                    TokenKind::Constraint,
                    TokenKind::Not,
                    TokenKind::Check,
                    TokenKind::Collate,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])?);
            } else if self.consume(TokenKind::Not) {
                self.expect(TokenKind::NullP)?;
                constraint.contype = ConstrType::Notnull;
            } else if self.consume(TokenKind::NullP) {
                constraint.contype = ConstrType::Null;
            } else if self.consume(TokenKind::Check) {
                constraint.contype = ConstrType::Check;
                self.expect(TokenKind::Char('('))?;
                constraint.raw_expr =
                    Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
                self.expect(TokenKind::Char(')'))?;
            } else {
                return Err(self.error_here("invalid domain constraint"));
            }
            constraints.push(Node::Constraint(constraint));
        }
        Ok(Node::CreateDomainStmt(CreateDomainStmt {
            node_tag: NodeTag::CreateDomainStmt,
            domainname,
            type_name,
            coll_clause,
            constraints,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdomain.html
    // ALTER DOMAIN name
    //     { SET DEFAULT expression | DROP DEFAULT }
    // ALTER DOMAIN name
    //     { SET | DROP } NOT NULL
    // ALTER DOMAIN name
    //     ADD domain_constraint [ NOT VALID ]
    // ALTER DOMAIN name
    //     DROP CONSTRAINT [ IF EXISTS ] constraint_name [ RESTRICT | CASCADE ]
    // ALTER DOMAIN name
    //      RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER DOMAIN name
    //     VALIDATE CONSTRAINT constraint_name
    // ALTER DOMAIN name
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER DOMAIN name
    //     RENAME TO new_name
    // ALTER DOMAIN name
    //     SET SCHEMA new_schema
    //
    // where domain_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL | CHECK (expression) }
    pub(super) fn parse_alter_domain(&mut self) -> PResult<Node> {
        self.expect(TokenKind::DomainP)?;
        let type_name = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Drop,
            TokenKind::AddP,
            TokenKind::Validate,
            TokenKind::Rename,
            TokenKind::Owner,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let mut stmt = AlterDomainStmt {
            node_tag: NodeTag::AlterDomainStmt,
            type_name,
            ..AlterDomainStmt::default()
        };
        if stmt.type_name.is_empty() {
            return Err(self.error_here("ALTER DOMAIN requires a domain name"));
        }
        match self.peek_kind() {
            TokenKind::Set => {
                self.advance();
                if self.consume(TokenKind::Default) {
                    stmt.subtype = AlterDomainType::AlterDefault;
                    stmt.def =
                        Some(self.parse_expr_box_strict_until(&[
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ])?);
                } else if self.consume(TokenKind::Not) {
                    self.expect(TokenKind::NullP)?;
                    stmt.subtype = AlterDomainType::SetNotNull;
                } else {
                    return Err(self.error_here("ALTER DOMAIN SET requires DEFAULT or NOT NULL"));
                }
            }
            TokenKind::Drop => {
                self.advance();
                if self.consume(TokenKind::Default) {
                    stmt.subtype = AlterDomainType::AlterDefault;
                } else if self.consume(TokenKind::Not) {
                    self.expect(TokenKind::NullP)?;
                    stmt.subtype = AlterDomainType::DropNotNull;
                } else if self.consume(TokenKind::Constraint) {
                    stmt.subtype = AlterDomainType::DropConstraint;
                    stmt.missing_ok = self.consume_if_exists()?;
                    stmt.name = Some(
                        self.consume_col_id()
                            .ok_or_else(|| self.error_here("DROP CONSTRAINT requires a name"))?,
                    );
                    stmt.behavior = self.parse_drop_behavior();
                } else {
                    return Err(self.error_here(
                        "ALTER DOMAIN DROP requires DEFAULT, NOT NULL, or CONSTRAINT",
                    ));
                }
            }
            TokenKind::AddP => {
                self.advance();
                stmt.subtype = AlterDomainType::AddConstraint;
                stmt.def = Some(Box::new(Node::Constraint(self.parse_domain_constraint()?)));
            }
            TokenKind::Validate => {
                self.advance();
                self.expect(TokenKind::Constraint)?;
                stmt.subtype = AlterDomainType::ValidateConstraint;
                stmt.name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("VALIDATE CONSTRAINT requires a name"))?,
                );
            }
            _ => return Err(self.error_here("unsupported ALTER DOMAIN action")),
        }
        self.expect_statement_end()?;
        Ok(Node::AlterDomainStmt(stmt))
    }

    fn parse_domain_constraint(&mut self) -> PResult<Constraint> {
        let location = self.location();
        let conname = if self.consume(TokenKind::Constraint) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?,
            )
        } else {
            None
        };
        let (contype, raw_expr, keys) = match self.peek_kind() {
            TokenKind::Check => {
                self.advance();
                self.expect(TokenKind::Char('('))?;
                let raw_expr = Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
                self.expect(TokenKind::Char(')'))?;
                (ConstrType::Check, raw_expr, Vec::new())
            }
            TokenKind::Not => {
                self.advance();
                self.expect(TokenKind::NullP)?;
                (ConstrType::Notnull, None, vec![make_string_node("value")])
            }
            _ => return Err(self.error_here("domain constraint must be CHECK or NOT NULL")),
        };
        let mut constraint = Constraint {
            node_tag: NodeTag::Constraint,
            conname,
            contype,
            raw_expr,
            keys,
            is_enforced: true,
            initially_valid: true,
            location: location as ParseLoc,
            ..Constraint::default()
        };

        let mut deferrable = None;
        let mut initially_deferred = None;
        let mut enforced = None;
        loop {
            match self.peek_kind() {
                TokenKind::Deferrable => {
                    self.advance();
                    self.set_constraint_attribute(&mut deferrable, true)?;
                    constraint.deferrable = true;
                }
                TokenKind::Initially => {
                    self.advance();
                    let value = match self.peek_kind() {
                        TokenKind::Immediate => false,
                        TokenKind::Deferred => true,
                        _ => {
                            return Err(self.error_here("INITIALLY requires IMMEDIATE or DEFERRED"));
                        }
                    };
                    self.advance();
                    self.set_constraint_attribute(&mut initially_deferred, value)?;
                    constraint.initdeferred = value;
                }
                TokenKind::No => {
                    self.advance();
                    self.expect(TokenKind::Inherit)?;
                    constraint.is_no_inherit = true;
                }
                TokenKind::Enforced => {
                    self.advance();
                    self.set_constraint_attribute(&mut enforced, true)?;
                    constraint.is_enforced = true;
                }
                TokenKind::Not => {
                    self.advance();
                    match self.peek_kind() {
                        TokenKind::Deferrable => {
                            self.advance();
                            self.set_constraint_attribute(&mut deferrable, false)?;
                            constraint.deferrable = false;
                        }
                        TokenKind::Valid => {
                            self.advance();
                            constraint.skip_validation = true;
                            constraint.initially_valid = false;
                        }
                        TokenKind::Enforced => {
                            self.advance();
                            self.set_constraint_attribute(&mut enforced, false)?;
                            constraint.is_enforced = false;
                        }
                        _ => {
                            return Err(self.error_here("invalid constraint attribute after NOT"));
                        }
                    }
                }
                _ => break,
            }
        }
        if deferrable == Some(false) && initially_deferred == Some(true) {
            return Err(self.error_here("conflicting constraint attributes"));
        }
        Ok(constraint)
    }

    fn set_constraint_attribute(&self, state: &mut Option<bool>, value: bool) -> PResult<()> {
        if state.is_some_and(|current| current != value) {
            return Err(self.error_here("conflicting constraint attributes"));
        }
        *state = Some(value);
        Ok(())
    }
}
