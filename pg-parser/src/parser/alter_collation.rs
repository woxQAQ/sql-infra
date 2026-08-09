//! Parsing for `ALTER COLLATION` refresh-version statements.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altercollation.html
    // ALTER COLLATION name REFRESH VERSION
    //
    // ALTER COLLATION name RENAME TO new_name
    // ALTER COLLATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER COLLATION name SET SCHEMA new_schema
    pub(super) fn parse_alter_collation(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Collation)?;
        let name_stops = [TokenKind::Refresh, TokenKind::Char(';'), TokenKind::Eof];
        self.record_completion_slot(completion::GrammarSlot::Collation);
        self.record_completion_qualified_name_slot(completion::GrammarSlot::Collation, &name_stops);
        let collname = self.parse_name_list_until_keywords_allow_initial_stop(&name_stops);
        if collname.is_empty() {
            return Err(self.error_here("ALTER COLLATION requires a collation name"));
        }
        self.expect(TokenKind::Refresh)?;
        self.expect(TokenKind::VersionP)?;
        self.expect_statement_end()?;
        Ok(Node::AlterCollationStmt(AlterCollationStmt {
            node_tag: NodeTag::AlterCollationStmt,
            collname,
        }))
    }
}
