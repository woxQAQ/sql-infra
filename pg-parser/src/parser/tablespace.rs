//! Tablespace creation, alteration, and removal.
//!
//! Owner, location, option, rename, and ownership clauses retain their dedicated
//! PostgreSQL statement shapes.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtablespace.html
    // CREATE TABLESPACE tablespace_name
    //     [ OWNER { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER } ]
    //     LOCATION 'directory'
    //     [ WITH ( tablespace_option = value [, ... ] ) ]
    pub(super) fn parse_create_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        self.record_completion_slot(completion::GrammarSlot::Tablespace);
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE TABLESPACE requires a name"))?,
        );
        let owner = if self.consume(TokenKind::Owner) {
            self.record_completion_slot(completion::GrammarSlot::Role);
            Some(Box::new(
                self.consume_role_spec()
                    .ok_or_else(|| self.error_here("OWNER requires a role"))?,
            ))
        } else {
            None
        };
        self.expect(TokenKind::Location)?;
        let location = Some(self.consume_required_string("TABLESPACE LOCATION requires a string")?);
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            Vec::new()
        };
        Ok(Node::CreateTableSpaceStmt(CreateTableSpaceStmt {
            node_tag: NodeTag::CreateTableSpaceStmt,
            tablespacename,
            owner,
            location,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertablespace.html
    // ALTER TABLESPACE name RENAME TO new_name
    // ALTER TABLESPACE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TABLESPACE name SET ( tablespace_option = value [, ... ] )
    // ALTER TABLESPACE name RESET ( tablespace_option [, ... ] )
    pub(super) fn parse_alter_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        self.record_completion_slot(completion::GrammarSlot::Tablespace);
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER TABLESPACE requires a tablespace name"))?,
        );
        if self.consume(TokenKind::Rename) {
            self.expect(TokenKind::To)?;
            let newname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME TO requires a new name"))?,
            );
            return Ok(Node::RenameStmt(RenameStmt {
                node_tag: NodeTag::RenameStmt,
                rename_type: ObjectType::Tablespace,
                subname: tablespacename,
                newname,
                ..RenameStmt::default()
            }));
        }
        let is_reset = if self.consume(TokenKind::Set) {
            false
        } else if self.consume(TokenKind::Reset) {
            true
        } else {
            return Err(self.error_here("ALTER TABLESPACE requires SET or RESET"));
        };
        let options = self.parse_parenthesized_reloptions()?;
        Ok(Node::AlterTableSpaceOptionsStmt(
            AlterTableSpaceOptionsStmt {
                node_tag: NodeTag::AlterTableSpaceOptionsStmt,
                tablespacename,
                options,
                is_reset,
            },
        ))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-droptablespace.html
    // DROP TABLESPACE [ IF EXISTS ] name
    pub(super) fn parse_drop_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let missing_ok = self.consume_if_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Tablespace);
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DROP TABLESPACE requires a name"))?,
        );
        self.expect_statement_end()?;
        Ok(Node::DropTableSpaceStmt(DropTableSpaceStmt {
            node_tag: NodeTag::DropTableSpaceStmt,
            tablespacename,
            missing_ok,
        }))
    }
}
