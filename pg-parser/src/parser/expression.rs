use super::*;

pub(super) struct ExprParser {
    pub(super) tokens: Vec<Token>,
    pub(super) pos: usize,
    pub(super) error: Option<ParseError>,
    pub(super) completion: Option<SharedCompletionRecorder>,
}

impl ExprParser {
    pub(super) fn new(mut tokens: Vec<Token>) -> Self {
        let location = tokens.last().map_or(0, Token::end_location);
        tokens.push(Token::synthetic(TokenKind::Eof, location));
        Self {
            tokens,
            pos: 0,
            error: None,
            completion: None,
        }
    }

    pub(super) fn new_completion(
        mut tokens: Vec<Token>,
        recorder: SharedCompletionRecorder,
    ) -> Self {
        let location = tokens.last().map_or(0, Token::end_location);
        tokens.push(Token::synthetic(TokenKind::Eof, location));
        Self {
            tokens,
            pos: 0,
            error: None,
            completion: Some(recorder),
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

    pub(super) fn parse_nested_select(&mut self, tokens: Vec<Token>) -> Option<Node> {
        match parse_select_statement_tokens(tokens) {
            Ok(node) => Some(node),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                None
            }
        }
    }

    pub(super) fn parse_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, false)
    }

    pub(super) fn parse_b_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, true)
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
