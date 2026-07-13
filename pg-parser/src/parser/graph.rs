use super::*;

impl Parser {
    pub(super) fn parse_graph_table(&mut self) -> PResult<RangeGraphTable> {
        let location = self.expect(TokenKind::GraphTable)?.location();
        self.expect(TokenKind::Char('('))?;
        let graph_name = Some(Box::new(
            self.try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("GRAPH_TABLE requires a graph name"))?,
        ));
        self.expect(TokenKind::Match)?;
        let mut paths = Vec::new();
        loop {
            let mut elements = Vec::new();
            while !matches!(
                self.peek_kind(),
                TokenKind::Char(',') | TokenKind::Where | TokenKind::Columns | TokenKind::Eof
            ) {
                elements.push(Node::GraphElementPattern(
                    self.parse_graph_element_pattern()?,
                ));
            }
            if elements.is_empty() {
                return Err(self.error_here("expected a graph path pattern"));
            }
            paths.push(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements,
                ..AArrayExpr::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        let where_clause = if self.consume(TokenKind::Where) {
            Some(self.parse_expr_box_strict_until(&[TokenKind::Columns])?)
        } else {
            None
        };
        self.expect(TokenKind::Columns)?;
        self.expect(TokenKind::Char('('))?;
        let columns = self.parse_res_target_list_strict_until(&[TokenKind::Char(')')])?;
        self.expect(TokenKind::Char(')'))?;
        self.expect(TokenKind::Char(')'))?;
        let alias = self.parse_optional_alias_clause()?;
        Ok(RangeGraphTable {
            node_tag: NodeTag::RangeGraphTable,
            graph_name,
            graph_pattern: Some(Box::new(GraphPattern {
                node_tag: NodeTag::GraphPattern,
                path_pattern_list: paths,
                where_clause,
            })),
            columns,
            alias,
            location: location as ParseLoc,
        })
    }

    pub(super) fn parse_graph_element_pattern(&mut self) -> PResult<GraphElementPattern> {
        let location = self.location();
        if self.at(TokenKind::Char('('))
            && matches!(
                self.peek_kind_n(1),
                TokenKind::Char('(')
                    | TokenKind::Char('<')
                    | TokenKind::Char('-')
                    | TokenKind::RightArrow
            )
        {
            self.advance();
            let mut subexpr = Vec::new();
            while !self.at_any(&[TokenKind::Where, TokenKind::Char(')'), TokenKind::Eof]) {
                subexpr.push(Node::GraphElementPattern(
                    self.parse_graph_element_pattern()?,
                ));
            }
            if subexpr.is_empty() {
                return Err(self.error_here("parenthesized graph path cannot be empty"));
            }
            let where_clause = if self.consume(TokenKind::Where) {
                Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?)
            } else {
                None
            };
            self.expect(TokenKind::Char(')'))?;
            let quantifier = self.parse_graph_pattern_quantifier()?;
            return Ok(GraphElementPattern {
                node_tag: NodeTag::GraphElementPattern,
                kind: GraphElementPatternKind::ParenExpr,
                subexpr,
                where_clause,
                quantifier,
                location: location as ParseLoc,
                ..GraphElementPattern::default()
            });
        }
        let (kind, close) = if self.consume(TokenKind::Char('(')) {
            (
                GraphElementPatternKind::VertexPattern,
                Some(TokenKind::Char(')')),
            )
        } else if self.consume(TokenKind::Char('<')) {
            self.expect(TokenKind::Char('-'))?;
            if self.consume(TokenKind::Char('[')) {
                (
                    GraphElementPatternKind::EdgePatternLeft,
                    Some(TokenKind::Char(']')),
                )
            } else {
                let quantifier = self.parse_graph_pattern_quantifier()?;
                return Ok(GraphElementPattern {
                    node_tag: NodeTag::GraphElementPattern,
                    kind: GraphElementPatternKind::EdgePatternLeft,
                    quantifier,
                    location: location as ParseLoc,
                    ..GraphElementPattern::default()
                });
            }
        } else if self.consume(TokenKind::RightArrow) {
            let quantifier = self.parse_graph_pattern_quantifier()?;
            return Ok(GraphElementPattern {
                node_tag: NodeTag::GraphElementPattern,
                kind: GraphElementPatternKind::EdgePatternRight,
                quantifier,
                location: location as ParseLoc,
                ..GraphElementPattern::default()
            });
        } else {
            self.expect(TokenKind::Char('-'))?;
            if self.consume(TokenKind::Char('[')) {
                (
                    GraphElementPatternKind::EdgePatternAny,
                    Some(TokenKind::Char(']')),
                )
            } else if self.consume(TokenKind::Char('>')) {
                let quantifier = self.parse_graph_pattern_quantifier()?;
                return Ok(GraphElementPattern {
                    node_tag: NodeTag::GraphElementPattern,
                    kind: GraphElementPatternKind::EdgePatternRight,
                    quantifier,
                    location: location as ParseLoc,
                    ..GraphElementPattern::default()
                });
            } else {
                let quantifier = self.parse_graph_pattern_quantifier()?;
                return Ok(GraphElementPattern {
                    node_tag: NodeTag::GraphElementPattern,
                    kind: GraphElementPatternKind::EdgePatternAny,
                    quantifier,
                    location: location as ParseLoc,
                    ..GraphElementPattern::default()
                });
            }
        };

        let variable = if close.is_some_and(|close| self.at(close))
            || self.at(TokenKind::Is)
            || self.at(TokenKind::Where)
        {
            None
        } else {
            self.consume_col_id()
        };
        let labelexpr = if self.consume(TokenKind::Is) {
            Some(Box::new(self.parse_graph_label_expression()?))
        } else {
            None
        };
        let where_clause = if self.consume(TokenKind::Where) {
            Some(self.parse_expr_box_strict_until(&[close.unwrap()])?)
        } else {
            None
        };
        if let Some(close) = close {
            self.expect(close)?;
        }
        let mut actual_kind = kind;
        if matches!(
            kind,
            GraphElementPatternKind::EdgePatternLeft | GraphElementPatternKind::EdgePatternAny
        ) && (self.consume(TokenKind::RightArrow)
            || (self.consume(TokenKind::Char('-')) && self.consume(TokenKind::Char('>'))))
        {
            actual_kind = GraphElementPatternKind::EdgePatternRight;
        }
        let quantifier = self.parse_graph_pattern_quantifier()?;
        Ok(GraphElementPattern {
            node_tag: NodeTag::GraphElementPattern,
            kind: actual_kind,
            variable,
            labelexpr,
            where_clause,
            quantifier,
            location: location as ParseLoc,
            ..GraphElementPattern::default()
        })
    }

    pub(super) fn parse_graph_label_expression(&mut self) -> PResult<Node> {
        let make_term = |name: std::string::String, location: usize| {
            Node::ColumnRef(ColumnRef {
                node_tag: NodeTag::ColumnRef,
                fields: vec![make_string_node(name)],
                location: location as ParseLoc,
            })
        };
        let location = self.location();
        let first = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected a graph label"))?;
        let mut expression = make_term(first, location);
        while self.at(TokenKind::Char('|'))
            || (self.at(TokenKind::Op) && token_name(self.peek()).as_deref() == Some("|"))
        {
            let operator_location = self.advance().location();
            let term_location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a graph label after '|'"))?;
            let term = make_term(name, term_location);
            match expression {
                Node::BoolExpr(BoolExpr {
                    boolop: BoolExprType::OrExpr,
                    ref mut args,
                    ..
                }) => args.push(term),
                left => {
                    expression =
                        make_bool_expr(BoolExprType::OrExpr, left, term, operator_location);
                }
            }
        }
        Ok(expression)
    }

    pub(super) fn parse_graph_pattern_quantifier(&mut self) -> PResult<NodeList> {
        let mut quantifier = Vec::new();
        if self.consume(TokenKind::Char('{')) {
            let (first, second) = if self.consume(TokenKind::Char(',')) {
                let second = match self.advance().value {
                    Some(TokenValue::Integer(value)) => value,
                    _ => return Err(self.error_here("expected an upper graph quantifier")),
                };
                (0, second)
            } else {
                let first = match self.advance().value {
                    Some(TokenValue::Integer(value)) => value,
                    _ => return Err(self.error_here("expected a graph quantifier")),
                };
                let second = if self.consume(TokenKind::Char(',')) {
                    match self.advance().value {
                        Some(TokenValue::Integer(value)) => value,
                        _ => return Err(self.error_here("expected an upper graph quantifier")),
                    }
                } else {
                    first
                };
                (first, second)
            };
            self.expect(TokenKind::Char('}'))?;
            quantifier = vec![
                Node::Integer(Integer::new(first)),
                Node::Integer(Integer::new(second)),
            ];
        }
        Ok(quantifier)
    }
}
