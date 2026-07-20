use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-values.html
    // VALUES ( expression [, ...] ) [, ...]
    //     [ ORDER BY sort_expression [ ASC | DESC | USING operator ] [, ...] ]
    //     [ LIMIT { count | ALL } ]
    //     [ OFFSET start [ ROW | ROWS ] ]
    //     [ FETCH { FIRST | NEXT } [ count ] { ROW | ROWS } ONLY ]
    pub(super) fn parse_values_lists(&mut self) -> PResult<NodeList> {
        let mut values = Vec::new();
        while self.consume(TokenKind::Char('(')) {
            let elements = self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?;
            if elements.is_empty() {
                return Err(self.error_here("VALUES row requires at least one expression"));
            }
            self.expect(TokenKind::Char(')'))?;
            values.push(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements,
                ..AArrayExpr::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at(TokenKind::Char('(')) {
                return Err(self.error_here("expected a VALUES row after ','"));
            }
        }
        Ok(values)
    }

    pub(super) fn parse_res_target_list_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        if self.at_completion_cursor() {
            self.record_expression_completion();
        }
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let range = self.take_until_top_level_range(&extend_stops(stops, TokenKind::Char(',')));
            if range.is_empty() {
                return Err(self.error_here("expected an expression"));
            }
            let (name, expr_range) = self.split_target_alias_range(range);
            let val = match self.parse_expression_range(expr_range)? {
                Node::AStar(star) => Node::ColumnRef(ColumnRef {
                    node_tag: NodeTag::ColumnRef,
                    fields: vec![Node::AStar(star)],
                    location: location as ParseLoc,
                }),
                val => val,
            };
            items.push(Node::ResTarget(ResTarget {
                node_tag: NodeTag::ResTarget,
                name,
                val: Some(Box::new(val)),
                location: location as ParseLoc,
                ..ResTarget::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an expression after ','"));
            }
        }
        Ok(items)
    }

    pub(super) fn parse_expr_list_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        if self.at_completion_cursor() {
            self.record_expression_completion();
        }
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let range = self.take_until_top_level_range(&extend_stops(stops, TokenKind::Char(',')));
            if range.is_empty() {
                return Err(self.error_here("expected an expression"));
            }
            items.push(self.parse_expression_range(range)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an expression after ','"));
            }
        }
        Ok(items)
    }

    pub(super) fn parse_group_by_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        if self.at_completion_cursor() {
            self.record_expression_completion();
        }
        let mut items = Vec::new();
        while !self.at_any(stops) {
            items.push(self.parse_group_by_item(stops)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a grouping item after ','"));
            }
        }
        Ok(items)
    }

    pub(super) fn parse_group_by_item(&mut self, stops: &[TokenKind]) -> PResult<Node> {
        if self.at(TokenKind::Char('(')) && self.peek_kind_n(1) == TokenKind::Char(')') {
            let location = self.advance().location();
            self.advance();
            return Ok(Node::GroupingSet(GroupingSet {
                node_tag: NodeTag::GroupingSet,
                kind: GroupingSetKind::Empty,
                location: location as ParseLoc,
                ..GroupingSet::default()
            }));
        }

        let (kind, location) = match self.peek_kind() {
            TokenKind::Rollup => {
                let location = self.advance().location();
                (Some(GroupingSetKind::Rollup), location)
            }
            TokenKind::Cube => {
                let location = self.advance().location();
                (Some(GroupingSetKind::Cube), location)
            }
            TokenKind::Grouping if self.peek_kind_n(1) == TokenKind::Sets => {
                let location = self.advance().location();
                self.advance();
                (Some(GroupingSetKind::Sets), location)
            }
            _ => (None, self.location()),
        };

        if let Some(kind) = kind {
            self.expect(TokenKind::Char('('))?;
            let content = if kind == GroupingSetKind::Sets {
                self.parse_group_by_list_until(&[TokenKind::Char(')')])?
            } else {
                self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?
            };
            if content.is_empty() {
                return Err(self.error_here("grouping set requires at least one item"));
            }
            self.expect(TokenKind::Char(')'))?;
            return Ok(Node::GroupingSet(GroupingSet {
                node_tag: NodeTag::GroupingSet,
                kind,
                content,
                location: location as ParseLoc,
            }));
        }

        let range = self.take_until_top_level_range(&extend_stops(stops, TokenKind::Char(',')));
        self.parse_expression_range(range)
    }

    pub(super) fn parse_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        if self.at_completion_cursor() {
            self.record_expression_completion();
        }
        let range = self.take_until_top_level_range(stops);
        self.parse_expression_range(range).map(Box::new)
    }

    pub(super) fn parse_b_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let range = self.take_until_top_level_range(stops);
        self.parse_b_expression_range(range).map(Box::new)
    }

    pub(super) fn parse_sort_list_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        if self.at_completion_cursor() {
            self.record_expression_completion();
        }
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let range = self.take_until_top_level_range(&extend_stops(stops, TokenKind::Char(',')));
            let mut expression_end = range.end;
            let mut location = -1;
            let mut sortby_dir = SortByDir::Default;
            let mut sortby_nulls = SortByNulls::Default;
            let mut use_op = Vec::new();
            if expression_end.saturating_sub(range.start) >= 2
                && self.tokens[expression_end - 2].kind == TokenKind::NullsP
            {
                sortby_nulls = match self.tokens[expression_end - 1].kind {
                    TokenKind::FirstP => SortByNulls::First,
                    TokenKind::LastP => SortByNulls::Last,
                    _ => {
                        return Err(ParseError::ranged(
                            self.tokens[expression_end - 2].range,
                            "NULLS requires FIRST or LAST",
                        ));
                    }
                };
                expression_end -= 2;
            }
            if let Some(token) = self.tokens.get(expression_end.saturating_sub(1))
                && expression_end > range.start
                && (token.kind == TokenKind::Asc || token.kind == TokenKind::Desc)
            {
                sortby_dir = if token.kind == TokenKind::Asc {
                    SortByDir::Asc
                } else {
                    SortByDir::Desc
                };
                expression_end -= 1;
            }
            if sortby_dir == SortByDir::Default
                && let Some(relative_using_index) = find_top_level_token(
                    &self.tokens[range.start..expression_end],
                    TokenKind::Using,
                )
            {
                let using_index = range.start + relative_using_index;
                let missing_operator_location = self.tokens[using_index].end_location();
                let mut operator_start = using_index + 1;
                let mut operator_end = expression_end;
                location = self
                    .tokens
                    .get(operator_start)
                    .map_or(missing_operator_location as ParseLoc, |token| {
                        token.location() as ParseLoc
                    });
                if self.tokens.get(operator_start).map(|token| token.kind)
                    == Some(TokenKind::Operator)
                {
                    if self.tokens.get(operator_start + 1).map(|token| token.kind)
                        != Some(TokenKind::Char('('))
                        || self
                            .tokens
                            .get(operator_end.saturating_sub(1))
                            .map(|token| token.kind)
                            != Some(TokenKind::Char(')'))
                    {
                        return Err(ParseError::new(
                            location as usize,
                            "invalid OPERATOR decoration",
                        ));
                    }
                    operator_start += 2;
                    operator_end -= 1;
                }
                let operator_tokens = self.tokens[operator_start..operator_end].to_vec();
                use_op = parse_operator_name_tokens(operator_tokens, location as usize)?;
                sortby_dir = SortByDir::Using;
                expression_end = using_index;
            }
            let node = self.parse_expression_range(range.start..expression_end)?;
            items.push(Node::SortBy(SortBy {
                node_tag: NodeTag::SortBy,
                node: Some(Box::new(node)),
                sortby_dir,
                sortby_nulls,
                use_op,
                location,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an ORDER BY expression after ','"));
            }
        }
        if items.is_empty() {
            return Err(self.error_here("ORDER BY requires at least one expression"));
        }
        Ok(items)
    }

    fn split_target_alias_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> (Option<std::string::String>, std::ops::Range<usize>) {
        let tokens = &self.tokens[range.clone()];
        let mut depth = 0usize;
        let mut alias_index = None;
        for (index, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                TokenKind::As if depth == 0 => alias_index = Some(index),
                _ => {}
            }
        }
        if let Some(index) = alias_index
            && index + 2 == tokens.len()
            && let Some(alias) = tokens.get(index + 1)
        {
            let accepted = matches!(alias.kind, TokenKind::Ident | TokenKind::UIdent)
                || match &alias.value {
                    Some(TokenValue::Keyword(word)) => lookup_keyword(word).is_some(),
                    _ => false,
                };
            if accepted && let Some(name) = token_name(alias) {
                return (Some(name), range.start..range.start + index);
            }
        }
        if self.expression_range_is_valid(range.clone()) || tokens.len() < 2 {
            return (None, range);
        }
        let alias = tokens.last().expect("checked token length");
        let accepted = matches!(alias.kind, TokenKind::Ident | TokenKind::UIdent)
            || match &alias.value {
                Some(TokenValue::Keyword(word)) => lookup_keyword(word)
                    .is_some_and(|keyword| keyword.bare_label == BareLabel::Bare),
                _ => false,
            };
        let continues_expression = expression_boundary(alias.kind)
            || matches!(
                alias.kind,
                TokenKind::Escape
                    | TokenKind::Filter
                    | TokenKind::Within
                    | TokenKind::Over
                    | TokenKind::Collate
                    | TokenKind::Isnull
                    | TokenKind::Notnull
            );
        let expression = range.start..range.end - 1;
        if accepted
            && !continues_expression
            && self.expression_range_is_valid(expression.clone())
            && let Some(name) = token_name(alias)
        {
            return (Some(name), expression);
        }
        (None, range)
    }
}
