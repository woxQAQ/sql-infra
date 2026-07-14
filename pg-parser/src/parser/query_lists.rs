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
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            if tokens.is_empty() {
                return Err(self.error_here("expected an expression"));
            }
            let (name, expr_tokens) = split_target_alias(tokens);
            let val = match parse_expression_tokens(expr_tokens)? {
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
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            if tokens.is_empty() {
                return Err(self.error_here("expected an expression"));
            }
            items.push(parse_expression_tokens(tokens)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an expression after ','"));
            }
        }
        Ok(items)
    }

    pub(super) fn parse_group_by_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            items.push(self.parse_group_by_item(stops)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
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

        let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
        parse_expression_tokens(tokens)
    }

    pub(super) fn parse_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let tokens = self.take_until_top_level(stops);
        parse_expression_tokens(tokens).map(Box::new)
    }

    pub(super) fn parse_b_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let tokens = self.take_until_top_level(stops);
        parse_b_expression_tokens(tokens).map(Box::new)
    }

    pub(super) fn parse_sort_list_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let mut tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            let mut location = -1;
            let mut sortby_dir = SortByDir::Default;
            let mut sortby_nulls = SortByNulls::Default;
            let mut use_op = Vec::new();
            if tokens.len() >= 2 && tokens[tokens.len() - 2].kind == TokenKind::NullsP {
                sortby_nulls = match tokens.last().map(|token| token.kind) {
                    Some(TokenKind::FirstP) => SortByNulls::First,
                    Some(TokenKind::LastP) => SortByNulls::Last,
                    _ => {
                        return Err(ParseError::ranged(
                            tokens[tokens.len() - 2].range,
                            "NULLS requires FIRST or LAST",
                        ));
                    }
                };
                tokens.truncate(tokens.len() - 2);
            }
            if let Some(token) = tokens.last()
                && (token.kind == TokenKind::Asc || token.kind == TokenKind::Desc)
            {
                sortby_dir = if token.kind == TokenKind::Asc {
                    SortByDir::Asc
                } else {
                    SortByDir::Desc
                };
                tokens.pop();
            }
            if sortby_dir == SortByDir::Default
                && let Some(using_index) = find_top_level_token(&tokens, TokenKind::Using)
            {
                let missing_operator_location = tokens[using_index].end_location();
                let mut operator_tokens = tokens.split_off(using_index + 1);
                tokens.pop();
                location = operator_tokens
                    .first()
                    .map_or(missing_operator_location as ParseLoc, |token| {
                        token.location() as ParseLoc
                    });
                if operator_tokens.first().map(|token| token.kind) == Some(TokenKind::Operator) {
                    if operator_tokens.get(1).map(|token| token.kind) != Some(TokenKind::Char('('))
                        || operator_tokens.last().map(|token| token.kind)
                            != Some(TokenKind::Char(')'))
                    {
                        return Err(ParseError::new(
                            location as usize,
                            "invalid OPERATOR decoration",
                        ));
                    }
                    operator_tokens = operator_tokens[2..operator_tokens.len() - 1].to_vec();
                }
                use_op = parse_operator_name_tokens(operator_tokens, location as usize)?;
                sortby_dir = SortByDir::Using;
            }
            let node = parse_expression_tokens(tokens)?;
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
            if self.at_any(stops) {
                return Err(self.error_here("expected an ORDER BY expression after ','"));
            }
        }
        if items.is_empty() {
            return Err(self.error_here("ORDER BY requires at least one expression"));
        }
        Ok(items)
    }
}
