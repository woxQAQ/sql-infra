use super::*;

impl Parser {
    pub(super) fn parse_insert(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Insert)?;
        self.expect(TokenKind::Into)?;
        let mut relation = self
            .try_parse_qualified_range_var()
            .ok_or_else(|| self.error_here("INSERT INTO requires a relation name"))?;
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
            let source = self.parse_statement(None)?;
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

    pub(super) fn parse_update(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Update)?;
        let mut relation = Some(Box::new(
            self.try_parse_range_var(false)
                .ok_or_else(|| self.error_here("UPDATE requires a relation name"))?,
        ));
        let for_portion_of = self.parse_for_portion_of_clause()?;
        if for_portion_of.is_some()
            && let Some(relation) = relation.as_mut()
        {
            relation.alias = self.parse_optional_alias(false);
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

    pub(super) fn parse_delete(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::DeleteP)?;
        self.expect(TokenKind::From)?;
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
        let where_clause = self.parse_where_or_current_clause(&[
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
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

    fn parse_where_or_current_clause(&mut self, stops: &[TokenKind]) -> PResult<Option<Box<Node>>> {
        if !self.consume(TokenKind::Where) {
            return Ok(None);
        }
        if self.consume(TokenKind::CurrentP) {
            self.expect(TokenKind::Of)?;
            let cursor_name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("CURRENT OF requires a cursor name"))?,
            );
            return Ok(Some(Box::new(Node::CurrentOfExpr(CurrentOfExpr {
                xpr: Expr::new(NodeTag::CurrentOfExpr),
                cursor_name,
                ..CurrentOfExpr::default()
            }))));
        }
        Ok(Some(self.parse_expr_box_strict_until(stops)?))
    }

    pub(super) fn parse_merge(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Merge)?;
        self.expect(TokenKind::Into)?;
        let relation = Some(Box::new(
            self.try_parse_range_var(true)
                .ok_or_else(|| self.error_here("MERGE INTO requires a relation name"))?,
        ));
        self.expect(TokenKind::Using)?;
        let source_relation = Some(Box::new(self.parse_from_item(&[TokenKind::On])?));
        self.expect(TokenKind::On)?;
        let join_condition = self.parse_expr_box_strict_until(&[
            TokenKind::When,
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        let merge_when_clauses = self.parse_merge_when_clauses()?;
        if merge_when_clauses.is_empty() {
            return Err(self.error_here("MERGE requires at least one WHEN clause"));
        }
        let returning_clause = self.parse_returning_clause()?;
        Ok(Node::MergeStmt(MergeStmt {
            node_tag: NodeTag::MergeStmt,
            relation,
            source_relation,
            join_condition: Some(join_condition),
            merge_when_clauses,
            returning_clause,
            with_clause: with_clause.map(Box::new),
        }))
    }
}
