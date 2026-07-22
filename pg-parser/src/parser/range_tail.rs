use super::*;

impl Parser {
    pub(super) fn parse_xmltable(&mut self, lateral: bool) -> PResult<RangeTableFunc> {
        let location = self.expect(TokenKind::Xmltable)?.location();
        self.expect(TokenKind::Char('('))?;
        let mut namespaces = Vec::new();
        if self.consume(TokenKind::Xmlnamespaces) {
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("XMLNAMESPACES requires at least one namespace"));
            }
            while !self.at(TokenKind::Char(')')) {
                let target_location = self.location();
                let slot = if namespaces.is_empty() {
                    CompletionSlot::XmlTableNamespace
                } else {
                    CompletionSlot::XmlTableNamespaceAfterComma
                };
                if self.consume(TokenKind::Default) {
                    let range = self
                        .take_until_top_level_range(&[TokenKind::Char(','), TokenKind::Char(')')]);
                    namespaces.push(Node::ResTarget(ResTarget {
                        node_tag: NodeTag::ResTarget,
                        val: Some(Box::new(self.parse_b_expression_range_at(slot, range)?)),
                        location: target_location as ParseLoc,
                        ..ResTarget::default()
                    }));
                } else {
                    let range = self
                        .take_until_top_level_range(&[TokenKind::Char(','), TokenKind::Char(')')]);
                    let (name, expression_range) = self.split_explicit_alias_range(range);
                    let name = name.ok_or_else(|| {
                        ParseError::new(target_location, "XML namespace requires AS alias")
                    })?;
                    namespaces.push(Node::ResTarget(ResTarget {
                        node_tag: NodeTag::ResTarget,
                        name: Some(name),
                        val: Some(Box::new(
                            self.parse_b_expression_range_at(slot, expression_range)?,
                        )),
                        location: target_location as ParseLoc,
                        ..ResTarget::default()
                    }));
                }
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected an XML namespace after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
            self.expect(TokenKind::Char(','))?;
        }
        let row_range = self.take_until_top_level_range(&[TokenKind::Passing]);
        let rowexpr = Box::new(
            self.parse_c_expression_range_at(CompletionSlot::XmlTableRowExpression, row_range)?,
        );
        self.expect(TokenKind::Passing)?;
        if self.consume(TokenKind::By)
            && !(self.consume(TokenKind::RefP) || self.consume(TokenKind::ValueP))
        {
            return Err(self.error_here("BY requires REF or VALUE"));
        }
        let mut doc_range = self.take_until_top_level_range(&[TokenKind::Columns]);
        if doc_range.end.saturating_sub(doc_range.start) >= 2
            && self.tokens[doc_range.end - 2].kind == TokenKind::By
            && matches!(
                self.tokens[doc_range.end - 1].kind,
                TokenKind::RefP | TokenKind::ValueP
            )
        {
            doc_range.end -= 2;
        }
        let docexpr = self
            .parse_c_expression_range_at(CompletionSlot::XmlTableDocumentExpression, doc_range)?;
        self.expect(TokenKind::Columns)?;
        let mut columns = Vec::new();
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("XMLTABLE COLUMNS requires at least one column"));
        }
        while !self.at(TokenKind::Char(')')) {
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            columns.push(Node::RangeTableFuncCol(xmltable_column_from_tokens(
                tokens,
            )?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an XMLTABLE column after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        let alias = self.parse_optional_alias_clause()?;
        Ok(RangeTableFunc {
            node_tag: NodeTag::RangeTableFunc,
            lateral,
            docexpr: Some(Box::new(docexpr)),
            rowexpr: Some(rowexpr),
            namespaces,
            columns,
            alias,
            location: location as ParseLoc,
        })
    }

    pub(super) fn parse_join_tail(&mut self, larg: Node, stops: &[TokenKind]) -> PResult<Node> {
        let mut is_natural = false;
        let (jointype, needs_qual) = match self.peek_kind() {
            TokenKind::Left => {
                self.advance();
                self.consume(TokenKind::OuterP);
                (JoinType::Left, true)
            }
            TokenKind::Right => {
                self.advance();
                self.consume(TokenKind::OuterP);
                (JoinType::Right, true)
            }
            TokenKind::Full => {
                self.advance();
                self.consume(TokenKind::OuterP);
                (JoinType::Full, true)
            }
            TokenKind::Cross => {
                self.advance();
                (JoinType::Inner, false)
            }
            TokenKind::InnerP => {
                self.advance();
                (JoinType::Inner, true)
            }
            TokenKind::Natural => {
                self.advance();
                is_natural = true;
                let jointype = match self.peek_kind() {
                    TokenKind::Left => {
                        self.advance();
                        self.consume(TokenKind::OuterP);
                        JoinType::Left
                    }
                    TokenKind::Right => {
                        self.advance();
                        self.consume(TokenKind::OuterP);
                        JoinType::Right
                    }
                    TokenKind::Full => {
                        self.advance();
                        self.consume(TokenKind::OuterP);
                        JoinType::Full
                    }
                    TokenKind::InnerP => {
                        self.advance();
                        JoinType::Inner
                    }
                    _ => JoinType::Inner,
                };
                (jointype, false)
            }
            TokenKind::Join => (JoinType::Inner, true),
            _ => return Err(self.error_here("expected a JOIN clause")),
        };
        self.expect(TokenKind::Join)?;
        let rarg = self.parse_from_item(&[
            TokenKind::On,
            TokenKind::Using,
            TokenKind::Char(','),
            TokenKind::Join,
            TokenKind::InnerP,
            TokenKind::Left,
            TokenKind::Right,
            TokenKind::Full,
            TokenKind::Cross,
            TokenKind::Natural,
            TokenKind::Where,
            TokenKind::GroupP,
            TokenKind::Having,
            TokenKind::Window,
            TokenKind::Order,
            TokenKind::Limit,
            TokenKind::Offset,
            TokenKind::Fetch,
            TokenKind::For,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        let mut using_clause = Vec::new();
        let mut join_using_alias = None;
        let quals = if self.consume(TokenKind::On) {
            if !needs_qual || is_natural {
                return Err(self.error_here("this JOIN form does not accept ON"));
            }
            let mut qual_stops = extend_stops(stops, TokenKind::Char(','));
            for stop in [
                TokenKind::Join,
                TokenKind::InnerP,
                TokenKind::Left,
                TokenKind::Right,
                TokenKind::Full,
                TokenKind::Cross,
                TokenKind::Natural,
            ] {
                if !qual_stops.contains(&stop) {
                    qual_stops.push(stop);
                }
            }
            Some(self.parse_expr_box_strict_until_at(CompletionSlot::JoinOn, &qual_stops)?)
        } else if self.consume(TokenKind::Using) {
            if !needs_qual || is_natural {
                return Err(self.error_here("this JOIN form does not accept USING"));
            }
            self.expect(TokenKind::Char('('))?;
            using_clause = self.parse_join_using_columns()?;
            self.expect(TokenKind::Char(')'))?;
            if self.consume(TokenKind::As) {
                join_using_alias = Some(Box::new(Alias {
                    node_tag: NodeTag::Alias,
                    aliasname: Some(
                        self.consume_col_id()
                            .ok_or_else(|| self.error_here("USING AS requires an alias"))?,
                    ),
                    ..Alias::default()
                }));
            }
            None
        } else if needs_qual {
            return Err(self.error_here("JOIN requires ON or USING"));
        } else {
            None
        };
        Ok(Node::JoinExpr(JoinExpr {
            node_tag: NodeTag::JoinExpr,
            jointype,
            is_natural,
            larg: Some(Box::new(larg)),
            rarg: Some(Box::new(rarg)),
            using_clause,
            join_using_alias,
            quals,
            ..JoinExpr::default()
        }))
    }

    fn parse_join_using_columns(&mut self) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            if self.at_completion_cursor() {
                self.record_completion_at(
                    CompletionSlot::JoinUsingColumn,
                    Expectation::Name(NameExpectation::Column(ColumnContext::JoinUsing)),
                );
                self.record_completion_at(
                    CompletionSlot::JoinUsingColumn,
                    Expectation::Token(TokenKind::Char(')')),
                );
                return Err(self.completion_stop());
            }
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a column name in JOIN USING"))?;
            names.push(make_string_node(name));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(names)
    }
}
