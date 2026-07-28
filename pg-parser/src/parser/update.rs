use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-update.html
    // [ WITH [ RECURSIVE ] with_query [, ...] ]
    // UPDATE [ ONLY ] table_name [ * ] [ [ AS ] alias ]
    //     SET { column_name = { expression | DEFAULT } |
    //           ( column_name [, ...] ) = [ ROW ] ( { expression | DEFAULT } [, ...] ) |
    //           ( column_name [, ...] ) = ( sub-SELECT )
    //         } [, ...]
    //     [ FROM from_item [, ...] ]
    //     [ WHERE condition | WHERE CURRENT OF cursor_name ]
    //     [ RETURNING [ WITH ( { OLD | NEW } AS output_alias [, ...] ) ]
    //                 { * | output_expression [ [ AS ] output_name ] } [, ...] ]
    pub(super) fn parse_update(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Update)?;
        let mut relation = Some(Box::new(
            self.try_parse_range_var_with_slot(false, completion::GrammarSlot::Table)?
                .ok_or_else(|| self.error_here("UPDATE requires a relation name"))?,
        ));
        let for_portion_of = self.parse_for_portion_of_clause()?;
        if for_portion_of.is_some()
            && let Some(relation) = relation.as_mut()
        {
            relation.alias = self.parse_optional_alias(false)?;
        }
        self.expect(TokenKind::Set)?;
        let target_list = self.parse_set_clause_list_until(&[
            TokenKind::From,
            TokenKind::Where,
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        if target_list.is_empty() {
            return Err(self.error_here("UPDATE SET requires at least one assignment"));
        }
        let from_clause = if self.consume(TokenKind::From) {
            let from_clause = self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
            if from_clause.is_empty() {
                return Err(self.error_here("UPDATE FROM requires at least one table reference"));
            }
            from_clause
        } else {
            Vec::new()
        };
        let where_clause = self.parse_where_or_current_clause(&[
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        let returning_clause = self.parse_returning_clause()?;
        Ok(Node::UpdateStmt(UpdateStmt {
            node_tag: NodeTag::UpdateStmt,
            relation,
            target_list,
            from_clause,
            where_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            for_portion_of,
        }))
    }
}
