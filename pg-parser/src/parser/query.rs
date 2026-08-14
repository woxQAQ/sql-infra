//! Query-expression orchestration for `WITH`, `SELECT`, `VALUES`, and set operations.
//!
//! This module makes the major query phases visible—CTEs, primary forms, set
//! precedence, result clauses, locking, and limits—while delegating list and range
//! details to focused modules.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WithTarget {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

impl Parser {
    pub(super) fn parse_with_statement(&mut self) -> PResult<Node> {
        let with = self.parse_with_clause()?;
        self.record_completion_tokens(&[
            TokenKind::Select,
            TokenKind::Values,
            TokenKind::Table,
            TokenKind::Insert,
            TokenKind::Update,
            TokenKind::DeleteP,
            TokenKind::Merge,
        ]);
        let target = match self.peek_kind() {
            TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::Char('(') => {
                WithTarget::Select
            }
            TokenKind::Insert => WithTarget::Insert,
            TokenKind::Update => WithTarget::Update,
            TokenKind::DeleteP => WithTarget::Delete,
            TokenKind::Merge => WithTarget::Merge,
            _ => {
                return Err(self.error_here(
                    "WITH must be followed by SELECT, INSERT, UPDATE, DELETE, or MERGE",
                ));
            }
        };

        match target {
            WithTarget::Select => Ok(Node::SelectStmt(self.parse_select(Some(with))?)),
            WithTarget::Insert => self.parse_insert(Some(with)),
            WithTarget::Update => self.parse_update(Some(with)),
            WithTarget::Delete => self.parse_delete(Some(with)),
            WithTarget::Merge => self.parse_merge(Some(with)),
        }
    }

    fn parse_with_clause(&mut self) -> PResult<WithClause> {
        let location = self.expect(TokenKind::With)?.location();
        let recursive = self.consume(TokenKind::Recursive);
        let mut ctes = Vec::new();

        loop {
            let cte_location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("WITH requires a common table expression name"))?;
            let mut aliascolnames = Vec::new();
            if self.consume(TokenKind::Char('(')) {
                loop {
                    let column = self.consume_col_id().ok_or_else(|| {
                        self.error_here("expected a column name in the CTE alias list")
                    })?;
                    aliascolnames.push(make_string_node(column));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                }
                self.expect(TokenKind::Char(')'))?;
            }
            self.expect(TokenKind::As)?;
            let ctematerialized = if self.consume(TokenKind::Materialized) {
                CteMaterialize::Always
            } else if self.consume(TokenKind::Not) {
                self.expect(TokenKind::Materialized)?;
                CteMaterialize::Never
            } else {
                CteMaterialize::Default
            };
            self.expect(TokenKind::Char('('))?;
            let inner = self.take_until_top_level(&[TokenKind::Char(')')]);
            self.record_completion_tokens(&[TokenKind::Char(')')]);
            let ctequery = self.parse_preparable_fragment_tokens(inner)?;
            self.expect(TokenKind::Char(')'))?;
            let search_clause = self.parse_cte_search_clause()?;
            let cycle_clause = self.parse_cte_cycle_clause()?;
            ctes.push(node!(CommonTableExpr {
                ctename: Some(name),
                aliascolnames,
                ctematerialized,
                ctequery: Some(Box::new(ctequery)),
                search_clause,
                cycle_clause,
                location: cte_location as ParseLoc,
                ..CommonTableExpr::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }

        Ok(WithClause {
            ctes,
            recursive,
            location: location as ParseLoc,
        })
    }

    fn parse_cte_search_clause(&mut self) -> PResult<Option<Box<CteSearchClause>>> {
        if !self.consume(TokenKind::Search) {
            return Ok(None);
        }
        let location = self.previous_location();
        let search_breadth_first = if self.consume(TokenKind::Depth) {
            false
        } else {
            self.expect(TokenKind::Breadth)?;
            true
        };
        self.expect(TokenKind::FirstP)?;
        self.expect(TokenKind::By)?;
        let search_col_list =
            self.parse_simple_name_list_until(&[TokenKind::Set], GrammarSlot::Column)?;
        self.expect(TokenKind::Set)?;
        let search_seq_column = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("SEARCH SET requires a column name"))?,
        );
        Ok(Some(Box::new(CteSearchClause {
            search_col_list,
            search_breadth_first,
            search_seq_column,
            location: location as ParseLoc,
        })))
    }

    fn parse_cte_cycle_clause(&mut self) -> PResult<Option<Box<CteCycleClause>>> {
        if !self.consume(TokenKind::Cycle) {
            return Ok(None);
        }
        let location = self.previous_location();
        let cycle_col_list =
            self.parse_simple_name_list_until(&[TokenKind::Set], GrammarSlot::Column)?;
        self.expect(TokenKind::Set)?;
        let cycle_mark_column = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CYCLE SET requires a mark column name"))?,
        );
        let (cycle_mark_value, cycle_mark_default) = if self.consume(TokenKind::To) {
            let value_tokens = self.take_until_top_level(&[TokenKind::Default]);
            if self.at_completion() && value_tokens.is_empty() {
                self.record_completion_slot(GrammarSlot::Type);
            }
            self.expect(TokenKind::Default)?;
            let default_tokens = self.take_until_top_level(&[TokenKind::Using]);
            if self.at_completion() && default_tokens.is_empty() {
                self.record_completion_slot(GrammarSlot::Type);
            }
            (
                Some(Box::new(parse_aexpr_const_tokens(value_tokens)?)),
                Some(Box::new(parse_aexpr_const_tokens(default_tokens)?)),
            )
        } else {
            (
                Some(Box::new(node!(AConst {
                    val: ValUnion::Boolean(Boolean::new(true)),
                    location: -1,
                    ..AConst::default()
                }))),
                Some(Box::new(node!(AConst {
                    val: ValUnion::Boolean(Boolean::new(false)),
                    location: -1,
                    ..AConst::default()
                }))),
            )
        };
        self.expect(TokenKind::Using)?;
        let cycle_path_column = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CYCLE USING requires a path column name"))?,
        );
        Ok(Some(Box::new(CteCycleClause {
            cycle_col_list,
            cycle_mark_column,
            cycle_mark_value,
            cycle_mark_default,
            cycle_path_column,
            location: location as ParseLoc,
            ..CteCycleClause::default()
        })))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-select.html
    // [ WITH [ RECURSIVE ] with_query [, ...] ]
    // SELECT [ ALL | DISTINCT [ ON ( expression [, ...] ) ] ]
    //     [ { * | expression [ [ AS ] output_name ] } [, ...] ]
    //     [ FROM from_item [, ...] ]
    //     [ WHERE condition ]
    //     [ GROUP BY [ ALL | DISTINCT ] grouping_element [, ...] ]
    //     [ HAVING condition ]
    //     [ WINDOW window_name AS ( window_definition ) [, ...] ]
    //     [ { UNION | INTERSECT | EXCEPT } [ ALL | DISTINCT ] select ]
    //     [ ORDER BY expression [ ASC | DESC | USING operator ] [ NULLS { FIRST | LAST } ] [, ...]
    // ]     [ LIMIT { count | ALL } ]
    //     [ OFFSET start [ ROW | ROWS ] ]
    //     [ FETCH { FIRST | NEXT } [ count ] { ROW | ROWS } { ONLY | WITH TIES } ]
    //     [ FOR { UPDATE | NO KEY UPDATE | SHARE | KEY SHARE } [ OF from_reference [, ...] ] [
    // NOWAIT | SKIP LOCKED ] [...] ]
    //
    // where from_item can be one of:
    //
    //     [ ONLY ] table_name [ * ] [ [ AS ] alias [ ( column_alias [, ...] ) ] ]
    //                 [ TABLESAMPLE sampling_method ( argument [, ...] ) [ REPEATABLE ( seed ) ] ]
    //     [ LATERAL ] ( select ) [ [ AS ] alias [ ( column_alias [, ...] ) ] ]
    //     with_query_name [ [ AS ] alias [ ( column_alias [, ...] ) ] ]
    //     [ LATERAL ] function_name ( [ argument [, ...] ] )
    //                 [ WITH ORDINALITY ] [ [ AS ] alias [ ( column_alias [, ...] ) ] ]
    //     [ LATERAL ] function_name ( [ argument [, ...] ] ) [ AS ] alias ( column_definition [,
    // ...] )     [ LATERAL ] function_name ( [ argument [, ...] ] ) AS ( column_definition [,
    // ...] )     [ LATERAL ] ROWS FROM( function_name ( [ argument [, ...] ] ) [ AS (
    // column_definition [, ...] ) ] [, ...] )                 [ WITH ORDINALITY ] [ [ AS ]
    // alias [ ( column_alias [, ...] ) ] ]     from_item join_type from_item { ON
    // join_condition | USING ( join_column [, ...] ) [ AS join_using_alias ] }     from_item
    // NATURAL join_type from_item     from_item CROSS JOIN from_item
    //
    // and grouping_element can be one of:
    //
    //     ( )
    //     expression
    //     ( expression [, ...] )
    //     ROLLUP ( { expression | ( expression [, ...] ) } [, ...] )
    //     CUBE ( { expression | ( expression [, ...] ) } [, ...] )
    //     GROUPING SETS ( grouping_element [, ...] )
    //
    // and with_query is:
    //
    //     with_query_name [ ( column_name [, ...] ) ] AS [ [ NOT ] MATERIALIZED ] ( select | values
    // | insert | update | delete | merge )         [ SEARCH { BREADTH | DEPTH } FIRST BY
    // column_name [, ...] SET search_seq_col_name ]         [ CYCLE column_name [, ...] SET
    // cycle_mark_col_name [ TO cycle_mark_value DEFAULT cycle_mark_default ] USING
    // cycle_path_col_name ]
    //
    // TABLE [ ONLY ] table_name [ * ]
    pub(super) fn parse_select(&mut self, with_clause: Option<WithClause>) -> PResult<SelectStmt> {
        let mut stmt = self.parse_select_set_expr(with_clause, 0)?;
        self.parse_select_tail(&mut stmt)?;
        Ok(stmt)
    }

    fn parse_select_set_expr(
        &mut self,
        with_clause: Option<WithClause>,
        min_precedence: u8,
    ) -> PResult<SelectStmt> {
        let mut lhs = self.parse_select_primary(with_clause)?;

        self.record_completion_tokens(&[TokenKind::Union, TokenKind::Except, TokenKind::Intersect]);
        while let Some((op, precedence)) = select_set_operator(self.peek_kind()) {
            if precedence < min_precedence {
                break;
            }
            self.advance();
            let all = if self.consume(TokenKind::All) {
                true
            } else {
                self.consume(TokenKind::Distinct);
                false
            };
            let rhs = self.parse_select_set_expr(None, precedence + 1)?;
            let outer_with = lhs.with_clause.take();
            lhs = SelectStmt {
                op,
                all,
                larg: Some(Box::new(lhs)),
                rarg: Some(Box::new(rhs)),
                with_clause: outer_with,
                ..SelectStmt::default()
            };
        }

        Ok(lhs)
    }

    fn parse_select_primary(&mut self, with_clause: Option<WithClause>) -> PResult<SelectStmt> {
        self.record_completion_tokens(&[
            TokenKind::Values,
            TokenKind::Table,
            TokenKind::Select,
            TokenKind::Char('('),
        ]);
        if self.consume(TokenKind::Char('(')) {
            let mut inner = self.take_until_top_level(&[TokenKind::Char(')')]);
            if self.at_completion() {
                if inner.is_empty() {
                    self.record_completion_tokens(&[
                        TokenKind::With,
                        TokenKind::Select,
                        TokenKind::Values,
                        TokenKind::Table,
                        TokenKind::Char('('),
                    ]);
                    return Err(self.error_here("completion point in parenthesized query"));
                }
                self.append_completion_marker(&mut inner);
                return match parse_select_statement_tokens_with_completion(
                    inner,
                    self.completion.clone(),
                )? {
                    Node::SelectStmt(mut stmt) => {
                        if with_clause.is_some() && stmt.with_clause.is_some() {
                            return Err(self.error_here("multiple WITH clauses are not allowed"));
                        }
                        if let Some(with_clause) = with_clause {
                            stmt.with_clause = Some(Box::new(with_clause));
                        }
                        Ok(stmt)
                    }
                    _ => unreachable!("parse_select_statement_tokens returned a non-select node"),
                };
            }
            self.expect(TokenKind::Char(')'))?;
            return match parse_select_statement_tokens(inner)? {
                Node::SelectStmt(mut stmt) => {
                    if with_clause.is_some() && stmt.with_clause.is_some() {
                        return Err(self.error_here("multiple WITH clauses are not allowed"));
                    }
                    if let Some(with_clause) = with_clause {
                        stmt.with_clause = Some(Box::new(with_clause));
                    }
                    Ok(stmt)
                }
                _ => unreachable!("parse_select_statement_tokens returned a non-select node"),
            };
        }
        let mut stmt = SelectStmt {
            with_clause: with_clause.map(Box::new),
            ..SelectStmt::default()
        };
        let mut distinct_requires_target = false;

        match self.peek_kind() {
            TokenKind::Values => {
                self.advance();
                stmt.values_lists = self.parse_values_lists()?;
                if stmt.values_lists.is_empty() {
                    return Err(self.error_here("VALUES requires at least one row"));
                }
            }
            TokenKind::Table => {
                self.advance();
                let range = self.parse_relation_expr()?;
                stmt.target_list.push(node!(ResTarget {
                    val: Some(Box::new(node!(ColumnRef {
                        fields: vec![Node::AStar],
                        location: -1,
                    }))),
                    location: -1,
                    ..ResTarget::default()
                }));
                stmt.from_clause.push(Node::RangeVar(range));
            }
            _ => {
                self.expect(TokenKind::Select)?;
                self.record_completion_tokens(&[
                    TokenKind::All,
                    TokenKind::Distinct,
                    TokenKind::Into,
                    TokenKind::From,
                    TokenKind::Where,
                    TokenKind::GroupP,
                    TokenKind::Having,
                    TokenKind::Window,
                    TokenKind::Order,
                    TokenKind::Limit,
                    TokenKind::Offset,
                    TokenKind::Fetch,
                    TokenKind::For,
                    TokenKind::Union,
                    TokenKind::Intersect,
                    TokenKind::Except,
                    TokenKind::Char(';'),
                ]);
                if self.consume(TokenKind::All) {
                    stmt.distinct_clause.clear();
                } else if self.consume(TokenKind::Distinct) {
                    distinct_requires_target = true;
                    stmt.distinct_clause.push(node!(String::new("distinct")));
                    if self.consume(TokenKind::On) {
                        self.expect(TokenKind::Char('('))?;
                        let expressions =
                            self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?;
                        if expressions.is_empty() {
                            return Err(self.error_here("DISTINCT ON requires an expression"));
                        }
                        stmt.distinct_clause = expressions;
                        self.expect(TokenKind::Char(')'))?;
                    }
                }
                stmt.target_list = self.parse_res_target_list_strict_until(&[
                    TokenKind::Into,
                    TokenKind::From,
                    TokenKind::Where,
                    TokenKind::GroupP,
                    TokenKind::Having,
                    TokenKind::Window,
                    TokenKind::Order,
                    TokenKind::Limit,
                    TokenKind::Offset,
                    TokenKind::Fetch,
                    TokenKind::For,
                    TokenKind::Union,
                    TokenKind::Intersect,
                    TokenKind::Except,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])?;
                if distinct_requires_target && stmt.target_list.is_empty() {
                    return Err(self.error_here("SELECT DISTINCT requires a target list"));
                }
            }
        }

        if self.consume(TokenKind::Into) {
            stmt.into_clause = Some(Box::new(self.parse_select_into_clause()?));
        }

        if self.consume(TokenKind::From) {
            stmt.from_clause = self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::GroupP,
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
        }
        stmt.where_clause = self.parse_optional_expr_clause(
            TokenKind::Where,
            &[
                TokenKind::GroupP,
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
        )?;
        if self.consume_phrase(&[TokenKind::GroupP, TokenKind::By])? {
            if self.consume(TokenKind::All) {
                stmt.group_by_all = true;
            } else if self.consume(TokenKind::Distinct) {
                stmt.group_distinct = true;
            }
            stmt.group_clause = self.parse_group_by_list_until(&[
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
            if stmt.group_clause.is_empty() {
                return Err(self.error_here("GROUP BY requires at least one expression"));
            }
        }
        stmt.having_clause = self.parse_optional_expr_clause(
            TokenKind::Having,
            &[
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
        )?;
        if self.consume(TokenKind::Window) {
            stmt.window_clause = self.parse_window_clause_until(&[
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
        }

        Ok(stmt)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-selectinto.html
    // [ WITH [ RECURSIVE ] with_query [, ...] ]
    // SELECT [ ALL | DISTINCT [ ON ( expression [, ...] ) ] ]
    //     [ { * | expression [ [ AS ] output_name ] } [, ...] ]
    //     INTO [ TEMPORARY | TEMP | UNLOGGED ] [ TABLE ] new_table
    //     [ FROM from_item [, ...] ]
    //     [ WHERE condition ]
    //     [ GROUP BY expression [, ...] ]
    //     [ HAVING condition ]
    //     [ WINDOW window_name AS ( window_definition ) [, ...] ]
    //     [ { UNION | INTERSECT | EXCEPT } [ ALL | DISTINCT ] select ]
    //     [ ORDER BY expression [ ASC | DESC | USING operator ] [ NULLS { FIRST | LAST } ] [, ...]
    // ]     [ LIMIT { count | ALL } ]
    //     [ OFFSET start [ ROW | ROWS ] ]
    //     [ FETCH { FIRST | NEXT } [ count ] { ROW | ROWS } ONLY ]
    //     [ FOR { UPDATE | SHARE } [ OF table_name [, ...] ] [ NOWAIT ] [...] ]
    fn parse_select_into_clause(&mut self) -> PResult<IntoClause> {
        let relpersistence = match self.peek_kind() {
            TokenKind::Temporary | TokenKind::Temp => {
                self.advance();
                b't'
            }
            scope @ (TokenKind::Local | TokenKind::Global) => {
                self.advance();
                if !(self.consume(TokenKind::Temporary) || self.consume(TokenKind::Temp)) {
                    let scope = match scope {
                        TokenKind::Local => "LOCAL",
                        TokenKind::Global => "GLOBAL",
                        _ => unreachable!(),
                    };
                    return Err(
                        self.error_here(format!("{scope} must be followed by TEMP or TEMPORARY"))
                    );
                }
                b't'
            }
            TokenKind::Unlogged => {
                self.advance();
                b'u'
            }
            _ => b'p',
        };
        self.consume(TokenKind::Table);
        let mut relation = self
            .try_parse_qualified_range_var_with_slot(GrammarSlot::Table)
            .ok_or_else(|| self.error_here("SELECT INTO requires a relation name"))?;
        relation.relpersistence = relpersistence;
        Ok(IntoClause {
            rel: Some(Box::new(relation)),
            ..IntoClause::default()
        })
    }

    fn parse_select_tail(&mut self, stmt: &mut SelectStmt) -> PResult<()> {
        self.record_completion_tokens(&[
            TokenKind::Order,
            TokenKind::Limit,
            TokenKind::Offset,
            TokenKind::Fetch,
            TokenKind::For,
        ]);
        if self.consume_phrase(&[TokenKind::Order, TokenKind::By])? {
            stmt.sort_clause = self.parse_sort_list_strict_until(&[
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?;
        }
        let locking_stops = [
            TokenKind::Limit,
            TokenKind::Offset,
            TokenKind::Fetch,
            TokenKind::Union,
            TokenKind::Intersect,
            TokenKind::Except,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        if self.at(TokenKind::For) {
            stmt.locking_clause = self.parse_locking_clause_until(&locking_stops)?;
            self.parse_select_limit_clauses(stmt)?;
        } else {
            self.parse_select_limit_clauses(stmt)?;
            if self.at(TokenKind::For) {
                stmt.locking_clause = self.parse_locking_clause_until(&locking_stops)?;
            }
        }
        if stmt.limit_option == LimitOption::WithTies {
            if stmt.sort_clause.is_empty() {
                return Err(self.error_here("WITH TIES requires an ORDER BY clause"));
            }
            if stmt.locking_clause.iter().any(|clause| {
                matches!(
                    clause,
                    node!(LockingClause {
                        wait_policy: LockWaitPolicy::Skip,
                        ..
                    })
                )
            }) {
                return Err(self.error_here("SKIP LOCKED and WITH TIES cannot be used together"));
            }
        }

        Ok(())
    }

    fn parse_select_limit_clauses(&mut self, stmt: &mut SelectStmt) -> PResult<()> {
        let mut saw_limit = false;
        let mut saw_offset = false;
        while self.at_any(&[TokenKind::Limit, TokenKind::Offset, TokenKind::Fetch]) {
            if self.consume(TokenKind::Limit) {
                if saw_limit {
                    return Err(self.error_here("multiple LIMIT or FETCH clauses are not allowed"));
                }
                saw_limit = true;
                stmt.limit_count = Some(if self.consume(TokenKind::All) {
                    Box::new(node!(AConst::null(self.previous_location() as ParseLoc)))
                } else {
                    self.parse_expr_box_strict_until(&[
                        TokenKind::Char(','),
                        TokenKind::Offset,
                        TokenKind::Fetch,
                        TokenKind::For,
                        TokenKind::Union,
                        TokenKind::Intersect,
                        TokenKind::Except,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])?
                });
                if self.consume(TokenKind::Char(',')) {
                    return Err(
                        self.error_here("LIMIT count, offset syntax is not supported; use OFFSET")
                    );
                }
            } else if self.consume(TokenKind::Offset) {
                if saw_offset {
                    return Err(self.error_here("multiple OFFSET clauses are not allowed"));
                }
                saw_offset = true;
                let offset_stops = [
                    TokenKind::Row,
                    TokenKind::Rows,
                    TokenKind::Limit,
                    TokenKind::Fetch,
                    TokenKind::For,
                    TokenKind::Union,
                    TokenKind::Intersect,
                    TokenKind::Except,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ];
                let offset_tokens = self.take_until_top_level(&offset_stops);
                self.record_expression_follow_tokens(&offset_tokens, &offset_stops, false);
                let has_row_suffix = matches!(self.peek_kind(), TokenKind::Row | TokenKind::Rows);
                stmt.limit_offset = Some(Box::new(if has_row_suffix {
                    parse_select_fetch_first_value_tokens(offset_tokens)?
                } else {
                    self.parse_expression_fragment_tokens(offset_tokens)?
                }));
                if has_row_suffix {
                    self.advance();
                }
            } else {
                self.expect(TokenKind::Fetch)?;
                if saw_limit {
                    return Err(self.error_here("multiple LIMIT or FETCH clauses are not allowed"));
                }
                saw_limit = true;
                if !(self.consume(TokenKind::FirstP) || self.consume(TokenKind::Next)) {
                    return Err(self.error_here("FETCH requires FIRST or NEXT"));
                }
                self.record_completion_tokens(&[TokenKind::Row, TokenKind::Rows]);
                stmt.limit_count = Some(
                    if matches!(self.peek_kind(), TokenKind::Row | TokenKind::Rows) {
                        Box::new(node!(AConst::integer(1, -1)))
                    } else {
                        let mut tokens =
                            self.take_until_top_level(&[TokenKind::Row, TokenKind::Rows]);
                        if self.at_completion()
                            && parse_select_fetch_first_value_tokens(tokens.clone()).is_ok()
                        {
                            self.record_completion_follow_tokens(&[
                                TokenKind::Row,
                                TokenKind::Rows,
                            ]);
                        }
                        self.append_completion_marker(&mut tokens);
                        Box::new(parse_select_fetch_first_value_tokens_with_completion(
                            tokens,
                            self.completion.clone(),
                        )?)
                    },
                );
                if !(self.consume(TokenKind::Row) || self.consume(TokenKind::Rows)) {
                    return Err(self.error_here("FETCH requires ROW or ROWS"));
                }
                if self.consume(TokenKind::With) {
                    self.expect(TokenKind::Ties)?;
                    stmt.limit_option = LimitOption::WithTies;
                } else {
                    self.expect(TokenKind::Only)?;
                }
            }
        }
        Ok(())
    }
}
pub(super) fn select_set_operator(kind: TokenKind) -> Option<(SetOperation, u8)> {
    match kind {
        TokenKind::Union => Some((SetOperation::Union, 1)),
        TokenKind::Except => Some((SetOperation::Except, 1)),
        TokenKind::Intersect => Some((SetOperation::Intersect, 2)),
        _ => None,
    }
}
