use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-delete.html
    // [ WITH [ RECURSIVE ] with_query [, ...] ]
    // DELETE FROM [ ONLY ] table_name [ * ] [ [ AS ] alias ]
    //     [ USING from_item [, ...] ]
    //     [ WHERE condition | WHERE CURRENT OF cursor_name ]
    //     [ RETURNING [ WITH ( { OLD | NEW } AS output_alias [, ...] ) ]
    //                 { * | output_expression [ [ AS ] output_name ] } [, ...] ]
    pub(super) fn parse_delete(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::DeleteP)?;
        self.expect(TokenKind::From)?;
        if self.at_completion_cursor() {
            self.record_relation_completion_at(CompletionSlot::DeleteTargetRelation);
            return Err(self.error_here("completion cursor"));
        }
        let mut relation = Some(Box::new(
            self.try_parse_range_var(true)
                .ok_or_else(|| self.error_here("DELETE FROM requires a relation name"))?,
        ));
        let for_portion_of = self.parse_for_portion_of_clause()?;
        if for_portion_of.is_some()
            && let Some(relation) = relation.as_mut()
        {
            relation.alias = self.parse_optional_alias(true);
        }
        let using_clause = if self.consume(TokenKind::Using) {
            let using_clause = self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
            if using_clause.is_empty() {
                return Err(self.error_here("DELETE USING requires at least one table reference"));
            }
            using_clause
        } else {
            Vec::new()
        };
        let where_clause = self.parse_where_or_current_clause(
            CompletionSlot::DeleteWhere,
            &[TokenKind::Returning, TokenKind::Char(';'), TokenKind::Eof],
        )?;
        let returning_clause = self.parse_returning_clause()?;
        Ok(Node::DeleteStmt(DeleteStmt {
            node_tag: NodeTag::DeleteStmt,
            relation,
            using_clause,
            where_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            for_portion_of,
        }))
    }
}
