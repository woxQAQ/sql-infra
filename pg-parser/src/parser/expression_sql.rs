//! Expressions written with dedicated SQL syntax rather than ordinary calls.
//!
//! `CASE`, casts, extraction, normalization, trimming, overlays, substrings,
//! grouping, XML existence, and SQL value functions are normalized here.

use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_case_expr(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Case)?.location();
        self.record_completion_tokens(&[TokenKind::When]);
        let arg = if self.at(TokenKind::When) {
            None
        } else {
            Some(Box::new(self.parse_expr(0)?))
        };
        let mut args = Vec::new();
        while self.consume(TokenKind::When) {
            let when_location = self.previous_location();
            let expr = self.parse_expr(0)?;
            self.expect(TokenKind::Then)?;
            let result = self.parse_expr(0)?;
            args.push(Node::CaseWhen(CaseWhen {
                xpr: Expr::new(NodeTag::CaseWhen),
                expr: Some(Box::new(expr)),
                result: Some(Box::new(result)),
                location: when_location as ParseLoc,
            }));
        }
        if args.is_empty() {
            return None;
        }
        let defresult = if self.consume(TokenKind::Else) {
            Some(Box::new(self.parse_expr(0)?))
        } else {
            None
        };
        self.expect(TokenKind::EndP)?;
        Some(Node::CaseExpr(CaseExpr {
            xpr: Expr::new(NodeTag::CaseExpr),
            arg,
            args,
            defresult,
            location: location as ParseLoc,
            ..CaseExpr::default()
        }))
    }

    pub(super) fn parse_grouping_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Grouping)?.location();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
        self.expect(TokenKind::Char(')'))?;
        if args.is_empty() {
            return self.fail("GROUPING requires at least one argument");
        }
        Some(Node::GroupingFunc(GroupingFunc {
            xpr: Expr::new(NodeTag::GroupingFunc),
            args,
            location: location as ParseLoc,
            ..GroupingFunc::default()
        }))
    }

    pub(super) fn parse_cast_or_treat(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let arg = self.parse_expr(0)?;
        self.expect(TokenKind::As)?;
        let type_tokens = self.take_until_balanced(TokenKind::Char(')'));
        if self.at_completion() {
            let mut completion_tokens = type_tokens.clone();
            completion_tokens.push(self.peek().clone());
            record_type_name_completion(&completion_tokens, self.completion.as_ref());
        }
        let type_name = parse_type_name_tokens(type_tokens).ok()?;
        self.expect(TokenKind::Char(')'))?;
        if token.kind == TokenKind::Cast {
            Some(Node::TypeCast(TypeCast {
                node_tag: NodeTag::TypeCast,
                arg: Some(Box::new(arg)),
                type_name: Some(Box::new(type_name)),
                location: token.location() as ParseLoc,
            }))
        } else {
            let function_name = type_name.names.last().and_then(|name| match name {
                Node::String(value) => value.sval.clone(),
                _ => None,
            })?;
            Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: system_type_names(&function_name),
                args: vec![arg],
                location: token.location() as ParseLoc,
                ..FuncCall::default()
            }))
        }
    }

    pub(super) fn parse_extract_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Extract)?.location();
        self.expect(TokenKind::Char('('))?;
        self.record_completion_tokens(&[
            TokenKind::YearP,
            TokenKind::MonthP,
            TokenKind::DayP,
            TokenKind::HourP,
            TokenKind::MinuteP,
            TokenKind::SecondP,
        ]);
        self.record_completion_slot(completion::GrammarSlot::AnyName);
        let field_token = self.peek().clone();
        if !matches!(
            field_token.kind,
            TokenKind::Ident
                | TokenKind::UIdent
                | TokenKind::SConst
                | TokenKind::YearP
                | TokenKind::MonthP
                | TokenKind::DayP
                | TokenKind::HourP
                | TokenKind::MinuteP
                | TokenKind::SecondP
        ) {
            return self.fail("invalid EXTRACT field");
        }
        let field = token_name(&field_token)?;
        self.advance();
        self.expect(TokenKind::From)?;
        let arg = self.parse_expr(0)?;
        self.expect(TokenKind::Char(')'))?;
        Some(Node::FuncCall(FuncCall {
            node_tag: NodeTag::FuncCall,
            funcname: system_type_names("extract"),
            args: vec![
                Node::AConst(AConst::string(field, field_token.location() as ParseLoc)),
                arg,
            ],
            funcformat: CoercionForm::SqlSyntax,
            location: location as ParseLoc,
            ..FuncCall::default()
        }))
    }

    pub(super) fn parse_normalize_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Normalize)?.location();
        self.expect(TokenKind::Char('('))?;
        let mut args = vec![self.parse_expr(0)?];
        if self.consume(TokenKind::Char(',')) {
            self.record_completion_tokens(&[
                TokenKind::Nfc,
                TokenKind::Nfd,
                TokenKind::Nfkc,
                TokenKind::Nfkd,
            ]);
            let form_token = self.advance().clone();
            let form = match form_token.kind {
                TokenKind::Nfc => "NFC",
                TokenKind::Nfd => "NFD",
                TokenKind::Nfkc => "NFKC",
                TokenKind::Nfkd => "NFKD",
                _ => return self.fail("NORMALIZE requires NFC, NFD, NFKC, or NFKD"),
            };
            args.push(Node::AConst(AConst::string(
                form,
                form_token.location() as ParseLoc,
            )));
        }
        self.expect(TokenKind::Char(')'))?;
        Some(Node::FuncCall(FuncCall {
            node_tag: NodeTag::FuncCall,
            funcname: system_type_names("normalize"),
            args,
            funcformat: CoercionForm::SqlSyntax,
            location: location as ParseLoc,
            ..FuncCall::default()
        }))
    }

    pub(super) fn make_sql_syntax_call(&self, name: &str, args: NodeList, location: usize) -> Node {
        Node::FuncCall(FuncCall {
            node_tag: NodeTag::FuncCall,
            funcname: system_type_names(name),
            args,
            funcformat: CoercionForm::SqlSyntax,
            location: location as ParseLoc,
            ..FuncCall::default()
        })
    }

    pub(super) fn parse_position_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Position)?.location();
        self.expect(TokenKind::Char('('))?;
        let needle = self.parse_b_expr(0)?;
        self.expect(TokenKind::InP)?;
        let haystack = self.parse_b_expr(0)?;
        self.expect(TokenKind::Char(')'))?;
        Some(self.make_sql_syntax_call("position", vec![haystack, needle], location))
    }

    pub(super) fn parse_overlay_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Overlay)?.location();
        self.expect(TokenKind::Char('('))?;
        if self.consume(TokenKind::Char(')')) {
            return Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: vec![make_string_node("overlay")],
                location: location as ParseLoc,
                ..FuncCall::default()
            }));
        }
        let first = self.parse_function_argument()?;
        if self.consume(TokenKind::Placing) {
            if matches!(&first, Node::NamedArgExpr(_)) {
                return self.fail("named arguments are not allowed in SQL OVERLAY syntax");
            }
            let replacement = self.parse_expr(0)?;
            self.expect(TokenKind::From)?;
            let start = self.parse_expr(0)?;
            let mut args = vec![first, replacement, start];
            if self.consume(TokenKind::For) {
                args.push(self.parse_expr(0)?);
            }
            self.expect(TokenKind::Char(')'))?;
            Some(self.make_sql_syntax_call("overlay", args, location))
        } else {
            let args = self.parse_plain_function_arguments_after(first)?;
            self.expect(TokenKind::Char(')'))?;
            Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: vec![make_string_node("overlay")],
                args,
                location: location as ParseLoc,
                ..FuncCall::default()
            }))
        }
    }

    pub(super) fn parse_substring_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Substring)?.location();
        self.expect(TokenKind::Char('('))?;
        if self.consume(TokenKind::Char(')')) {
            return Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: vec![make_string_node("substring")],
                location: location as ParseLoc,
                ..FuncCall::default()
            }));
        }
        if self.starts_named_function_argument() {
            let first = self.parse_function_argument()?;
            let args = self.parse_plain_function_arguments_after(first)?;
            self.expect(TokenKind::Char(')'))?;
            return Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: vec![make_string_node("substring")],
                args,
                location: location as ParseLoc,
                ..FuncCall::default()
            }));
        }
        let first = self.parse_expr(36)?;
        self.record_completion_tokens(&[TokenKind::From, TokenKind::For, TokenKind::Similar]);
        let args = match self.peek_kind() {
            TokenKind::From => {
                self.advance();
                let second = self.parse_expr(0)?;
                let mut args = vec![first, second];
                if self.consume(TokenKind::For) {
                    args.push(self.parse_expr(0)?);
                }
                args
            }
            TokenKind::For => {
                self.advance();
                let count = self.parse_expr(0)?;
                vec![
                    first,
                    Node::AConst(AConst::integer(1, -1)),
                    Node::TypeCast(TypeCast {
                        node_tag: NodeTag::TypeCast,
                        arg: Some(Box::new(count)),
                        type_name: Some(Box::new(TypeName {
                            node_tag: NodeTag::TypeName,
                            names: system_type_names("int4"),
                            location: -1,
                            ..TypeName::default()
                        })),
                        location: -1,
                    }),
                ]
            }
            TokenKind::Similar => {
                self.advance();
                let pattern = self.parse_expr(36)?;
                self.expect(TokenKind::Escape)?;
                let escape = self.parse_expr(36)?;
                vec![first, pattern, escape]
            }
            _ => {
                let args = self.parse_plain_function_arguments_after(first)?;
                self.expect(TokenKind::Char(')'))?;
                return Some(Node::FuncCall(FuncCall {
                    node_tag: NodeTag::FuncCall,
                    funcname: vec![make_string_node("substring")],
                    args,
                    location: location as ParseLoc,
                    ..FuncCall::default()
                }));
            }
        };
        self.expect(TokenKind::Char(')'))?;
        Some(self.make_sql_syntax_call("substring", args, location))
    }

    pub(super) fn parse_trim_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Trim)?.location();
        self.expect(TokenKind::Char('('))?;
        let function = if self.consume(TokenKind::Leading) {
            "ltrim"
        } else if self.consume(TokenKind::Trailing) {
            "rtrim"
        } else {
            self.consume(TokenKind::Both);
            "btrim"
        };
        let args = if self.consume(TokenKind::From) {
            self.parse_expr_list_until(TokenKind::Char(')'))?
        } else {
            let first = self.parse_expr(0)?;
            if self.consume(TokenKind::From) {
                let mut values = self.parse_expr_list_until(TokenKind::Char(')'))?;
                values.push(first);
                values
            } else {
                let mut values = vec![first];
                while self.consume(TokenKind::Char(',')) {
                    values.push(self.parse_expr(0)?);
                }
                values
            }
        };
        if args.is_empty() {
            return self.fail("TRIM requires an expression");
        }
        self.expect(TokenKind::Char(')'))?;
        Some(self.make_sql_syntax_call(function, args, location))
    }

    pub(super) fn parse_xmlexists_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Xmlexists)?.location();
        self.expect(TokenKind::Char('('))?;
        let xpath = self.parse_c_expr()?;
        self.expect(TokenKind::Passing)?;
        self.record_completion_lookahead_tokens(&[TokenKind::By]);
        if self.at(TokenKind::By) {
            self.parse_xml_passing_mechanism()?;
        }
        let document = self.parse_c_expr()?;
        self.record_completion_lookahead_tokens(&[TokenKind::By]);
        if self.at(TokenKind::By) {
            self.parse_xml_passing_mechanism()?;
        }
        self.expect(TokenKind::Char(')'))?;
        Some(self.make_sql_syntax_call("xmlexists", vec![xpath, document], location))
    }

    pub(super) fn parse_xml_passing_mechanism(&mut self) -> Option<()> {
        self.expect(TokenKind::By)?;
        if !self.consume(TokenKind::RefP) {
            self.expect(TokenKind::ValueP)?;
        }
        Some(())
    }

    pub(super) fn parse_sql_value_function(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        let (plain, with_precision) = match token.kind {
            TokenKind::CurrentDate => (SqlValueFunctionOp::CurrentDate, None),
            TokenKind::CurrentTime => (
                SqlValueFunctionOp::CurrentTime,
                Some(SqlValueFunctionOp::CurrentTimeN),
            ),
            TokenKind::CurrentTimestamp => (
                SqlValueFunctionOp::CurrentTimestamp,
                Some(SqlValueFunctionOp::CurrentTimestampN),
            ),
            TokenKind::Localtime => (
                SqlValueFunctionOp::Localtime,
                Some(SqlValueFunctionOp::LocaltimeN),
            ),
            TokenKind::Localtimestamp => (
                SqlValueFunctionOp::Localtimestamp,
                Some(SqlValueFunctionOp::LocaltimestampN),
            ),
            TokenKind::CurrentRole => (SqlValueFunctionOp::CurrentRole, None),
            TokenKind::CurrentUser => (SqlValueFunctionOp::CurrentUser, None),
            TokenKind::User => (SqlValueFunctionOp::User, None),
            TokenKind::SessionUser => (SqlValueFunctionOp::SessionUser, None),
            TokenKind::CurrentCatalog => (SqlValueFunctionOp::CurrentCatalog, None),
            TokenKind::CurrentSchema => (SqlValueFunctionOp::CurrentSchema, None),
            _ => return None,
        };
        let (op, typmod) = if self.consume(TokenKind::Char('(')) {
            let precision = match self.advance().value {
                Some(TokenValue::Integer(value)) => value,
                _ => return None,
            };
            self.expect(TokenKind::Char(')'))?;
            (with_precision?, precision)
        } else {
            (plain, -1)
        };
        Some(Node::SqlValueFunction(SqlValueFunction {
            xpr: Expr::new(NodeTag::SqlValueFunction),
            op,
            typmod,
            location: token.location() as ParseLoc,
            ..SqlValueFunction::default()
        }))
    }
}
