use super::*;

pub(super) struct ExprParser {
    pub(super) tokens: Rc<[Token]>,
    pub(super) start: usize,
    pub(super) pos: usize,
    pub(super) end: usize,
    pub(super) eof: Token,
    pub(super) error: Option<ParseError>,
    pub(super) completion: Option<SharedCompletionRecorder>,
    pub(super) completion_slot: Option<CompletionSlot>,
    pub(super) completion_root_slot: Option<CompletionSlot>,
}

impl ExprParser {
    pub(super) fn active_completion_slot(&self) -> CompletionSlot {
        match self.completion_slot {
            Some(slot) => slot,
            None => panic!("expression completion parser must carry a semantic slot"),
        }
    }

    /// Build a standalone expression parser for an owned/transformed stream.
    /// Expressions sliced from an outer parser use `from_shared_range`.
    pub(super) fn from_owned_tokens(mut tokens: Vec<Token>) -> Self {
        let eof = match tokens.last() {
            Some(token) if token.kind == TokenKind::Eof => token.clone(),
            Some(token) => Token::synthetic(TokenKind::Eof, token.end_location()),
            None => Token::synthetic(TokenKind::Eof, 0),
        };
        if tokens
            .last()
            .is_some_and(|token| token.kind == TokenKind::Eof)
        {
            tokens.pop();
        }
        let end = tokens.len();
        Self {
            tokens: Rc::from(tokens),
            start: 0,
            pos: 0,
            end,
            eof,
            error: None,
            completion: None,
            completion_slot: None,
            completion_root_slot: None,
        }
    }

    pub(super) fn from_shared_range(
        tokens: Rc<[Token]>,
        range: std::ops::Range<usize>,
        eof_location: usize,
        completion: Option<SharedCompletionRecorder>,
        completion_slot: Option<CompletionSlot>,
    ) -> Self {
        assert!(range.start <= range.end && range.end <= tokens.len());
        Self {
            tokens,
            start: range.start,
            pos: range.start,
            end: range.end,
            eof: Token::synthetic(TokenKind::Eof, eof_location),
            error: None,
            completion,
            completion_slot,
            completion_root_slot: completion_slot,
        }
    }

    pub(super) fn parser_view(&self, range: std::ops::Range<usize>) -> Parser {
        self.parser_view_with_completion(range, self.completion.clone())
    }

    fn parser_view_without_completion(&self, range: std::ops::Range<usize>) -> Parser {
        self.parser_view_with_completion(range, None)
    }

    fn parser_view_with_completion(
        &self,
        range: std::ops::Range<usize>,
        completion: Option<SharedCompletionRecorder>,
    ) -> Parser {
        assert!(
            self.start <= range.start && range.start <= range.end && range.end <= self.end,
            "parser view must be contained in its parent"
        );
        let eof_location = if range.end == self.end {
            self.eof.location()
        } else {
            self.tokens[range.end].location()
        };
        Parser::from_shared_range(self.tokens.clone(), range, eof_location, completion)
    }

    pub(super) fn expression_view(&self, range: std::ops::Range<usize>) -> Self {
        assert!(
            self.start <= range.start && range.start <= range.end && range.end <= self.end,
            "expression view must be contained in its parent"
        );
        let eof_location = if range.end == self.end {
            self.eof.location()
        } else {
            self.tokens[range.end].location()
        };
        let mut view = Self::from_shared_range(
            self.tokens.clone(),
            range,
            eof_location,
            self.completion.clone(),
            self.completion_slot,
        );
        view.completion_root_slot = self.completion_root_slot;
        view
    }

    pub(super) fn record_expression_completion_at(&self, slot: CompletionSlot, restricted: bool) {
        let Some(recorder) = &self.completion else {
            return;
        };
        let root_slot = self.completion_root_slot.unwrap_or(slot);
        if restricted {
            recorder
                .borrow_mut()
                .record_restricted_expression_at_with_root(slot, root_slot);
        } else {
            recorder
                .borrow_mut()
                .record_expression_at_with_root(slot, root_slot);
        }
    }

    pub(super) fn parse(self) -> PResult<Node> {
        self.parse_complete(false)
    }

    pub(super) fn parse_b(self) -> PResult<Node> {
        self.parse_complete(true)
    }

    pub(super) fn parse_c(mut self) -> PResult<Node> {
        let location = self.location();
        let node = self.parse_c_expr().ok_or_else(|| {
            self.error
                .take()
                .unwrap_or_else(|| ParseError::new(location, "invalid common expression"))
        })?;
        if !self.at(TokenKind::Eof) {
            return Err(ParseError::ranged(
                self.peek().range,
                "unexpected token after common expression",
            ));
        }
        Ok(node)
    }

    pub(super) fn parse_complete(mut self, restricted: bool) -> PResult<Node> {
        let location = self.location();
        let node = self.parse_expr_mode(0, restricted).ok_or_else(|| {
            self.error
                .take()
                .unwrap_or_else(|| ParseError::new(location, "invalid or unsupported expression"))
        })?;
        if !self.at(TokenKind::Eof) {
            return Err(ParseError::ranged(
                self.peek().range,
                "unexpected token after expression",
            ));
        }
        Ok(node)
    }

    pub(super) fn parse_nested_select_range(
        &mut self,
        range: std::ops::Range<usize>,
    ) -> Option<Node> {
        let mut parser = self.parser_view(range);
        match parser.parse_statement(None) {
            Ok(node) if parser.at(TokenKind::Eof) && matches!(node, Node::SelectStmt(_)) => {
                Some(node)
            }
            Ok(_) => {
                if self.error.is_none() {
                    self.error = Some(parser.error_here("expected a SELECT statement"));
                }
                None
            }
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                None
            }
        }
    }

    pub(super) fn select_range_is_valid(&self, range: std::ops::Range<usize>) -> bool {
        let mut parser = self.parser_view_without_completion(range);
        matches!(parser.parse_statement(None), Ok(Node::SelectStmt(_))) && parser.at(TokenKind::Eof)
    }

    pub(super) fn parse_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, false)
    }

    pub(super) fn parse_expr_at(&mut self, slot: CompletionSlot, min_bp: u8) -> Option<Node> {
        let previous = self.completion_slot.replace(slot);
        let result = self.parse_expr_mode(min_bp, false);
        self.completion_slot = previous;
        result
    }

    pub(super) fn parse_b_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, true)
    }

    pub(super) fn parse_b_expr_at(&mut self, slot: CompletionSlot, min_bp: u8) -> Option<Node> {
        let previous = self.completion_slot.replace(slot);
        let result = self.parse_expr_mode(min_bp, true);
        self.completion_slot = previous;
        result
    }

    pub(super) fn parse_expr_mode(&mut self, min_bp: u8, restricted: bool) -> Option<Node> {
        let expression_start = self.location();
        let mut lhs = self.parse_prefix(restricted)?;
        let mut saw_is_predicate = false;
        let mut saw_comparison = false;
        let mut saw_special_predicate = false;

        loop {
            lhs = match self.peek_kind() {
                TokenKind::Char('[') => {
                    if 90 < min_bp {
                        break;
                    }
                    let index = self.parse_indirection_index()?;
                    append_indirection(lhs, index)
                }
                TokenKind::Char('.') => {
                    if 90 < min_bp {
                        break;
                    }
                    self.advance();
                    let item = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let name = self
                            .consume_identifier_in_categories(&[
                                KeywordCategory::Unreserved,
                                KeywordCategory::ColName,
                                KeywordCategory::TypeFuncName,
                                KeywordCategory::Reserved,
                            ])
                            .or_else(|| self.fail("expected a field name after '.'"))?;
                        make_string_node(name)
                    };
                    append_indirection(lhs, item)
                }
                TokenKind::TypeCast => {
                    if 80 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let type_name = Some(Box::new(self.parse_cast_type_name()?));
                    Node::TypeCast(TypeCast {
                        node_tag: NodeTag::TypeCast,
                        arg: Some(Box::new(lhs)),
                        type_name,
                        location: location as ParseLoc,
                    })
                }
                TokenKind::Collate => {
                    if restricted || 80 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let collname = self.parse_name_nodes()?;
                    Node::CollateClause(CollateClause {
                        node_tag: NodeTag::CollateClause,
                        arg: Some(Box::new(lhs)),
                        collname,
                        location: location as ParseLoc,
                    })
                }
                TokenKind::Isnull => {
                    if restricted || 70 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    Node::NullTest(NullTest {
                        xpr: Expr::new(NodeTag::NullTest),
                        arg: Some(Box::new(lhs)),
                        nulltesttype: NullTestType::Null,
                        location: location as ParseLoc,
                        ..NullTest::default()
                    })
                }
                TokenKind::Notnull => {
                    if restricted || 70 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    Node::NullTest(NullTest {
                        xpr: Expr::new(NodeTag::NullTest),
                        arg: Some(Box::new(lhs)),
                        nulltesttype: NullTestType::NotNull,
                        location: location as ParseLoc,
                        ..NullTest::default()
                    })
                }
                TokenKind::At => {
                    if restricted || 60 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let (args, call_location) = if self.consume(TokenKind::Time) {
                        self.expect(TokenKind::Zone)?;
                        let zone = self.parse_expr_mode(61, restricted)?;
                        (vec![zone, lhs], location as ParseLoc)
                    } else {
                        self.expect(TokenKind::Local)?;
                        (vec![lhs], -1)
                    };
                    Node::FuncCall(FuncCall {
                        node_tag: NodeTag::FuncCall,
                        funcname: system_type_names("timezone"),
                        args,
                        funcformat: CoercionForm::SqlSyntax,
                        location: call_location,
                        ..FuncCall::default()
                    })
                }
                TokenKind::Or => {
                    if restricted || 10 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let rhs = self.parse_expr_mode(11, restricted)?;
                    make_bool_expr(BoolExprType::OrExpr, lhs, rhs, location)
                }
                TokenKind::And => {
                    if restricted || 20 < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let rhs = self.parse_expr_mode(21, restricted)?;
                    make_bool_expr(BoolExprType::AndExpr, lhs, rhs, location)
                }
                TokenKind::Not
                    if matches!(
                        self.peek_kind_n(1),
                        TokenKind::InP
                            | TokenKind::Like
                            | TokenKind::Ilike
                            | TokenKind::Similar
                            | TokenKind::Between
                    ) =>
                {
                    if restricted || 35 < min_bp {
                        break;
                    }
                    if saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    saw_special_predicate = true;
                    let location = self.advance().location();
                    let op = self.advance().kind;
                    self.parse_special_infix(lhs, op, true, location)?
                }
                TokenKind::InP | TokenKind::Like | TokenKind::Ilike | TokenKind::Similar => {
                    if restricted || 35 < min_bp {
                        break;
                    }
                    if saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    saw_special_predicate = true;
                    let token = self.advance().clone();
                    self.parse_special_infix(lhs, token.kind, false, token.location())?
                }
                TokenKind::Between => {
                    if restricted || 35 < min_bp {
                        break;
                    }
                    if saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    saw_special_predicate = true;
                    let location = self.advance().location();
                    self.parse_between(lhs, false, location)?
                }
                TokenKind::Is => {
                    if 25 < min_bp {
                        break;
                    }
                    if saw_is_predicate {
                        return self.fail("cannot chain IS predicates");
                    }
                    saw_is_predicate = true;
                    let location = self.advance().location();
                    self.parse_is_expr(lhs, location, expression_start, restricted)?
                }
                TokenKind::Overlaps => {
                    if restricted || 35 < min_bp {
                        break;
                    }
                    if saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    saw_special_predicate = true;
                    let location = self.advance().location();
                    let Node::RowExpr(left_row) = lhs else {
                        return self.fail("left side of OVERLAPS must be a two-element row");
                    };
                    if left_row.args.len() != 2 {
                        return self.fail("left side of OVERLAPS must have two elements");
                    }
                    let rhs = self.parse_expr_mode(36, restricted)?;
                    let Node::RowExpr(right_row) = rhs else {
                        return self.fail("right side of OVERLAPS must be a two-element row");
                    };
                    if right_row.args.len() != 2 {
                        return self.fail("right side of OVERLAPS must have two elements");
                    }
                    let mut args = left_row.args;
                    args.extend(right_row.args);
                    Node::FuncCall(FuncCall {
                        node_tag: NodeTag::FuncCall,
                        funcname: system_type_names("overlaps"),
                        args,
                        funcformat: CoercionForm::SqlSyntax,
                        location: location as ParseLoc,
                        ..FuncCall::default()
                    })
                }
                kind if comparison_operator(kind).is_some() => {
                    if 30 < min_bp {
                        break;
                    }
                    if saw_comparison {
                        return self.fail("cannot chain comparison operators");
                    }
                    saw_comparison = true;
                    let token = self.advance().clone();
                    let operator = comparison_operator(token.kind).unwrap_or("=");
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, operator, token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(31, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec![operator],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                kind if additive_operator(kind).is_some() => {
                    if 45 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let operator = additive_operator(token.kind).unwrap_or("+");
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, operator, token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(46, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec![operator],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                kind if multiplicative_operator(kind).is_some() => {
                    if 50 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let operator = multiplicative_operator(token.kind).unwrap_or("*");
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, operator, token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(51, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec![operator],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                TokenKind::Char('^') => {
                    if 55 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, "^", token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(56, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec!["^"],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                TokenKind::RightArrow | TokenKind::Char('|') => {
                    if 40 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let operator = token_text(&token);
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, &operator, token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(41, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec![operator],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                TokenKind::Op => {
                    if 40 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let operator = token_name(&token).unwrap_or_else(|| token_text(&token));
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison(lhs, &operator, token.location())?
                    } else {
                        let rhs = self.parse_expr_mode(41, restricted)?;
                        make_aexpr(
                            AExprKind::Op,
                            vec![operator],
                            Some(lhs),
                            Some(rhs),
                            token.location(),
                        )
                    }
                }
                TokenKind::Operator => {
                    if 40 < min_bp {
                        break;
                    }
                    let location = self.location();
                    let name = self.parse_explicit_operator_name()?;
                    if !restricted && self.quantified_sub_link_type().is_some() {
                        self.parse_quantified_comparison_with_name(lhs, name, location)?
                    } else {
                        let rhs = self.parse_expr_mode(41, restricted)?;
                        make_aexpr_with_name(AExprKind::Op, name, Some(lhs), Some(rhs), location)
                    }
                }
                _ => break,
            };
        }

        Some(lhs)
    }
}
