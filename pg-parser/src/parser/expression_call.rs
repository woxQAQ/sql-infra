use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_name_or_func(&mut self) -> Option<Node> {
        let location = self.location();
        let fields = self.parse_expression_name_nodes()?;
        if fields.is_empty() {
            return None;
        }
        if self.consume(TokenKind::Char('(')) {
            let mut call = FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: fields,
                location: location as ParseLoc,
                ..FuncCall::default()
            };
            if self.consume(TokenKind::Char('*')) {
                call.agg_star = true;
                self.expect(TokenKind::Char(')'))?;
            } else if self.consume(TokenKind::Char(')')) {
            } else {
                let agg_all = self.consume(TokenKind::All);
                call.agg_distinct = self.consume(TokenKind::Distinct);
                if agg_all && call.agg_distinct {
                    return self.fail("ALL and DISTINCT cannot be used together");
                }
                loop {
                    let slot = if call.args.is_empty() {
                        CompletionSlot::FunctionArgument
                    } else {
                        CompletionSlot::FunctionArgumentAfterComma
                    };
                    if self.at(TokenKind::Variadic) {
                        if agg_all || call.agg_distinct {
                            return self.fail("ALL/DISTINCT cannot be used with VARIADIC");
                        }
                        self.advance();
                        call.func_variadic = true;
                        call.args.push(self.parse_expr_at(slot, 0)?);
                        break;
                    }
                    call.args.push(self.parse_function_argument_at(slot)?);
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if matches!(self.peek_kind(), TokenKind::Order | TokenKind::Char(')')) {
                        return self.fail("expected a function argument after ','");
                    }
                }
                if call.agg_distinct && call.args.is_empty() {
                    return self.fail("DISTINCT requires at least one function argument");
                }
                if self.consume(TokenKind::Order) {
                    self.expect(TokenKind::By)?;
                    call.agg_order = self.parse_expression_sort_list(
                        CompletionSlot::FunctionOrderBy,
                        CompletionSlot::FunctionOrderByAfterComma,
                        TokenKind::Char(')'),
                    )?;
                }
                self.expect(TokenKind::Char(')'))?;
            }
            self.parse_function_decorations(&mut call)?;
            Some(Node::FuncCall(call))
        } else {
            Some(Node::ColumnRef(ColumnRef {
                node_tag: NodeTag::ColumnRef,
                fields,
                location: location as ParseLoc,
            }))
        }
    }

    pub(super) fn parse_array_expr_body(
        &mut self,
        location: usize,
        list_start: usize,
    ) -> Option<Node> {
        let mut elements = Vec::new();
        if !self.at(TokenKind::Char(']')) {
            if self.at(TokenKind::Char('[')) {
                loop {
                    let nested_start = self.expect(TokenKind::Char('['))?.location();
                    elements.push(self.parse_array_expr_body(nested_start, nested_start)?);
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if !self.at(TokenKind::Char('[')) {
                        return self.fail("nested ARRAY elements must all be arrays");
                    }
                }
            } else {
                elements = self.parse_expr_list_until_at(
                    CompletionSlot::ArrayElement,
                    CompletionSlot::ArrayElementAfterComma,
                    TokenKind::Char(']'),
                )?;
            }
        }
        let list_end = self.expect(TokenKind::Char(']'))?.location();
        Some(Node::AArrayExpr(AArrayExpr {
            node_tag: NodeTag::AArrayExpr,
            elements,
            list_start: list_start as ParseLoc,
            list_end: list_end as ParseLoc,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_function_argument_at(&mut self, slot: CompletionSlot) -> Option<Node> {
        if self.starts_named_function_argument() {
            let name_token = self.advance().clone();
            let location = name_token.location();
            self.advance();
            return Some(Node::NamedArgExpr(NamedArgExpr {
                xpr: Expr::new(NodeTag::NamedArgExpr),
                arg: Some(Box::new(self.parse_expr_at(slot, 0)?)),
                name: token_name(&name_token),
                argnumber: -1,
                location: location as ParseLoc,
            }));
        }
        self.parse_expr_at(slot, 0)
    }

    pub(super) fn starts_named_function_argument(&self) -> bool {
        self.identifier_in_categories(&[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName])
            && matches!(
                self.peek_kind_n(1),
                TokenKind::EqualsGreater | TokenKind::ColonEquals
            )
    }

    pub(super) fn parse_plain_function_arguments_after_at(
        &mut self,
        first: Node,
        continuation_slot: CompletionSlot,
    ) -> Option<NodeList> {
        let mut args = vec![first];
        while self.consume(TokenKind::Char(',')) {
            if self.at(TokenKind::Char(')')) {
                return self.fail("expected a function argument after ','");
            }
            args.push(self.parse_function_argument_at(continuation_slot)?);
        }
        Some(args)
    }

    pub(super) fn parse_expression_sort_list(
        &mut self,
        first_slot: CompletionSlot,
        continuation_slot: CompletionSlot,
        stop: TokenKind,
    ) -> Option<NodeList> {
        if self.at_completion_cursor() {
            self.record_expression_completion_at(first_slot, false);
        }
        let mut items = Vec::new();
        while !self.at(stop) && !self.at(TokenKind::Eof) {
            let slot = if items.is_empty() {
                first_slot
            } else {
                continuation_slot
            };
            let expression = self.parse_expr_at(slot, 0)?;
            let mut sortby_dir = SortByDir::Default;
            let mut use_op = Vec::new();
            let mut location = -1;
            match self.peek_kind() {
                TokenKind::Asc => {
                    self.advance();
                    sortby_dir = SortByDir::Asc;
                }
                TokenKind::Desc => {
                    self.advance();
                    sortby_dir = SortByDir::Desc;
                }
                TokenKind::Using => {
                    self.advance();
                    sortby_dir = SortByDir::Using;
                    location = self.location() as ParseLoc;
                    if self.at(TokenKind::Operator) {
                        use_op = self.parse_explicit_operator_name()?;
                    } else {
                        let operator = self.peek().clone();
                        if !matches!(
                            operator.kind,
                            TokenKind::Op
                                | TokenKind::Char('+')
                                | TokenKind::Char('-')
                                | TokenKind::Char('*')
                                | TokenKind::Char('/')
                                | TokenKind::Char('%')
                                | TokenKind::Char('^')
                                | TokenKind::Char('<')
                                | TokenKind::Char('>')
                                | TokenKind::Char('=')
                                | TokenKind::LessEquals
                                | TokenKind::GreaterEquals
                                | TokenKind::NotEquals
                                | TokenKind::RightArrow
                                | TokenKind::Char('|')
                        ) {
                            return self.fail("ORDER BY USING requires an operator");
                        }
                        self.advance();
                        use_op.push(make_string_node(token_text(&operator)));
                    }
                }
                _ => {}
            }
            let sortby_nulls = if self.consume(TokenKind::NullsP) {
                if self.consume(TokenKind::FirstP) {
                    SortByNulls::First
                } else {
                    self.expect(TokenKind::LastP)?;
                    SortByNulls::Last
                }
            } else {
                SortByNulls::Default
            };
            items.push(Node::SortBy(SortBy {
                node_tag: NodeTag::SortBy,
                node: Some(Box::new(expression)),
                sortby_dir,
                sortby_nulls,
                use_op,
                location,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(stop) || self.at(TokenKind::Eof) {
                if self.at_completion_cursor() {
                    self.record_expression_completion_at(continuation_slot, false);
                    return self.stop_for_completion();
                }
                return self.fail("expected an ORDER BY expression after ','");
            }
        }
        if items.is_empty() {
            return self.fail("ORDER BY requires at least one expression");
        }
        Some(items)
    }

    pub(super) fn parse_function_decorations(&mut self, call: &mut FuncCall) -> Option<()> {
        if self.consume(TokenKind::Within) {
            if !call.agg_order.is_empty() || call.agg_distinct || call.func_variadic {
                return self.fail("WITHIN GROUP conflicts with function argument modifiers");
            }
            self.expect(TokenKind::GroupP)?;
            self.expect(TokenKind::Char('('))?;
            self.expect(TokenKind::Order)?;
            self.expect(TokenKind::By)?;
            call.agg_order = self.parse_expression_sort_list(
                CompletionSlot::WithinGroupOrderBy,
                CompletionSlot::WithinGroupOrderByAfterComma,
                TokenKind::Char(')'),
            )?;
            self.expect(TokenKind::Char(')'))?;
            call.agg_within_group = true;
        }
        if self.consume(TokenKind::Filter) {
            self.expect(TokenKind::Char('('))?;
            self.expect(TokenKind::Where)?;
            call.agg_filter = Some(Box::new(
                self.parse_expr_at(CompletionSlot::FunctionFilter, 0)?,
            ));
            self.expect(TokenKind::Char(')'))?;
        }
        if self.consume(TokenKind::IgnoreP) {
            self.expect(TokenKind::NullsP)?;
            call.ignore_nulls = 1;
        } else if self.consume(TokenKind::RespectP) {
            self.expect(TokenKind::NullsP)?;
            call.ignore_nulls = 2;
        }
        call.over = self.parse_optional_over_clause()?;
        Some(())
    }

    pub(super) fn parse_optional_over_clause(&mut self) -> Option<Option<Box<WindowDef>>> {
        if !self.consume(TokenKind::Over) {
            return Some(None);
        }
        if self.at(TokenKind::Char('(')) {
            let location = self.advance().location();
            let range = self.take_until_balanced_range(TokenKind::Char(')'));
            let mut parser = self.parser_view(range);
            match parser.parse_window_specification_body(location, TokenKind::Eof) {
                Ok(window) => {
                    self.expect(TokenKind::Char(')'))?;
                    Some(Some(Box::new(window)))
                }
                Err(error) => {
                    if self.error.is_none() {
                        self.error = Some(error);
                    }
                    None
                }
            }
        } else {
            let name_location = self.location();
            let name = self
                .consume_identifier_in_categories(&[
                    KeywordCategory::Unreserved,
                    KeywordCategory::ColName,
                ])
                .or_else(|| self.fail("OVER requires a window name"))?;
            Some(Some(Box::new(WindowDef {
                node_tag: NodeTag::WindowDef,
                name: Some(name),
                frame_options: FRAMEOPTION_DEFAULTS,
                location: name_location as ParseLoc,
                ..WindowDef::default()
            })))
        }
    }

    pub(super) fn try_parse_typed_constant(&mut self) -> Option<Node> {
        if token_name(self.peek()).is_none() || self.at(TokenKind::SConst) {
            return None;
        }
        let start = self.pos;
        let mut depth = 0usize;
        let mut top_level_string_seen = false;
        let mut best = None;
        for end in start + 1..=self.end {
            let token = &self.tokens[end - 1];
            if end - 1 > start && depth == 0 && expression_boundary(token.kind) {
                break;
            }
            if depth == 0 && token.kind == TokenKind::SConst {
                top_level_string_seen = true;
            }
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            if depth == 0
                && top_level_string_seen
                && let Ok(node @ Node::TypeCast(_)) =
                    parse_aexpr_const_tokens(self.tokens[start..end].to_vec())
            {
                best = Some((end, node));
            }
        }
        let (end, node) = best?;
        self.pos = end;
        Some(node)
    }
}
