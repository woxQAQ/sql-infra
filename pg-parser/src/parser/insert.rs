use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-insert.html
    // [ WITH [ RECURSIVE ] with_query [, ...] ]
    // INSERT INTO table_name [ AS alias ] [ ( column_name [, ...] ) ]
    //     [ OVERRIDING { SYSTEM | USER } VALUE ]
    //     { DEFAULT VALUES | VALUES ( { expression | DEFAULT } [, ...] ) [, ...] | query }
    //     [ ON CONFLICT [ conflict_target ] conflict_action ]
    //     [ RETURNING [ WITH ( { OLD | NEW } AS output_alias [, ...] ) ]
    //                 { * | output_expression [ [ AS ] output_name ] } [, ...] ]
    //
    // where conflict_target can be one of:
    //
    //     ( { index_column_name | ( index_expression ) } [ COLLATE collation ] [ opclass ] [, ...] ) [ WHERE index_predicate ]
    //     ON CONSTRAINT constraint_name
    //
    // and conflict_action is one of:
    //
    //     DO NOTHING
    //     DO UPDATE SET { column_name = { expression | DEFAULT } |
    //                     ( column_name [, ...] ) = [ ROW ] ( { expression | DEFAULT } [, ...] ) |
    //                     ( column_name [, ...] ) = ( sub-SELECT )
    //                   } [, ...]
    //               [ WHERE condition ]
    pub(super) fn parse_insert(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Insert)?;
        self.expect(TokenKind::Into)?;
        if self.at_completion_cursor() {
            self.record_relation_completion_at(CompletionSlot::InsertTargetRelation);
            return Err(self.completion_stop());
        }
        let mut relation =
            self.parse_qualified_range_var_at(CompletionSlot::InsertTargetRelation, false)?;
        if self.consume(TokenKind::As) {
            relation.alias = Some(Box::new(Alias {
                node_tag: NodeTag::Alias,
                aliasname: Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("INSERT AS requires an alias"))?,
                ),
                ..Alias::default()
            }));
        }
        let relation = Some(Box::new(relation));
        let mut cols = Vec::new();
        if self.at(TokenKind::Char('('))
            && !matches!(
                self.peek_kind_n(1),
                TokenKind::Select
                    | TokenKind::With
                    | TokenKind::Values
                    | TokenKind::Table
                    | TokenKind::Char('(')
            )
        {
            self.advance();
            cols = self.parse_insert_column_list()?;
            self.expect(TokenKind::Char(')'))?;
        }
        let override_ = if self.consume(TokenKind::Overriding) {
            let kind = if self.consume(TokenKind::User) {
                OverridingKind::UserValue
            } else if self.consume(TokenKind::SystemP) {
                OverridingKind::SystemValue
            } else {
                return Err(self.error_here("expected USER or SYSTEM after OVERRIDING"));
            };
            self.expect(TokenKind::ValueP)?;
            kind
        } else {
            OverridingKind::NotSet
        };
        let select_stmt = if self.consume(TokenKind::Default) {
            if !cols.is_empty() || override_ != OverridingKind::NotSet {
                return Err(
                    self.error_here("INSERT DEFAULT VALUES does not accept columns or OVERRIDING")
                );
            }
            self.expect(TokenKind::Values)?;
            None
        } else if matches!(
            self.peek_kind(),
            TokenKind::Select
                | TokenKind::Values
                | TokenKind::With
                | TokenKind::Table
                | TokenKind::Char('(')
        ) {
            let default_allowed = self.at(TokenKind::Values);
            let previous_default_allowed = self.replace_completion_default_allowed(default_allowed);
            let source = self.parse_statement(None);
            if let Some(previous) = previous_default_allowed {
                self.replace_completion_default_allowed(previous);
            }
            let source = source?;
            if !matches!(source, Node::SelectStmt(_)) {
                return Err(self.error_here("INSERT source must be a SELECT statement"));
            }
            Some(Box::new(source))
        } else {
            return Err(self.error_here("INSERT requires DEFAULT VALUES or a query"));
        };
        let on_conflict_clause = self.parse_on_conflict_clause()?;
        let returning_clause = self.parse_returning_clause()?;
        Ok(Node::InsertStmt(InsertStmt {
            node_tag: NodeTag::InsertStmt,
            relation,
            cols,
            select_stmt,
            on_conflict_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            override_,
        }))
    }
}
