use super::*;

impl Parser {
    pub(super) fn parse_from_clause_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        self.record_completion_slot(completion::GrammarSlot::Relation);
        self.record_completion_slot(completion::GrammarSlot::Function);
        self.record_completion_tokens(&[
            TokenKind::LateralP,
            TokenKind::Only,
            TokenKind::Rows,
            TokenKind::Xmltable,
            TokenKind::JsonTable,
            TokenKind::GraphTable,
            TokenKind::Char('('),
        ]);
        let mut items = Vec::new();
        while self.at_completion() || !self.at_any(stops) {
            items.push(self.parse_from_item(stops)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected a FROM item after ','"));
            }
        }
        if items.is_empty() {
            return Err(self.error_here("FROM requires at least one table reference"));
        }
        Ok(items)
    }

    pub(super) fn parse_from_item(&mut self, stops: &[TokenKind]) -> PResult<Node> {
        self.record_completion_slot(completion::GrammarSlot::Relation);
        self.record_completion_slot(completion::GrammarSlot::Function);
        self.record_completion_tokens(&[
            TokenKind::LateralP,
            TokenKind::Only,
            TokenKind::Rows,
            TokenKind::Xmltable,
            TokenKind::JsonTable,
            TokenKind::GraphTable,
            TokenKind::Char('('),
        ]);
        let lateral = self.consume(TokenKind::LateralP);
        if lateral {
            self.record_completion_slot(completion::GrammarSlot::Function);
            self.record_completion_tokens(&[
                TokenKind::Char('('),
                TokenKind::Rows,
                TokenKind::Xmltable,
                TokenKind::JsonTable,
            ]);
        }
        let mut base = if self.at(TokenKind::GraphTable) {
            if lateral {
                return Err(self.error_here("LATERAL is not allowed before GRAPH_TABLE"));
            }
            Node::RangeGraphTable(self.parse_graph_table()?)
        } else if self.at(TokenKind::Xmltable) {
            Node::RangeTableFunc(self.parse_xmltable(lateral)?)
        } else if self.at(TokenKind::JsonTable) {
            Node::JsonTable(self.parse_json_table(lateral)?)
        } else if self.at(TokenKind::Rows) && self.peek_kind_n(1) == TokenKind::From {
            Node::RangeFunction(self.parse_rows_from(lateral)?)
        } else if self.at(TokenKind::Only) {
            if lateral {
                return Err(self.error_here("LATERAL requires a function or subquery"));
            }
            Node::RangeVar(self.parse_relation_expr_with_alias()?)
        } else if self.at(TokenKind::Char('(')) {
            self.parse_parenthesized_from_item(lateral)?
        } else {
            let name_start = self.pos;
            let mut name_stops = vec![
                TokenKind::Char('('),
                TokenKind::As,
                TokenKind::Char(','),
                TokenKind::Join,
                TokenKind::InnerP,
                TokenKind::Left,
                TokenKind::Right,
                TokenKind::Full,
                TokenKind::Cross,
                TokenKind::Natural,
                TokenKind::On,
                TokenKind::Using,
                TokenKind::Tablesample,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ];
            for stop in stops {
                if !name_stops.contains(stop) {
                    name_stops.push(*stop);
                }
            }
            let name_tokens = self.take_until_top_level(&name_stops);
            let can_be_function_name =
                !name_tokens.is_empty() && parse_qualified_type_names(&name_tokens).is_ok();
            if can_be_function_name {
                self.record_completion_tokens(&[TokenKind::Char('(')]);
            }
            let looks_like_function_name = self.at(TokenKind::Char('(')) && can_be_function_name;
            self.pos = name_start;
            if looks_like_function_name {
                let function = self.parse_function_expression()?;
                let ordinality = if self.consume(TokenKind::With) {
                    self.expect(TokenKind::Ordinality)?;
                    true
                } else {
                    false
                };
                let (alias, coldeflist) = self.parse_function_alias_clause()?;
                Node::RangeFunction(RangeFunction {
                    node_tag: NodeTag::RangeFunction,
                    lateral,
                    ordinality,
                    functions: vec![name_list_node(vec![function, name_list_node(Vec::new())])],
                    alias,
                    coldeflist,
                    ..RangeFunction::default()
                })
            } else {
                if lateral {
                    return Err(self.error_here("LATERAL requires a function or subquery"));
                }
                Node::RangeVar(self.parse_relation_expr_with_alias()?)
            }
        };
        if self.consume(TokenKind::Tablesample) {
            if !matches!(base, Node::RangeVar(_)) {
                return Err(self.error_here("TABLESAMPLE requires a relation"));
            }
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::Function);
            let method = self.parse_name_list();
            if method.is_empty() {
                return Err(self.error_here("TABLESAMPLE requires a sampling method"));
            }
            self.expect(TokenKind::Char('('))?;
            let args = self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?;
            if args.is_empty() {
                return Err(self.error_here("TABLESAMPLE requires at least one argument"));
            }
            self.expect(TokenKind::Char(')'))?;
            let repeatable = if self.consume(TokenKind::Repeatable) {
                self.expect(TokenKind::Char('('))?;
                let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
                self.expect(TokenKind::Char(')'))?;
                Some(expr)
            } else {
                None
            };
            base = Node::RangeTableSample(RangeTableSample {
                node_tag: NodeTag::RangeTableSample,
                relation: Some(Box::new(base)),
                method,
                args,
                repeatable,
                location: location as ParseLoc,
            });
        }
        loop {
            self.record_completion_tokens(&[
                TokenKind::Join,
                TokenKind::InnerP,
                TokenKind::Left,
                TokenKind::Right,
                TokenKind::Full,
                TokenKind::Cross,
                TokenKind::Natural,
            ]);
            if self.at_any(&extend_stops(stops, TokenKind::Char(','))) {
                break;
            }
            if matches!(
                self.peek_kind(),
                TokenKind::Join
                    | TokenKind::InnerP
                    | TokenKind::Left
                    | TokenKind::Right
                    | TokenKind::Full
                    | TokenKind::Cross
                    | TokenKind::Natural
            ) {
                base = self.parse_join_tail(base, stops)?;
            } else {
                break;
            }
        }
        Ok(base)
    }

    fn parse_parenthesized_from_item(&mut self, lateral: bool) -> PResult<Node> {
        self.expect(TokenKind::Char('('))?;
        let mut inner_tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
        self.record_completion_tokens(&[TokenKind::Char(')')]);

        if self.at_completion() && inner_tokens.is_empty() {
            self.record_completion_tokens(&[
                TokenKind::With,
                TokenKind::Select,
                TokenKind::Values,
                TokenKind::Table,
                TokenKind::Char('('),
                TokenKind::LateralP,
                TokenKind::Only,
                TokenKind::Rows,
                TokenKind::Xmltable,
                TokenKind::JsonTable,
                TokenKind::GraphTable,
            ]);
            self.record_completion_slot(completion::GrammarSlot::Relation);
            self.record_completion_slot(completion::GrammarSlot::Function);
            return Err(self.error_here("completion point in parenthesized FROM item"));
        }

        let starts_subquery = inner_tokens
            .first()
            .is_some_and(|token| completion::SUBQUERY_START_TOKENS.contains(&token.kind));
        if self.at_completion() {
            self.append_completion_marker(&mut inner_tokens);
            if starts_subquery {
                let _ = parse_select_statement_tokens_with_completion(
                    inner_tokens,
                    self.completion.clone(),
                )?;
            } else {
                let end_location = inner_tokens
                    .last()
                    .map_or(self.location(), Token::end_location);
                inner_tokens.push(Token::synthetic(TokenKind::Eof, end_location));
                let mut nested = Parser {
                    tokens: inner_tokens,
                    pos: 0,
                    completion: self.completion.clone(),
                };
                let _ = nested.parse_from_item(&[TokenKind::Eof])?;
            }
            return Err(self.error_here("completion point in parenthesized FROM item"));
        }

        self.expect(TokenKind::Char(')'))?;
        if starts_subquery {
            let subquery = parse_select_statement_tokens(inner_tokens)?;
            return Ok(Node::RangeSubselect(RangeSubselect {
                node_tag: NodeTag::RangeSubselect,
                lateral,
                subquery: Some(Box::new(subquery)),
                alias: self.parse_optional_alias_clause()?,
            }));
        }

        let item_location = inner_tokens
            .first()
            .map_or(self.location(), |token| token.location());
        let end_location = inner_tokens
            .last()
            .map_or(self.location(), Token::end_location);
        inner_tokens.push(Token::synthetic(TokenKind::Eof, end_location));
        let mut nested = Parser {
            tokens: inner_tokens,
            pos: 0,
            completion: None,
        };
        let mut item = nested.parse_from_item(&[TokenKind::Eof])?;
        if !nested.at(TokenKind::Eof) {
            return Err(ParseError::syntax_exit(
                nested.location(),
                "unexpected token in parenthesized FROM item",
            ));
        }
        if !matches!(item, Node::JoinExpr(_) | Node::RangeSubselect(_)) {
            return Err(ParseError::syntax_exit(
                item_location,
                "parenthesized FROM item must be a joined table or subquery",
            ));
        }
        if lateral {
            match &mut item {
                Node::RangeSubselect(range) => range.lateral = true,
                Node::JoinExpr(_) => {
                    return Err(self.error_here("LATERAL requires a function or subquery"));
                }
                _ => unreachable!("parenthesized FROM item shape was checked above"),
            }
        }
        if let Some(alias) = self.parse_optional_alias_clause()? {
            match &mut item {
                Node::JoinExpr(join) => join.alias = Some(alias),
                Node::RangeSubselect(range) => range.alias = Some(alias),
                _ => unreachable!("parenthesized FROM item shape was checked above"),
            }
        }
        Ok(item)
    }

    pub(super) fn parse_rows_from(&mut self, lateral: bool) -> PResult<RangeFunction> {
        self.expect(TokenKind::Rows)?;
        self.expect(TokenKind::From)?;
        self.expect(TokenKind::Char('('))?;
        let mut functions = Vec::new();
        loop {
            let expression_tokens = self.take_until_top_level(&[
                TokenKind::As,
                TokenKind::Char(','),
                TokenKind::Char(')'),
            ]);
            self.record_expression_follow_tokens(
                &expression_tokens,
                &[TokenKind::As, TokenKind::Char(','), TokenKind::Char(')')],
                false,
            );
            let expression = self.parse_expression_fragment_tokens(expression_tokens)?;
            if !is_function_expression_node(&expression) {
                return Err(self.error_here("ROWS FROM items must be function expressions"));
            }
            let coldeflist = if self.consume(TokenKind::As) {
                self.expect(TokenKind::Char('('))?;
                let definitions = self.parse_table_func_element_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                definitions
            } else {
                Vec::new()
            };
            functions.push(name_list_node(vec![expression, name_list_node(coldeflist)]));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a ROWS FROM item after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        let ordinality = if self.consume(TokenKind::With) {
            self.expect(TokenKind::Ordinality)?;
            true
        } else {
            false
        };
        let (alias, coldeflist) = self.parse_function_alias_clause()?;
        Ok(RangeFunction {
            node_tag: NodeTag::RangeFunction,
            lateral,
            ordinality,
            is_rowsfrom: true,
            functions,
            alias,
            coldeflist,
        })
    }

    pub(super) fn parse_function_alias_clause(
        &mut self,
    ) -> PResult<(Option<Box<Alias>>, NodeList)> {
        let has_as = self.consume(TokenKind::As);
        if has_as && self.consume(TokenKind::Char('(')) {
            let coldeflist = self.parse_table_func_element_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            return Ok((None, coldeflist));
        }
        self.record_completion_slot(completion::GrammarSlot::Alias);
        let aliasname = if has_as {
            self.consume_col_id()
                .ok_or_else(|| self.error_here("expected a function alias"))?
        } else {
            let Some(aliasname) = self.consume_col_id() else {
                return Ok((None, Vec::new()));
            };
            aliasname
        };
        let mut alias = Box::new(Alias {
            node_tag: NodeTag::Alias,
            aliasname: Some(aliasname),
            ..Alias::default()
        });
        self.record_completion_tokens(&[TokenKind::Char('(')]);
        if !self.at(TokenKind::Char('(')) {
            return Ok((Some(alias), Vec::new()));
        }

        let alias_suffix_start = self.pos;
        self.advance();
        let inner = self.take_until_top_level(&[TokenKind::Char(')')]);
        let chunks = split_top_level_commas(inner);
        if self.at_completion()
            && chunks.last().is_some_and(|chunk| {
                chunk.len() == 1
                    && chunk.first().is_some_and(|token| {
                        token_name_in_categories(
                            token,
                            &[KeywordCategory::Unreserved, KeywordCategory::ColName],
                        )
                        .is_some()
                    })
            })
        {
            // `alias(column |)` is still ambiguous: the active name may end
            // an alias column list, or it may begin `column type` in a table
            // function definition. Preserve both productions until a comma,
            // closing parenthesis, or type token resolves the branch.
            self.record_completion_slot(completion::GrammarSlot::Type);
        }
        self.pos = alias_suffix_start;
        let is_alias_column_list = !chunks.is_empty()
            && chunks.iter().all(|chunk| {
                chunk.len() == 1
                    && chunk.first().is_some_and(|token| {
                        token_name_in_categories(
                            token,
                            &[KeywordCategory::Unreserved, KeywordCategory::ColName],
                        )
                        .is_some()
                    })
            });
        self.expect(TokenKind::Char('('))?;
        if is_alias_column_list {
            alias.colnames = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            Ok((Some(alias), Vec::new()))
        } else {
            let coldeflist = self.parse_table_func_element_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            Ok((Some(alias), coldeflist))
        }
    }

    pub(super) fn parse_table_func_element_list_body(&mut self) -> PResult<NodeList> {
        let mut definitions = Vec::new();
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("column definition list cannot be empty"));
        }
        loop {
            definitions.push(
                *self.parse_table_func_element_until(&[
                    TokenKind::Char(','),
                    TokenKind::Char(')'),
                ])?,
            );
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a column definition after ','"));
            }
        }
        Ok(definitions)
    }
}
