use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-create-access-method.html
    // CREATE ACCESS METHOD name
    //     TYPE access_method_type
    //     HANDLER handler_function
    pub(super) fn parse_create_am(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Method)?;
        let amname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE ACCESS METHOD requires a name"))?,
        );
        self.expect(TokenKind::TypeP)?;
        let amtype = if self.consume(TokenKind::Index) {
            b'i'
        } else if self.consume(TokenKind::Table) {
            b't'
        } else {
            return Err(self.error_here("access method TYPE must be INDEX or TABLE"));
        };
        self.expect(TokenKind::Handler)?;
        let handler_name = self.parse_name_list();
        if handler_name.is_empty() {
            return Err(self.error_here("access method HANDLER requires a function name"));
        }
        Ok(Node::CreateAmStmt(CreateAmStmt {
            node_tag: NodeTag::CreateAmStmt,
            amname,
            handler_name,
            amtype,
        }))
    }
}
