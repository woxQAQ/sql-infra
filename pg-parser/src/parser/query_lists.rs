//! Strict query-list parsing for targets, values, grouping, sorting, and expressions.
//!
//! List terminators and completion follow tokens are handled together to prevent
//! callers from silently accepting missing elements.

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
            values.push(node!(AArrayExpr {
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
        self.record_completion_tokens(&[TokenKind::Char('*')]);
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            if tokens.is_empty() && !self.at_completion() {
                return Err(self.error_here("expected an expression"));
            }
            if self.at_completion()
                && tokens.last().has_kind(TokenKind::As)
                && parse_expression_tokens(tokens[..tokens.len() - 1].to_vec()).is_ok()
            {
                self.record_completion_slot(GrammarSlot::Alias);
                return Err(self.error_here("expected an output alias after AS"));
            }
            let (name, mut expr_tokens) = if tokens.is_empty() {
                (None, Vec::new())
            } else {
                split_target_alias(tokens)
            };
            let standalone_star =
                matches!(expr_tokens.as_slice(), [token] if token.kind == TokenKind::Char('*'));
            if standalone_star && name.is_some() {
                return Err(self.error_here("an unqualified '*' cannot have an output alias"));
            }
            if self.at_completion()
                && !expr_tokens.is_empty()
                && parse_expression_tokens(expr_tokens.clone()).is_ok()
            {
                self.record_completion_follow_tokens(&[TokenKind::As]);
                self.record_completion_slot(GrammarSlot::Alias);
            }
            self.record_expression_follow_tokens(
                &expr_tokens,
                &extend_stops(stops, TokenKind::Char(',')),
                false,
            );
            if standalone_star {
                self.record_completion_follow_tokens(&extend_stops(stops, TokenKind::Char(',')));
            }
            self.append_completion_marker(&mut expr_tokens);
            let target_value = if standalone_star {
                node!(ColumnRef {
                    fields: vec![Node::AStar],
                    location: location as ParseLoc,
                })
            } else {
                parse_expression_tokens_with_completion(expr_tokens, self.completion.clone())?
            };
            items.push(node!(ResTarget {
                name,
                val: Some(Box::new(target_value)),
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
        while self.at_completion() || !self.at_any(stops) {
            let mut tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            self.record_expression_follow_tokens(
                &tokens,
                &extend_stops(stops, TokenKind::Char(',')),
                false,
            );
            self.append_completion_marker(&mut tokens);
            if tokens.is_empty() {
                return Err(self.error_here("expected an expression"));
            }
            items.push(parse_expression_tokens_with_completion(
                tokens,
                self.completion.clone(),
            )?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected an expression after ','"));
            }
        }
        Ok(items)
    }

    pub(super) fn parse_group_by_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        self.record_completion_slot(GrammarSlot::Column);
        self.record_completion_slot(GrammarSlot::Function);
        let mut items = Vec::new();
        while self.at_completion() || !self.at_any(stops) {
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
            return Ok(node!(GroupingSet {
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
            TokenKind::Grouping if self.peek_kind_n(1) == TokenKind::Completion => {
                let location = self.advance().location();
                self.record_completion_tokens(&[TokenKind::Sets]);
                self.pos = self.pos.saturating_sub(1);
                (None, location)
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
            return Ok(node!(GroupingSet {
                kind,
                content,
                location: location as ParseLoc,
            }));
        }

        let mut tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
        self.record_expression_follow_tokens(
            &tokens,
            &extend_stops(stops, TokenKind::Char(',')),
            false,
        );
        self.append_completion_marker(&mut tokens);
        parse_expression_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let mut tokens = self.take_until_top_level(stops);
        self.record_expression_follow_tokens(&tokens, stops, false);
        self.append_completion_marker(&mut tokens);
        parse_expression_tokens_with_completion(tokens, self.completion.clone()).map(Box::new)
    }

    pub(super) fn parse_optional_expr_clause(
        &mut self,
        start: TokenKind,
        stops: &[TokenKind],
    ) -> PResult<Option<Box<Node>>> {
        if !self.consume(start) {
            return Ok(None);
        }
        self.parse_expr_box_strict_until(stops).map(Some)
    }

    pub(super) fn parse_parenthesized_expr_box(&mut self) -> PResult<Box<Node>> {
        self.expect(TokenKind::Char('('))?;
        let expr = self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?;
        self.expect(TokenKind::Char(')'))?;
        Ok(expr)
    }

    pub(super) fn parse_b_expr_box_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let mut tokens = self.take_until_top_level(stops);
        self.record_expression_follow_tokens(&tokens, stops, true);
        self.append_completion_marker(&mut tokens);
        parse_b_expression_tokens_with_completion(tokens, self.completion.clone()).map(Box::new)
    }

    pub(super) fn record_expression_follow_tokens(
        &self,
        tokens: &[Token],
        follows: &[TokenKind],
        restricted: bool,
    ) {
        if !self.at_completion() || tokens.is_empty() {
            return;
        }
        let complete = if restricted {
            parse_b_expression_tokens(tokens.to_vec()).is_ok()
        } else {
            parse_expression_tokens(tokens.to_vec()).is_ok()
        };
        if complete {
            self.record_completion_follow_tokens(follows);
            for follow in follows {
                if let Some(phrase) = completion::follow_phrase(*follow) {
                    self.record_completion_follow_phrase(phrase);
                }
            }
        }
    }

    pub(super) fn parse_sort_list_strict_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        self.record_completion_slot(GrammarSlot::Column);
        self.record_completion_slot(GrammarSlot::Function);
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let mut tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            self.record_sort_item_expectations(&tokens, stops);
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
                if operator_tokens.first().has_kind(TokenKind::Operator) {
                    if !operator_tokens.get(1).has_kind(TokenKind::Char('('))
                        || !operator_tokens.last().has_kind(TokenKind::Char(')'))
                    {
                        return Err(ParseError::syntax_exit(
                            location as usize,
                            "invalid OPERATOR decoration",
                        ));
                    }
                    operator_tokens = operator_tokens[2..operator_tokens.len() - 1].to_vec();
                }
                use_op = parse_operator_name_tokens(operator_tokens, location as usize)?;
                sortby_dir = SortByDir::Using;
            }
            let node = self.parse_expression_fragment_tokens(tokens)?;
            items.push(node!(SortBy {
                node: Some(Box::new(node)),
                sortby_dir,
                sortby_nulls,
                use_op,
                location,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected an ORDER BY expression after ','"));
            }
        }
        if items.is_empty() {
            return Err(self.error_here("ORDER BY requires at least one expression"));
        }
        Ok(items)
    }

    fn record_sort_item_expectations(&self, tokens: &[Token], stops: &[TokenKind]) {
        if !self.at_completion() || tokens.is_empty() {
            return;
        }
        if let Some(using) = find_top_level_token(tokens, TokenKind::Using) {
            let decoration = &tokens[using + 1..];
            if decoration.len() == 1 && decoration[0].kind == TokenKind::Operator {
                self.record_completion_tokens(&[TokenKind::Char('(')]);
                return;
            }
            if decoration.first().has_kind(TokenKind::Operator)
                && decoration.get(1).has_kind(TokenKind::Char('('))
                && !decoration.last().has_kind(TokenKind::Char(')'))
            {
                if matches!(
                    decoration.last().map(|token| token.kind),
                    Some(TokenKind::Char('(') | TokenKind::Char('.'))
                ) {
                    self.record_completion_slot(GrammarSlot::Operator);
                } else {
                    self.record_completion_tokens(&[TokenKind::Char(')')]);
                }
                return;
            }
        }
        if tokens.last().has_kind(TokenKind::NullsP) {
            self.record_completion_tokens(&[TokenKind::FirstP, TokenKind::LastP]);
            return;
        }
        if tokens.last().has_kind(TokenKind::Using) {
            self.record_completion_tokens(&[TokenKind::Op, TokenKind::Operator]);
            self.record_completion_slot(GrammarSlot::Operator);
            return;
        }

        let mut expression_end = tokens.len();
        let mut has_nulls = false;
        if expression_end >= 2
            && tokens[expression_end - 2].kind == TokenKind::NullsP
            && matches!(
                tokens[expression_end - 1].kind,
                TokenKind::FirstP | TokenKind::LastP
            )
        {
            expression_end -= 2;
            has_nulls = true;
        }
        let mut has_direction = false;
        if expression_end > 0
            && matches!(
                tokens[expression_end - 1].kind,
                TokenKind::Asc | TokenKind::Desc
            )
        {
            expression_end -= 1;
            has_direction = true;
        } else if let Some(using) =
            find_top_level_token(&tokens[..expression_end], TokenKind::Using)
        {
            expression_end = using;
            has_direction = true;
        }
        if expression_end == 0
            || parse_expression_tokens(tokens[..expression_end].to_vec()).is_err()
        {
            return;
        }

        let mut follows = extend_stops(stops, TokenKind::Char(','));
        if !has_nulls {
            follows.push(TokenKind::NullsP);
        }
        if !has_direction {
            follows.extend([TokenKind::Using, TokenKind::Asc, TokenKind::Desc]);
        }
        self.record_completion_follow_tokens(&follows);
    }
}
