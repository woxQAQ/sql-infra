//! Postfix, predicate, and token-cursor operations for [`ExprParser`].
//!
//! Indirection, casts, quantified comparisons, `BETWEEN`, `IS`, list parsing,
//! and expression-level completion recording extend already parsed prefixes.

use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_name_nodes(&mut self) -> Option<NodeList> {
        self.parse_name_nodes_with_slots(
            &[
                completion::GrammarSlot::Column,
                completion::GrammarSlot::Function,
            ],
            true,
        )
    }

    pub(super) fn parse_name_nodes_with_slots(
        &mut self,
        slots: &[completion::GrammarSlot],
        allow_star: bool,
    ) -> Option<NodeList> {
        let mut fields = Vec::new();
        loop {
            if self.at_completion() {
                for slot in slots {
                    self.record_completion_slot(*slot);
                }
                if allow_star {
                    self.record_completion_tokens(&[TokenKind::Char('*')]);
                }
                let Some(hole) = self.recover_completion_hole() else {
                    return self.fail("completion point in a qualified name");
                };
                fields.push(make_string_node(token_name(&hole)?));
            } else if allow_star && !fields.is_empty() && self.consume(TokenKind::Char('*')) {
                fields.push(Node::AStar);
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
            if matches!(fields.last(), Some(Node::AStar)) {
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
        Some(node!(AIndices {
            is_slice,
            lidx,
            uidx,
        }))
    }

    pub(super) fn parse_parenthesized_expr(&mut self) -> Option<(Node, bool)> {
        let location = self.expect(TokenKind::Char('('))?.location();
        self.record_completion_expression_start_tokens(completion::SUBQUERY_START_TOKENS);
        if let Some(tokens) = self.take_parenthesized_select_tokens() {
            let subselect = self.parse_nested_select(tokens)?;
            self.expect(TokenKind::Char(')'))?;
            return Some((
                node!(SubLink {
                    sub_link_type: SubLinkType::ExprSublink,
                    subselect: Some(Box::new(subselect)),
                    location: location as ParseLoc,
                    ..SubLink::default()
                }),
                false,
            ));
        }
        let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("parenthesized expression cannot be empty");
        }
        if args.len() == 1 {
            args.into_iter().next().map(|node| (node, false))
        } else {
            Some((
                node!(RowExpr {
                    args,
                    row_format: CoercionForm::ImplicitCast,
                    location: location as ParseLoc,
                    ..RowExpr::default()
                }),
                true,
            ))
        }
    }

    pub(super) fn parse_parenthesized_statement(&mut self) -> Option<Node> {
        if !self.consume(TokenKind::Char('(')) {
            return None;
        }
        self.record_completion_tokens(completion::SUBQUERY_START_TOKENS);
        let tokens = self.take_until_balanced(TokenKind::Char(')'));
        let subselect = self.parse_nested_select(tokens)?;
        self.expect(TokenKind::Char(')'))?;
        Some(subselect)
    }

    pub(super) fn parse_keyword_call_as_coalesce(&mut self) -> Option<Node> {
        let location = self.advance().location();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("COALESCE requires at least one argument");
        }
        Some(node!(CoalesceExpr {
            args,
            location: location as ParseLoc,
            ..CoalesceExpr::default()
        }))
    }

    pub(super) fn parse_keyword_call_as_minmax(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("GREATEST/LEAST requires at least one argument");
        }
        Some(node!(MinMaxExpr {
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
        let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
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
                self.record_completion_expression_start_tokens(completion::SUBQUERY_START_TOKENS);
                if let Some(tokens) = self.take_parenthesized_select_tokens() {
                    let subselect = self.parse_nested_select(tokens)?;
                    self.expect(TokenKind::Char(')'))?;
                    let sublink = node!(SubLink {
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
                    let elements = self.parse_expr_list_until(TokenKind::Char(')'))?;
                    if elements.is_empty() {
                        return self.fail("IN requires at least one expression");
                    }
                    let list_end = self.expect(TokenKind::Char(')'))?.location();
                    let mut expression = make_aexpr(
                        AExprKind::In,
                        vec![if negated { "<>" } else { "=" }],
                        Some(lhs),
                        Some(node!(AArrayExpr {
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
                    node!(FuncCall {
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
        self.record_completion_expression_continuation_tokens(&[
            TokenKind::Any,
            TokenKind::Some,
            TokenKind::All,
        ]);
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
        if self.at_completion() {
            self.record_completion_slot(completion::GrammarSlot::Operator);
        }
        self.expect(TokenKind::Char(')'))?;
        match parse_operator_name_tokens(tokens, location) {
            Ok(name) => Some(name),
            Err(error) => self.fail_with(error),
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
        self.record_completion_expression_start_tokens(completion::SUBQUERY_START_TOKENS);
        if let Some(tokens) = self.take_parenthesized_select_tokens() {
            let subselect = self.parse_nested_select(tokens)?;
            self.expect(TokenKind::Char(')'))?;
            Some(node!(SubLink {
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
            Some(node!(AArrayExpr {
                elements: vec![lower, upper],
                location: location as ParseLoc,
                ..AArrayExpr::default()
            })),
            location,
        ))
    }

    pub(super) fn parse_overlaps(&mut self, lhs: Node, location: usize) -> Option<Node> {
        let Node::RowExpr(left_row) = lhs else {
            return self.fail("left side of OVERLAPS must be a two-element row");
        };
        if left_row.args.len() != 2 {
            return self.fail("left side of OVERLAPS must have two elements");
        }

        let right = self.parse_prefix(false)?;
        if !right.is_row_syntax {
            return self.fail("right side of OVERLAPS must be a row expression");
        }
        let Node::RowExpr(right_row) = right.node else {
            return self.fail("right side of OVERLAPS must be a row expression");
        };
        if right_row.args.len() != 2 {
            return self.fail("right side of OVERLAPS must have two elements");
        }

        let mut args = left_row.args;
        args.extend(right_row.args);
        Some(node!(FuncCall {
            funcname: system_type_names("overlaps"),
            args,
            funcformat: CoercionForm::SqlSyntax,
            location: location as ParseLoc,
            ..FuncCall::default()
        }))
    }

    pub(super) fn parse_is_expr(
        &mut self,
        lhs: Node,
        location: usize,
        expression_start: usize,
        restricted: bool,
    ) -> Option<Node> {
        let negated = self.consume(TokenKind::Not);
        self.record_completion_tokens(&[TokenKind::DocumentP, TokenKind::Distinct]);
        if !restricted {
            self.record_completion_tokens(&[
                TokenKind::Json,
                TokenKind::Normalized,
                TokenKind::Nfc,
                TokenKind::Nfd,
                TokenKind::Nfkc,
                TokenKind::Nfkd,
                TokenKind::NullP,
                TokenKind::TrueP,
                TokenKind::FalseP,
                TokenKind::Unknown,
            ]);
        }
        if self.consume(TokenKind::DocumentP) {
            let document = node!(XmlExpr {
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
                args.push(node!(AConst::string(form, form_location as ParseLoc,)));
            }
            self.expect(TokenKind::Normalized)?;
            let normalized = node!(FuncCall {
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
            self.record_completion_tokens(&[
                TokenKind::ValueP,
                TokenKind::Array,
                TokenKind::ObjectP,
                TokenKind::Scalar,
                TokenKind::With,
                TokenKind::Without,
            ]);
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
            let predicate = node!(JsonIsPredicate {
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
            return Some(node!(NullTest {
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
            return Some(node!(BooleanTest {
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
        for end in start + 1..self.tokens.len() {
            let token = &self.tokens[end - 1];
            if token.kind == TokenKind::Completion {
                record_type_name_completion(&self.tokens[start..end], self.completion.as_ref());
            }
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

    pub(super) fn parse_expr_list_until(&mut self, stop: TokenKind) -> Option<NodeList> {
        let mut items = Vec::new();
        while !self.at(stop) && !self.at(TokenKind::Eof) {
            items.push(self.parse_expr(0)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(stop) || self.at(TokenKind::Eof) {
                return self.fail_with(ParseError::ranged(
                    self.peek().range,
                    "expected an expression after ','",
                ));
            }
        }
        Some(items)
    }

    pub(super) fn fail<T>(&mut self, message: impl Into<std::string::String>) -> Option<T> {
        if self.error.is_some() {
            return None;
        }
        let error = self.error_here(message);
        self.fail_with(error)
    }

    pub(super) fn fail_with<T>(&mut self, error: ParserExit) -> Option<T> {
        if self.error.is_none() {
            self.error = Some(error);
        }
        None
    }

    pub(super) fn take_until_balanced(&mut self, stop: TokenKind) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            if self.at_completion() {
                if let Some(hole) = self.recover_completion_hole() {
                    tokens.push(hole);
                    continue;
                }
                break;
            }
            let kind = self.peek_kind();
            if depth == 0 && kind == stop {
                break;
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            tokens.push(self.advance().clone());
        }
        tokens
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
        if self.at_completion() {
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            return self
                .recover_completion_hole()
                .and_then(|token| token_name(&token));
        }
        if !self.identifier_in_categories(categories) {
            return None;
        }
        let token = self.advance().clone();
        token_name(&token)
    }

    pub(super) fn consume_column_label(&mut self) -> Option<std::string::String> {
        self.consume_identifier_in_categories(&[
            KeywordCategory::Unreserved,
            KeywordCategory::ColName,
            KeywordCategory::TypeFuncName,
            KeywordCategory::Reserved,
        ])
    }

    pub(super) fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    pub(super) fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at_completion() {
            self.record_completion_tokens(&[kind]);
            return false;
        }
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Optional match of a fixed multi-token unit: if the head token matches,
    /// every following token is required. Publishes the whole phrase as one
    /// completion unit.
    pub(super) fn consume_phrase(&mut self, phrase: &'static [TokenKind]) -> Option<bool> {
        self.record_completion_phrase(phrase);
        if !self.consume(phrase[0]) {
            return Some(false);
        }
        for kind in &phrase[1..] {
            self.expect(*kind)?;
        }
        Some(true)
    }

    pub(super) fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at_completion() {
            self.record_completion_tokens(&[kind]);
            let error = self.error_here(format!("expected {kind:?}"));
            return self.fail_with(error);
        }
        if self.at(kind) {
            Some(self.advance().clone())
        } else {
            None
        }
    }

    pub(super) fn advance(&mut self) -> &Token {
        if !matches!(self.peek_kind(), TokenKind::Eof | TokenKind::Completion) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    pub(super) fn location(&self) -> usize {
        self.peek().location()
    }

    pub(super) fn previous_location(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.location())
            .unwrap_or_else(|| self.location())
    }

    pub(super) fn at_completion(&self) -> bool {
        self.at(TokenKind::Completion)
    }

    pub(super) fn recover_completion_hole(&mut self) -> Option<Token> {
        if !self.at_completion() {
            return None;
        }
        let recovered = self
            .completion
            .as_ref()
            .is_some_and(|collector| collector.borrow_mut().try_recover_hole());
        if !recovered {
            return None;
        }
        let location = self.peek().location();
        self.pos += 1;
        Some(Token::completion_hole(location))
    }

    pub(super) fn record_completion_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_tokens(kinds);
        }
    }

    pub(super) fn record_completion_lookahead_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_lookahead_tokens(kinds);
        }
    }

    pub(super) fn record_completion_expression_start_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_expression_start_tokens(kinds);
        }
    }

    pub(super) fn record_completion_expression_continuation_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector
                .borrow_mut()
                .record_expression_continuation_tokens(kinds);
        }
    }

    pub(super) fn record_completion_expression_continuation_phrase(
        &self,
        phrase: &'static [TokenKind],
    ) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector
                .borrow_mut()
                .record_expression_continuation_phrase(phrase);
        }
    }

    pub(super) fn record_completion_slot(&self, slot: completion::GrammarSlot) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_slot(slot);
        }
    }

    pub(super) fn record_completion_phrase(&self, phrase: &'static [TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_phrase(phrase);
        }
    }

    pub(super) fn error_here(&self, message: impl Into<std::string::String>) -> ParserExit {
        if self.at_completion() && self.completion.is_some() {
            ParserExit::completion(self.peek().range)
        } else {
            ParseError::ranged(self.peek().range, message)
        }
    }
}
