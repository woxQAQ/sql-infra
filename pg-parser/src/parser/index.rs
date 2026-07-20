use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createindex.html
    // CREATE [ UNIQUE ] INDEX [ CONCURRENTLY ] [ [ IF NOT EXISTS ] name ] ON [ ONLY ] table_name [ USING method ]
    //     ( { column_name | ( expression ) } [ COLLATE collation ] [ opclass [ ( opclass_parameter = value [, ... ] ) ] ] [ ASC | DESC ] [ NULLS { FIRST | LAST } ] [, ...] )
    //     [ INCLUDE ( column_name [, ...] ) ]
    //     [ NULLS [ NOT ] DISTINCT ]
    //     [ WITH ( storage_parameter [= value] [, ... ] ) ]
    //     [ TABLESPACE tablespace_name ]
    //     [ WHERE predicate ]
    pub(super) fn parse_index(&mut self, unique_seen: bool) -> PResult<Node> {
        let unique = unique_seen || self.consume(TokenKind::Unique);
        self.expect(TokenKind::Index)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        let if_not_exists = self.consume_if_not_exists()?;
        let idxname = if self.peek_kind() != TokenKind::On {
            self.consume_col_id()
        } else {
            None
        };
        if if_not_exists && idxname.is_none() {
            return Err(self.error_here("CREATE INDEX IF NOT EXISTS requires an index name"));
        }
        self.expect(TokenKind::On)?;
        if self.at_completion_cursor() {
            self.record_relation_completion_at(CompletionSlot::IndexRelation);
            return Err(self.error_here("completion cursor"));
        }
        let relation = Some(Box::new(
            self.try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("CREATE INDEX requires a relation"))?,
        ));
        let access_method = if self.consume(TokenKind::Using) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            Some("btree".to_owned())
        };
        self.expect(TokenKind::Char('('))?;
        let index_params = self.parse_index_elem_list_body()?;
        self.expect(TokenKind::Char(')'))?;

        let index_including_params = if self.consume(TokenKind::Include) {
            self.expect(TokenKind::Char('('))?;
            let columns = self.parse_index_elem_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            columns
        } else {
            Vec::new()
        };
        let nulls_not_distinct = if self.consume(TokenKind::NullsP) {
            let not_distinct = self.consume(TokenKind::Not);
            self.expect(TokenKind::Distinct)?;
            not_distinct
        } else {
            false
        };
        let options = if self.consume(TokenKind::With) {
            let options = self.parse_parenthesized_reloptions()?;
            if options.is_empty() {
                return Err(self.error_here("CREATE INDEX WITH requires an option list"));
            }
            options
        } else {
            Vec::new()
        };
        let table_space = if self.consume(TokenKind::Tablespace) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?,
            )
        } else {
            None
        };
        let where_clause = if self.consume(TokenKind::Where) {
            Some(self.parse_expr_box_strict_until_at(
                CompletionSlot::IndexPredicate,
                &[TokenKind::Char(';'), TokenKind::Eof],
            )?)
        } else {
            None
        };
        Ok(Node::IndexStmt(IndexStmt {
            node_tag: NodeTag::IndexStmt,
            idxname,
            relation,
            access_method,
            table_space,
            index_params,
            index_including_params,
            options,
            where_clause,
            unique,
            nulls_not_distinct,
            concurrent,
            if_not_exists,
            ..IndexStmt::default()
        }))
    }

    fn parse_index_elem_list_body(&mut self) -> PResult<NodeList> {
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("index element list cannot be empty"));
        }
        let mut elements = Vec::new();
        loop {
            let range =
                self.take_until_top_level_range(&[TokenKind::Char(','), TokenKind::Char(')')]);
            let location = self
                .tokens
                .get(range.start)
                .map_or(self.location(), Token::location);
            let starts_parenthesized =
                self.tokens.get(range.start).map(|token| token.kind) == Some(TokenKind::Char('('));
            let starts_with_cast =
                self.tokens.get(range.start).map(|token| token.kind) == Some(TokenKind::Cast);
            let slot = if elements.is_empty() {
                CompletionSlot::CreateIndexElement
            } else {
                CompletionSlot::CreateIndexElementAfterComma
            };
            let element = self.parse_index_elem_range(slot, range)?;
            if let Some(expression) = element.expr.as_deref()
                && !starts_parenthesized
                && !is_windowless_function_expression_node(expression, starts_with_cast)
            {
                return Err(ParseError::new(
                    location,
                    "index expressions must be parenthesized unless they are function calls",
                ));
            }
            elements.push(Node::IndexElem(element));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an index element after ','"));
            }
        }
        Ok(elements)
    }
}
pub(super) fn node_to_index_elem(node: Node) -> Node {
    match node {
        Node::ColumnRef(ColumnRef {
            fields, location, ..
        }) if fields.len() == 1 => Node::IndexElem(IndexElem {
            node_tag: NodeTag::IndexElem,
            name: fields.first().and_then(|field| match field {
                Node::String(value) => value.sval.clone(),
                _ => None,
            }),
            location,
            ..IndexElem::default()
        }),
        expr => Node::IndexElem(IndexElem {
            node_tag: NodeTag::IndexElem,
            expr: Some(Box::new(expr)),
            ..IndexElem::default()
        }),
    }
}

impl Parser {
    pub(super) fn parse_index_elem_range(
        &self,
        slot: CompletionSlot,
        range: std::ops::Range<usize>,
    ) -> PResult<IndexElem> {
        let tokens = &self.tokens[range.clone()];
        let location = tokens.first().map_or(self.location(), Token::location);
        if tokens.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion_at(slot);
            }
            return Err(ParseError::new(location, "expected an index element"));
        }
        let collate_index = find_top_level_token(tokens, TokenKind::Collate);
        let (expression, suffix_start) = if let Some(index) = collate_index {
            if index == 0 {
                return Err(ParseError::new(
                    location,
                    "COLLATE requires an index expression",
                ));
            }
            (
                self.parse_expression_range_at(slot, range.start..range.start + index)?,
                range.start + index,
            )
        } else {
            let mut parser = self.expression_view_at(slot, range.clone());
            let expression = parser.parse_expr(0).ok_or_else(|| {
                parser
                    .error
                    .take()
                    .unwrap_or_else(|| ParseError::new(location, "invalid index expression"))
            })?;
            (expression, parser.pos)
        };
        let Node::IndexElem(mut element) = node_to_index_elem(expression) else {
            unreachable!("node_to_index_elem always returns IndexElem");
        };
        element.location = location as ParseLoc;

        let mut suffix = self.bounded_view(suffix_start..range.end);
        if suffix.consume(TokenKind::Collate) {
            element.collation = suffix.parse_name_list();
            if element.collation.is_empty() {
                return Err(suffix.error_here("COLLATE requires a collation name"));
            }
        }
        if !suffix.at_any(&[
            TokenKind::Asc,
            TokenKind::Desc,
            TokenKind::NullsP,
            TokenKind::Eof,
        ]) {
            element.opclass = suffix.parse_name_list();
            if element.opclass.is_empty() {
                return Err(suffix.error_here("expected an operator class name"));
            }
            if suffix.at(TokenKind::Char('(')) {
                element.opclassopts = suffix.parse_parenthesized_reloptions()?;
            }
        }
        element.ordering = if suffix.consume(TokenKind::Asc) {
            SortByDir::Asc
        } else if suffix.consume(TokenKind::Desc) {
            SortByDir::Desc
        } else {
            SortByDir::Default
        };
        if suffix.consume(TokenKind::NullsP) {
            element.nulls_ordering = if suffix.consume(TokenKind::FirstP) {
                SortByNulls::First
            } else if suffix.consume(TokenKind::LastP) {
                SortByNulls::Last
            } else {
                return Err(suffix.error_here("NULLS requires FIRST or LAST"));
            };
        }
        if !suffix.at(TokenKind::Eof) {
            return Err(suffix.error_here("unexpected token after index element options"));
        }
        Ok(element)
    }
}
