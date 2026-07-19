use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_json_func(&mut self, token: Token) -> Option<Node> {
        let op = match token.kind {
            TokenKind::JsonQuery => JsonExprOp::QueryOp,
            TokenKind::JsonExists => JsonExprOp::ExistsOp,
            TokenKind::JsonValue => JsonExprOp::ValueOp,
            _ => return None,
        };
        let context_item = self.parse_json_value_expr()?;
        self.expect(TokenKind::Char(','))?;
        let pathspec = self.parse_expr(0)?;
        let passing = self.parse_json_passing()?;
        let output = if op == JsonExprOp::ExistsOp {
            None
        } else {
            self.parse_json_output()?
        };
        let wrapper = if op == JsonExprOp::QueryOp {
            self.parse_json_wrapper()?
        } else {
            JsonWrapper::Unspec
        };
        let quotes = if op == JsonExprOp::QueryOp {
            self.parse_json_quotes()?
        } else {
            JsonQuotes::Unspec
        };
        let (on_empty, on_error) = self.parse_json_behaviors(op != JsonExprOp::ExistsOp)?;
        self.expect(TokenKind::Char(')'))?;
        Some(Node::JsonFuncExpr(JsonFuncExpr {
            node_tag: NodeTag::JsonFuncExpr,
            op,
            context_item: Some(Box::new(context_item)),
            pathspec: Some(Box::new(pathspec)),
            passing,
            output,
            on_empty,
            on_error,
            wrapper,
            quotes,
            location: token.location() as ParseLoc,
            ..JsonFuncExpr::default()
        }))
    }

    pub(super) fn parse_json_passing(&mut self) -> Option<NodeList> {
        if !self.consume(TokenKind::Passing) {
            return Some(Vec::new());
        }
        let mut arguments = Vec::new();
        loop {
            let val = self.parse_json_value_expr()?;
            self.expect(TokenKind::As)?;
            let name = self
                .consume_identifier_in_categories(&[
                    KeywordCategory::Unreserved,
                    KeywordCategory::ColName,
                    KeywordCategory::TypeFuncName,
                    KeywordCategory::Reserved,
                ])
                .or_else(|| self.fail("JSON PASSING requires a column label after AS"))?;
            arguments.push(Node::JsonArgument(JsonArgument {
                node_tag: NodeTag::JsonArgument,
                val: Some(Box::new(val)),
                name: Some(name),
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Some(arguments)
    }

    pub(super) fn parse_json_wrapper(&mut self) -> Option<JsonWrapper> {
        if self.consume(TokenKind::Without) {
            self.consume(TokenKind::Array);
            self.expect(TokenKind::Wrapper)?;
            Some(JsonWrapper::None)
        } else if self.consume(TokenKind::With) {
            let conditional = self.consume(TokenKind::Conditional);
            if !conditional {
                self.consume(TokenKind::Unconditional);
            }
            self.consume(TokenKind::Array);
            self.expect(TokenKind::Wrapper)?;
            if conditional {
                Some(JsonWrapper::Conditional)
            } else {
                Some(JsonWrapper::Unconditional)
            }
        } else {
            Some(JsonWrapper::Unspec)
        }
    }

    pub(super) fn parse_json_quotes(&mut self) -> Option<JsonQuotes> {
        if self.consume(TokenKind::Keep) {
            self.expect(TokenKind::Quotes)?;
            if self.consume(TokenKind::On) {
                self.expect(TokenKind::Scalar)?;
                self.expect(TokenKind::StringP)?;
            }
            Some(JsonQuotes::Keep)
        } else if self.consume(TokenKind::Omit) {
            self.expect(TokenKind::Quotes)?;
            if self.consume(TokenKind::On) {
                self.expect(TokenKind::Scalar)?;
                self.expect(TokenKind::StringP)?;
            }
            Some(JsonQuotes::Omit)
        } else {
            Some(JsonQuotes::Unspec)
        }
    }

    pub(super) fn parse_json_behaviors(&mut self, allow_empty: bool) -> Option<JsonBehaviorPair> {
        let mut on_empty = None;
        let mut on_error = None;
        while matches!(
            self.peek_kind(),
            TokenKind::Default
                | TokenKind::ErrorP
                | TokenKind::NullP
                | TokenKind::TrueP
                | TokenKind::FalseP
                | TokenKind::Unknown
                | TokenKind::EmptyP
        ) {
            let behavior = self.parse_json_behavior()?;
            self.expect(TokenKind::On)?;
            if allow_empty && self.consume(TokenKind::EmptyP) {
                if on_error.is_some() {
                    return self.fail("JSON ON EMPTY must precede ON ERROR");
                }
                if on_empty.is_some() {
                    return self.fail("duplicate ON EMPTY clause");
                }
                on_empty = Some(Box::new(behavior));
            } else {
                self.expect(TokenKind::ErrorP)?;
                if on_error.is_some() {
                    return self.fail("duplicate ON ERROR clause");
                }
                on_error = Some(Box::new(behavior));
            }
        }
        Some((on_empty, on_error))
    }

    pub(super) fn parse_json_behavior(&mut self) -> Option<JsonBehavior> {
        let location = self.location();
        let (btype, expr) = match self.peek_kind() {
            TokenKind::Default => {
                self.advance();
                (
                    JsonBehaviorType::Default,
                    Some(Box::new(self.parse_expr(0)?)),
                )
            }
            TokenKind::ErrorP => {
                self.advance();
                (JsonBehaviorType::Error, None)
            }
            TokenKind::NullP => {
                self.advance();
                (JsonBehaviorType::Null, None)
            }
            TokenKind::TrueP => {
                self.advance();
                (JsonBehaviorType::True, None)
            }
            TokenKind::FalseP => {
                self.advance();
                (JsonBehaviorType::False, None)
            }
            TokenKind::Unknown => {
                self.advance();
                (JsonBehaviorType::Unknown, None)
            }
            TokenKind::EmptyP => {
                self.advance();
                match self.peek_kind() {
                    TokenKind::ObjectP => {
                        self.advance();
                        (JsonBehaviorType::EmptyObject, None)
                    }
                    TokenKind::Array => {
                        self.advance();
                        (JsonBehaviorType::EmptyArray, None)
                    }
                    _ => return self.fail("EMPTY requires ARRAY or OBJECT"),
                }
            }
            _ => return None,
        };
        Some(JsonBehavior {
            node_tag: NodeTag::JsonBehavior,
            btype,
            expr,
            location: location as ParseLoc,
            ..JsonBehavior::default()
        })
    }

    pub(super) fn parse_json_object_agg(&mut self, location: usize) -> Option<Node> {
        let key = self.parse_expr(0)?;
        if !self.consume(TokenKind::ValueP) {
            self.expect(TokenKind::Char(':'))?;
        }
        let value = self.parse_json_value_expr()?;
        let absent_on_null = self.parse_json_null_clause(false)?;
        let unique = self.parse_json_unique_keys()?;
        let output = self.parse_json_output()?;
        self.expect(TokenKind::Char(')'))?;
        let mut constructor = JsonAggConstructor {
            node_tag: NodeTag::JsonAggConstructor,
            output,
            location: location as ParseLoc,
            ..JsonAggConstructor::default()
        };
        self.parse_json_aggregate_decorations(&mut constructor)?;
        Some(Node::JsonObjectAgg(JsonObjectAgg {
            node_tag: NodeTag::JsonObjectAgg,
            constructor: Some(Box::new(constructor)),
            arg: Some(Box::new(JsonKeyValue {
                node_tag: NodeTag::JsonKeyValue,
                key: Some(Box::new(key)),
                value: Some(Box::new(value)),
            })),
            absent_on_null,
            unique,
        }))
    }

    pub(super) fn parse_json_array_agg(&mut self, location: usize) -> Option<Node> {
        let value = self.parse_json_value_expr()?;
        let mut agg_order = Vec::new();
        if self.consume(TokenKind::Order) {
            self.expect(TokenKind::By)?;
            let start = self.pos;
            let mut end = start;
            let mut depth = 0usize;
            while end < self.tokens.len() {
                let kind = self.tokens[end].kind;
                if depth == 0
                    && (matches!(
                        kind,
                        TokenKind::Returning | TokenKind::Char(')') | TokenKind::Eof
                    ) || (matches!(kind, TokenKind::Absent | TokenKind::NullP)
                        && self.tokens.get(end + 1).map(|token| token.kind) == Some(TokenKind::On)))
                {
                    break;
                }
                match kind {
                    TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                    TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                    _ => {}
                }
                end += 1;
            }
            match parse_sort_list_tokens(self.tokens[start..end].to_vec()) {
                Ok(items) => agg_order = items,
                Err(error) => {
                    if self.error.is_none() {
                        self.error = Some(error);
                    }
                    return None;
                }
            }
            self.pos = end;
        }
        let absent_on_null = self.parse_json_null_clause(true)?;
        let output = self.parse_json_output()?;
        self.expect(TokenKind::Char(')'))?;
        let mut constructor = JsonAggConstructor {
            node_tag: NodeTag::JsonAggConstructor,
            output,
            agg_order,
            location: location as ParseLoc,
            ..JsonAggConstructor::default()
        };
        self.parse_json_aggregate_decorations(&mut constructor)?;
        Some(Node::JsonArrayAgg(JsonArrayAgg {
            node_tag: NodeTag::JsonArrayAgg,
            constructor: Some(Box::new(constructor)),
            arg: Some(Box::new(value)),
            absent_on_null,
        }))
    }

    pub(super) fn parse_json_aggregate_decorations(
        &mut self,
        constructor: &mut JsonAggConstructor,
    ) -> Option<()> {
        if self.consume(TokenKind::Filter) {
            self.expect(TokenKind::Char('('))?;
            self.expect(TokenKind::Where)?;
            constructor.agg_filter = Some(Box::new(self.parse_expr(0)?));
            self.expect(TokenKind::Char(')'))?;
        }
        constructor.over = self.parse_optional_over_clause()?;
        Some(())
    }
}
pub(super) fn parse_sort_list_tokens(mut tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.last().map_or(0, Token::end_location);
    tokens.push(Token::synthetic(TokenKind::Eof, location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion: None,
    };
    let items = parser.parse_sort_list_strict_until(&[TokenKind::Eof])?;
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after sort list"));
    }
    Ok(items)
}
