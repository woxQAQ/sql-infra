use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_name_nodes(&mut self) -> Option<NodeList> {
        let mut fields = Vec::new();
        loop {
            if self.consume(TokenKind::Char('*')) {
                fields.push(Node::AStar(AStar {
                    node_tag: NodeTag::AStar,
                }));
            } else {
                let token = self.peek().clone();
                let categories: &[KeywordCategory] = if fields.is_empty() {
                    if self.peek_kind_n(1) == TokenKind::Char('(') {
                        &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName]
                    } else {
                        &[KeywordCategory::Unreserved, KeywordCategory::ColName]
                    }
                } else {
                    &[
                        KeywordCategory::Unreserved,
                        KeywordCategory::ColName,
                        KeywordCategory::TypeFuncName,
                        KeywordCategory::Reserved,
                    ]
                };
                let accepted = matches!(token.kind, TokenKind::Ident | TokenKind::UIdent)
                    || match &token.value {
                        Some(TokenValue::Keyword(word)) => lookup_keyword(word)
                            .is_some_and(|keyword| categories.contains(&keyword.category)),
                        _ => false,
                    };
                if !accepted {
                    if fields.is_empty() {
                        return None;
                    }
                    return self.fail("expected a name after '.'");
                }
                let name = token_name(&token)?;
                self.advance();
                fields.push(make_string_node(name));
            }
            if !self.consume(TokenKind::Char('.')) {
                break;
            }
            if matches!(fields.last(), Some(Node::AStar(_))) {
                return self.fail("'*' must be the last indirection element");
            }
        }
        Some(fields)
    }

    pub(super) fn parse_indirection_index(&mut self) -> Option<Node> {
        self.expect(TokenKind::Char('['))?;
        let (is_slice, lidx, uidx) = if self.consume(TokenKind::Char(':')) {
            let upper = if self.at(TokenKind::Char(']')) {
                None
            } else {
                Some(Box::new(self.parse_expr(0)?))
            };
            (true, None, upper)
        } else {
            if self.at(TokenKind::Char(']')) {
                return self.fail("array subscript cannot be empty");
            }
            let first = self.parse_expr(0)?;
            if self.consume(TokenKind::Char(':')) {
                let upper = if self.at(TokenKind::Char(']')) {
                    None
                } else {
                    Some(Box::new(self.parse_expr(0)?))
                };
                (true, Some(Box::new(first)), upper)
            } else {
                (false, None, Some(Box::new(first)))
            }
        };
        self.expect(TokenKind::Char(']'))?;
        Some(Node::AIndices(AIndices {
            node_tag: NodeTag::AIndices,
            is_slice,
            lidx,
            uidx,
        }))
    }

    pub(super) fn parse_parenthesized_expr(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Char('('))?.location();
        if self.starts_statement() {
            let range = self.take_until_balanced_range(TokenKind::Char(')'));
            let subselect = self.parse_nested_select_range(range)?;
            self.expect(TokenKind::Char(')'))?;
            return Some(Node::SubLink(SubLink {
                xpr: Expr::new(NodeTag::SubLink),
                sub_link_type: SubLinkType::ExprSublink,
                subselect: Some(Box::new(subselect)),
                location: location as ParseLoc,
                ..SubLink::default()
            }));
        }
        let args = self.parse_expr_list_until_at(
            CompletionSlot::ParenthesizedExpression,
            CompletionSlot::ParenthesizedExpressionAfterComma,
            TokenKind::Char(')'),
        )?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("parenthesized expression cannot be empty");
        }
        if args.len() == 1 {
            args.into_iter().next()
        } else {
            Some(Node::RowExpr(RowExpr {
                xpr: Expr::new(NodeTag::RowExpr),
                args,
                row_format: CoercionForm::ImplicitCast,
                location: location as ParseLoc,
                ..RowExpr::default()
            }))
        }
    }

    pub(super) fn parse_parenthesized_statement(&mut self) -> Option<Node> {
        if !self.consume(TokenKind::Char('(')) {
            return None;
        }
        let range = self.take_until_balanced_range(TokenKind::Char(')'));
        let subselect = self.parse_nested_select_range(range)?;
        self.expect(TokenKind::Char(')'))?;
        Some(subselect)
    }

    pub(super) fn parse_keyword_call_as_coalesce(&mut self) -> Option<Node> {
        let location = self.advance().location();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until_at(
            CompletionSlot::CoalesceArgument,
            CompletionSlot::CoalesceArgumentAfterComma,
            TokenKind::Char(')'),
        )?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("COALESCE requires at least one argument");
        }
        Some(Node::CoalesceExpr(CoalesceExpr {
            xpr: Expr::new(NodeTag::CoalesceExpr),
            args,
            location: location as ParseLoc,
            ..CoalesceExpr::default()
        }))
    }

    pub(super) fn parse_keyword_call_as_minmax(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until_at(
            CompletionSlot::MinmaxArgument,
            CompletionSlot::MinmaxArgumentAfterComma,
            TokenKind::Char(')'),
        )?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("GREATEST/LEAST requires at least one argument");
        }
        Some(Node::MinMaxExpr(MinMaxExpr {
            xpr: Expr::new(NodeTag::MinMaxExpr),
            op: if token.kind == TokenKind::Least {
                MinMaxOp::Least
            } else {
                MinMaxOp::Greatest
            },
            args,
            location: token.location() as ParseLoc,
            ..MinMaxExpr::default()
        }))
    }

    pub(super) fn parse_keyword_call_as_aexpr(&mut self, kind: AExprKind) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until_at(
            CompletionSlot::NullifArgument,
            CompletionSlot::NullifArgumentAfterComma,
            TokenKind::Char(')'),
        )?;
        self.expect(TokenKind::Char(')'))?;
        if args.len() != 2 {
            return self.fail("NULLIF requires exactly two arguments");
        }
        let mut iter = args.into_iter();
        let lhs = iter.next();
        let rhs = iter.next();
        Some(make_aexpr(kind, vec!["="], lhs, rhs, token.location()))
    }

    pub(super) fn parse_special_infix(
        &mut self,
        lhs: Node,
        op: TokenKind,
        negated: bool,
        location: usize,
    ) -> Option<Node> {
        match op {
            TokenKind::InP => {
                let list_start = self.expect(TokenKind::Char('('))?.location();
                if self.starts_statement() {
                    let range = self.take_until_balanced_range(TokenKind::Char(')'));
                    let subselect = self.parse_nested_select_range(range)?;
                    self.expect(TokenKind::Char(')'))?;
                    let sublink = Node::SubLink(SubLink {
                        xpr: Expr::new(NodeTag::SubLink),
                        sub_link_type: SubLinkType::AnySublink,
                        testexpr: Some(Box::new(lhs)),
                        subselect: Some(Box::new(subselect)),
                        location: location as ParseLoc,
                        ..SubLink::default()
                    });
                    Some(if negated {
                        make_not_expr(sublink, location)
                    } else {
                        sublink
                    })
                } else {
                    let elements = self.parse_expr_list_until_at(
                        CompletionSlot::InListExpression,
                        CompletionSlot::InListExpressionAfterComma,
                        TokenKind::Char(')'),
                    )?;
                    if elements.is_empty() {
                        return self.fail("IN requires at least one expression");
                    }
                    let list_end = self.expect(TokenKind::Char(')'))?.location();
                    let mut expression = make_aexpr(
                        AExprKind::In,
                        vec![if negated { "<>" } else { "=" }],
                        Some(lhs),
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements,
                            location: location as ParseLoc,
                            ..AArrayExpr::default()
                        })),
                        location,
                    );
                    if let Node::AExpr(aexpr) = &mut expression {
                        aexpr.rexpr_list_start = list_start as ParseLoc;
                        aexpr.rexpr_list_end = list_end as ParseLoc;
                    }
                    Some(expression)
                }
            }
            TokenKind::Like | TokenKind::Ilike | TokenKind::Similar => {
                if op != TokenKind::Similar && self.quantified_sub_link_type().is_some() {
                    let operator = match (op, negated) {
                        (TokenKind::Like, false) => "~~",
                        (TokenKind::Like, true) => "!~~",
                        (TokenKind::Ilike, false) => "~~*",
                        (TokenKind::Ilike, true) => "!~~*",
                        _ => return None,
                    };
                    return self.parse_quantified_comparison(lhs, operator, location);
                }
                if op == TokenKind::Similar {
                    self.expect(TokenKind::To)?;
                }
                let rhs = self.parse_expr(36)?;
                let escape = if self.consume(TokenKind::Escape) {
                    Some(self.parse_expr(36)?)
                } else {
                    None
                };
                let kind = match op {
                    TokenKind::Ilike => AExprKind::Ilike,
                    TokenKind::Similar => AExprKind::Similar,
                    _ => AExprKind::Like,
                };
                let operator = match (op, negated) {
                    (TokenKind::Like, false) => "~~",
                    (TokenKind::Like, true) => "!~~",
                    (TokenKind::Ilike, false) => "~~*",
                    (TokenKind::Ilike, true) => "!~~*",
                    (TokenKind::Similar, false) => "~",
                    (TokenKind::Similar, true) => "!~",
                    _ => return None,
                };
                let rhs = if op == TokenKind::Similar || escape.is_some() {
                    let mut args = vec![rhs];
                    if let Some(escape) = escape {
                        args.push(escape);
                    }
                    Node::FuncCall(FuncCall {
                        node_tag: NodeTag::FuncCall,
                        funcname: system_type_names(if op == TokenKind::Similar {
                            "similar_to_escape"
                        } else {
                            "like_escape"
                        }),
                        args,
                        location: location as ParseLoc,
                        ..FuncCall::default()
                    })
                } else {
                    rhs
                };
                Some(make_aexpr(
                    kind,
                    vec![operator],
                    Some(lhs),
                    Some(rhs),
                    location,
                ))
            }
            TokenKind::Between => self.parse_between(lhs, negated, location),
            _ => None,
        }
    }

    pub(super) fn quantified_sub_link_type(&self) -> Option<SubLinkType> {
        match self.peek_kind() {
            TokenKind::Any | TokenKind::Some => Some(SubLinkType::AnySublink),
            TokenKind::All => Some(SubLinkType::AllSublink),
            _ => None,
        }
    }

    pub(super) fn parse_explicit_operator_name(&mut self) -> Option<NodeList> {
        let location = self.expect(TokenKind::Operator)?.location();
        self.expect(TokenKind::Char('('))?;
        let tokens = self.take_until_balanced(TokenKind::Char(')'));
        self.expect(TokenKind::Char(')'))?;
        match parse_operator_name_tokens(tokens, location) {
            Ok(name) => Some(name),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                None
            }
        }
    }

    pub(super) fn parse_quantified_comparison(
        &mut self,
        lhs: Node,
        operator: &str,
        location: usize,
    ) -> Option<Node> {
        self.parse_quantified_comparison_with_name(lhs, vec![make_string_node(operator)], location)
    }

    pub(super) fn parse_quantified_comparison_with_name(
        &mut self,
        lhs: Node,
        operator_name: NodeList,
        location: usize,
    ) -> Option<Node> {
        let sub_link_type = self.quantified_sub_link_type()?;
        self.advance();
        self.expect(TokenKind::Char('('))?;
        if self.starts_statement() {
            let range = self.take_until_balanced_range(TokenKind::Char(')'));
            let subselect = self.parse_nested_select_range(range)?;
            self.expect(TokenKind::Char(')'))?;
            Some(Node::SubLink(SubLink {
                xpr: Expr::new(NodeTag::SubLink),
                sub_link_type,
                testexpr: Some(Box::new(lhs)),
                oper_name: operator_name,
                subselect: Some(Box::new(subselect)),
                location: location as ParseLoc,
                ..SubLink::default()
            }))
        } else {
            let rhs = self.parse_expr(0)?;
            self.expect(TokenKind::Char(')'))?;
            Some(make_aexpr_with_name(
                if sub_link_type == SubLinkType::AllSublink {
                    AExprKind::OpAll
                } else {
                    AExprKind::OpAny
                },
                operator_name,
                Some(lhs),
                Some(rhs),
                location,
            ))
        }
    }

    pub(super) fn parse_between(
        &mut self,
        lhs: Node,
        negated: bool,
        location: usize,
    ) -> Option<Node> {
        let symmetric = self.consume(TokenKind::Symmetric);
        self.consume(TokenKind::Asymmetric);
        let lower = self.parse_b_expr(0)?;
        self.expect(TokenKind::And)?;
        let upper = self.parse_expr(36)?;
        let kind = match (negated, symmetric) {
            (true, true) => AExprKind::NotBetweenSym,
            (true, false) => AExprKind::NotBetween,
            (false, true) => AExprKind::BetweenSym,
            (false, false) => AExprKind::Between,
        };
        Some(make_aexpr(
            kind,
            vec![match (negated, symmetric) {
                (true, true) => "NOT BETWEEN SYMMETRIC",
                (true, false) => "NOT BETWEEN",
                (false, true) => "BETWEEN SYMMETRIC",
                (false, false) => "BETWEEN",
            }],
            Some(lhs),
            Some(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements: vec![lower, upper],
                location: location as ParseLoc,
                ..AArrayExpr::default()
            })),
            location,
        ))
    }

    pub(super) fn parse_is_expr(
        &mut self,
        lhs: Node,
        location: usize,
        expression_start: usize,
        restricted: bool,
    ) -> Option<Node> {
        let negated = self.consume(TokenKind::Not);
        if self.consume(TokenKind::DocumentP) {
            let document = Node::XmlExpr(XmlExpr {
                xpr: Expr::new(NodeTag::XmlExpr),
                op: XmlExprOp::Document,
                args: vec![lhs],
                location: location as ParseLoc,
                ..XmlExpr::default()
            });
            return Some(if negated {
                make_not_expr(document, location)
            } else {
                document
            });
        }
        if restricted && !self.at(TokenKind::Distinct) {
            return self.fail("this IS predicate is not allowed in a restricted expression");
        }
        let normalization_form = match self.peek_kind() {
            TokenKind::Nfc => Some("NFC"),
            TokenKind::Nfd => Some("NFD"),
            TokenKind::Nfkc => Some("NFKC"),
            TokenKind::Nfkd => Some("NFKD"),
            _ => None,
        };
        if !restricted && (self.at(TokenKind::Normalized) || normalization_form.is_some()) {
            let mut args = vec![lhs];
            if let Some(form) = normalization_form {
                let form_location = self.advance().location();
                args.push(Node::AConst(AConst::string(
                    form,
                    form_location as ParseLoc,
                )));
            }
            self.expect(TokenKind::Normalized)?;
            let normalized = Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: system_type_names("is_normalized"),
                args,
                funcformat: CoercionForm::SqlSyntax,
                location: location as ParseLoc,
                ..FuncCall::default()
            });
            return Some(if negated {
                make_not_expr(normalized, location)
            } else {
                normalized
            });
        }
        if !restricted && self.consume(TokenKind::Json) {
            let item_type = match self.peek_kind() {
                TokenKind::ValueP => JsonValueType::Any,
                TokenKind::Array => JsonValueType::Array,
                TokenKind::ObjectP => JsonValueType::Object,
                TokenKind::Scalar => JsonValueType::Scalar,
                _ => JsonValueType::Any,
            };
            if matches!(
                self.peek_kind(),
                TokenKind::ValueP | TokenKind::Array | TokenKind::ObjectP | TokenKind::Scalar
            ) {
                self.advance();
            }
            let unique_keys = if self.consume(TokenKind::With) {
                self.expect(TokenKind::Unique)?;
                self.consume(TokenKind::Keys);
                true
            } else if self.consume(TokenKind::Without) {
                self.expect(TokenKind::Unique)?;
                self.consume(TokenKind::Keys);
                false
            } else {
                false
            };
            let predicate = Node::JsonIsPredicate(JsonIsPredicate {
                node_tag: NodeTag::JsonIsPredicate,
                expr: Some(Box::new(lhs)),
                format: Some(Box::new(default_json_format())),
                item_type,
                unique_keys,
                location: expression_start as ParseLoc,
                ..JsonIsPredicate::default()
            });
            return Some(if negated {
                make_not_expr(predicate, expression_start)
            } else {
                predicate
            });
        }
        if self.consume(TokenKind::Distinct) {
            self.expect(TokenKind::From)?;
            let rhs = self.parse_expr_mode(26, restricted)?;
            return Some(make_aexpr(
                if negated {
                    AExprKind::NotDistinct
                } else {
                    AExprKind::Distinct
                },
                vec!["="],
                Some(lhs),
                Some(rhs),
                location,
            ));
        }
        if self.consume(TokenKind::NullP) {
            return Some(Node::NullTest(NullTest {
                xpr: Expr::new(NodeTag::NullTest),
                arg: Some(Box::new(lhs)),
                nulltesttype: if negated {
                    NullTestType::NotNull
                } else {
                    NullTestType::Null
                },
                location: location as ParseLoc,
                ..NullTest::default()
            }));
        }
        let booltesttype = match self.peek_kind() {
            TokenKind::TrueP => Some(if negated {
                BoolTestType::NotTrue
            } else {
                BoolTestType::True
            }),
            TokenKind::FalseP => Some(if negated {
                BoolTestType::NotFalse
            } else {
                BoolTestType::False
            }),
            TokenKind::Unknown => Some(if negated {
                BoolTestType::NotUnknown
            } else {
                BoolTestType::Unknown
            }),
            _ => None,
        };
        if let Some(booltesttype) = booltesttype {
            self.advance();
            return Some(Node::BooleanTest(BooleanTest {
                xpr: Expr::new(NodeTag::BooleanTest),
                arg: Some(Box::new(lhs)),
                booltesttype,
                location: location as ParseLoc,
            }));
        }
        self.fail("expected a valid IS predicate")
    }

    pub(super) fn parse_cast_type_name(&mut self) -> Option<TypeName> {
        let start = self.pos;
        let mut depth = 0usize;
        let mut best = None;
        for end in start + 1..=self.end {
            let token = &self.tokens[end - 1];
            if end - 1 > start
                && depth == 0
                && (expression_boundary(token.kind)
                    || matches!(
                        token.kind,
                        TokenKind::TypeCast
                            | TokenKind::Collate
                            | TokenKind::Isnull
                            | TokenKind::Notnull
                    ))
            {
                break;
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
                && let Some(type_name) = tokens_to_type_name(self.tokens[start..end].to_vec())
            {
                best = Some((end, type_name));
            }
        }
        let Some((end, type_name)) = best else {
            return self.fail("expected a type name after '::'");
        };
        self.pos = end;
        Some(type_name)
    }

    pub(super) fn parse_expr_list_until_at(
        &mut self,
        first_slot: CompletionSlot,
        continuation_slot: CompletionSlot,
        stop: TokenKind,
    ) -> Option<NodeList> {
        if self.at_completion_cursor()
            && let Some(recorder) = &self.completion
        {
            recorder.borrow_mut().record_expression_at(first_slot);
        }
        let mut items = Vec::new();
        while !self.at(stop) && !self.at(TokenKind::Eof) {
            let slot = if items.is_empty() {
                first_slot
            } else {
                continuation_slot
            };
            items.push(self.parse_expr_at(slot, 0)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(stop) || self.at(TokenKind::Eof) {
                if self.at_completion_cursor()
                    && let Some(recorder) = &self.completion
                {
                    recorder
                        .borrow_mut()
                        .record_expression_at(continuation_slot);
                    return self.stop_for_completion();
                }
                if self.error.is_none() {
                    self.error = Some(ParseError::ranged(
                        self.peek().range,
                        "expected an expression after ','",
                    ));
                }
                return None;
            }
        }
        Some(items)
    }

    pub(super) fn fail<T>(&mut self, message: impl Into<std::string::String>) -> Option<T> {
        if self.error.is_none() {
            self.error = Some(ParseError::ranged(self.peek().range, message));
        }
        None
    }

    pub(super) fn stop_for_completion<T>(&mut self) -> Option<T> {
        debug_assert!(self.at_completion_cursor());
        if self.error.is_none() {
            self.error = Some(ParseError::completion(self.peek().range));
        }
        None
    }

    pub(super) fn take_until_balanced(&mut self, stop: TokenKind) -> Vec<Token> {
        let range = self.take_until_balanced_range(stop);
        self.tokens[range].to_vec()
    }

    pub(super) fn take_until_balanced_range(&mut self, stop: TokenKind) -> std::ops::Range<usize> {
        let start = self.pos;
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            if depth == 0 && kind == stop {
                break;
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.advance();
        }
        start..self.pos
    }

    pub(super) fn starts_statement(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::With
        )
    }

    pub(super) fn identifier_in_categories(&self, categories: &[KeywordCategory]) -> bool {
        let token = self.peek();
        matches!(token.kind, TokenKind::Ident | TokenKind::UIdent)
            || match &token.value {
                Some(TokenValue::Keyword(word)) => lookup_keyword(word)
                    .is_some_and(|keyword| categories.contains(&keyword.category)),
                _ => false,
            }
    }

    pub(super) fn consume_identifier_in_categories(
        &mut self,
        categories: &[KeywordCategory],
    ) -> Option<std::string::String> {
        if !self.identifier_in_categories(categories) {
            return None;
        }
        let token = self.advance().clone();
        token_name(&token)
    }

    pub(super) fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    pub(super) fn at_completion_cursor(&self) -> bool {
        self.at(TokenKind::Eof)
            && self
                .completion
                .as_ref()
                .is_some_and(|recorder| recorder.borrow().is_cursor(self.location()))
    }

    pub(super) fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at_completion_cursor()
            && let Some(recorder) = &self.completion
        {
            recorder
                .borrow_mut()
                .record_at(self.active_completion_slot(), Expectation::Token(kind));
            return false;
        }
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at_completion_cursor()
            && let Some(recorder) = &self.completion
        {
            recorder
                .borrow_mut()
                .record_at(self.active_completion_slot(), Expectation::Token(kind));
        }
        if self.at(kind) {
            Some(self.advance().clone())
        } else {
            None
        }
    }

    pub(super) fn advance(&mut self) -> &Token {
        if self.pos < self.end {
            let consumed = self.pos;
            self.pos += 1;
            &self.tokens[consumed]
        } else {
            &self.eof
        }
    }

    pub(super) fn peek(&self) -> &Token {
        if self.pos < self.end {
            &self.tokens[self.pos]
        } else {
            &self.eof
        }
    }

    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        let index = self.pos.saturating_add(n);
        if index < self.end {
            self.tokens[index].kind
        } else {
            TokenKind::Eof
        }
    }

    pub(super) fn location(&self) -> usize {
        self.peek().location()
    }

    pub(super) fn previous_location(&self) -> usize {
        if self.pos > self.start {
            self.tokens[self.pos - 1].location()
        } else {
            self.location()
        }
    }
}
