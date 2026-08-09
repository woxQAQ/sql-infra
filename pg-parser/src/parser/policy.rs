//! Row-level security policy creation and alteration.
//!
//! Commands, roles, `USING`, and `WITH CHECK` expressions are kept with their
//! policy-specific defaults and raw locations.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createpolicy.html
    // CREATE POLICY name ON table_name
    //     [ AS { PERMISSIVE | RESTRICTIVE } ]
    //     [ FOR { ALL | SELECT | INSERT | UPDATE | DELETE } ]
    //     [ TO { role_name | PUBLIC | CURRENT_ROLE | CURRENT_USER | SESSION_USER } [, ...] ]
    //     [ USING ( using_expression ) ]
    //     [ WITH CHECK ( check_expression ) ]
    pub(super) fn parse_create_policy(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Policy)?;
        self.record_completion_slot(completion::GrammarSlot::Policy);
        let policy_name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE POLICY requires a name"))?,
        );
        self.expect(TokenKind::On)?;
        let table = Some(Box::new(
            self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                .ok_or_else(|| self.error_here("CREATE POLICY requires a table"))?,
        ));
        let permissive = if self.consume(TokenKind::As) {
            let value = self
                .consume_identifier()
                .ok_or_else(|| self.error_here("AS requires PERMISSIVE or RESTRICTIVE"))?;
            if value == "permissive" {
                true
            } else if value == "restrictive" {
                false
            } else {
                return Err(self.error_here("policy AS mode must be PERMISSIVE or RESTRICTIVE"));
            }
        } else {
            true
        };
        let cmd_name = if self.consume(TokenKind::For) {
            self.record_completion_tokens(&[
                TokenKind::All,
                TokenKind::Select,
                TokenKind::Insert,
                TokenKind::Update,
                TokenKind::DeleteP,
            ]);
            let command = match self.advance().kind {
                TokenKind::All => "all",
                TokenKind::Select => "select",
                TokenKind::Insert => "insert",
                TokenKind::Update => "update",
                TokenKind::DeleteP => "delete",
                _ => return Err(self.error_here("invalid policy command")),
            };
            Some(command.to_owned())
        } else {
            Some("all".to_owned())
        };
        let roles = if self.consume(TokenKind::To) {
            self.parse_role_specs_until(
                &[
                    TokenKind::Using,
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ],
                false,
            )?
        } else {
            vec![Node::RoleSpec(RoleSpec {
                roletype: RoleSpecType::Public,
                rolename: None,
                location: -1,
            })]
        };
        let qual = if self.consume(TokenKind::Using) {
            self.expect(TokenKind::Char('('))?;
            let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
            self.expect(TokenKind::Char(')'))?;
            Some(expr)
        } else {
            None
        };
        let with_check = if self.consume(TokenKind::With) {
            self.expect(TokenKind::Check)?;
            self.expect(TokenKind::Char('('))?;
            let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
            self.expect(TokenKind::Char(')'))?;
            Some(expr)
        } else {
            None
        };
        Ok(Node::CreatePolicyStmt(CreatePolicyStmt {
            policy_name,
            table,
            cmd_name,
            permissive,
            roles,
            qual,
            with_check,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterpolicy.html
    // ALTER POLICY name ON table_name RENAME TO new_name
    //
    // ALTER POLICY name ON table_name
    //     [ TO { role_name | PUBLIC | CURRENT_ROLE | CURRENT_USER | SESSION_USER } [, ...] ]
    //     [ USING ( using_expression ) ]
    //     [ WITH CHECK ( check_expression ) ]
    pub(super) fn parse_alter_policy(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Policy)?;
        self.record_completion_slot(completion::GrammarSlot::Policy);
        let policy_name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER POLICY requires a policy name"))?,
        );
        self.expect(TokenKind::On)?;
        let table = Some(Box::new(
            self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                .ok_or_else(|| self.error_here("ALTER POLICY requires a table"))?,
        ));
        self.record_completion_tokens(&[TokenKind::Rename]);
        let roles = if self.consume(TokenKind::To) {
            let roles = self.parse_role_specs_until(
                &[
                    TokenKind::Using,
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ],
                false,
            )?;
            if roles.is_empty() {
                return Err(self.error_here("TO requires at least one role"));
            }
            roles
        } else {
            Vec::new()
        };
        let qual = if self.consume(TokenKind::Using) {
            self.expect(TokenKind::Char('('))?;
            let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
            self.expect(TokenKind::Char(')'))?;
            Some(expr)
        } else {
            None
        };
        let with_check = if self.consume(TokenKind::With) {
            self.expect(TokenKind::Check)?;
            self.expect(TokenKind::Char('('))?;
            let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
            self.expect(TokenKind::Char(')'))?;
            Some(expr)
        } else {
            None
        };
        self.expect_statement_end()?;
        Ok(Node::AlterPolicyStmt(AlterPolicyStmt {
            policy_name,
            table,
            roles,
            qual,
            with_check,
        }))
    }
}
