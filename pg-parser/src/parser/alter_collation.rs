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
        let collname = self.parse_name_list_until_keywords(&[
            TokenKind::Refresh,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
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
