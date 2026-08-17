//! SQL/JSON query functions, behaviors, wrappers, passing clauses, and aggregates.
//!
//! This module owns the clause-rich JSON forms that operate on paths or aggregate
//! rows, complementing the value constructors in `expression_json`.

use super::expression::ExprParser;
use super::*;

const JSON_ABSENT_ON_NULL: &[TokenKind] = &[TokenKind::Absent, TokenKind::On, TokenKind::NullP];
const JSON_NULL_ON_NULL: &[TokenKind] = &[TokenKind::NullP, TokenKind::On, TokenKind::NullP];

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
        Some(node!(JsonFuncExpr {
            op,
            context_item: Some(Box::new(context_item)),
            pathspec: Some(Box::new(pathspec)),
            passing,
            output,
            on_empty,
            on_error,
            wrapper,
            quotes,
            parse_loc: token.offset() as ParseLoc,
            ..JsonFuncExpr::default()
        }))
    }

    pub(super) fn parse_json_passing(&mut self) -> Option<NodeList> {
        if !self.consume(TokenKind::Passing) {
            return Some(Vec::new());
        }
        let mut arguments = Vec::new();
        loop {
            let value = self.parse_json_value_expr()?;
            self.expect(TokenKind::As)?;
            let name = self
                .consume_column_label()
                .or_else(|| self.fail("JSON PASSING requires a column label after AS"))?;
            arguments.push(node!(JsonArgument {
                val: Some(Box::new(value)),
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

    pub(super) fn parse_json_behaviors(
        &mut self,
        allow_on_empty: bool,
    ) -> Option<JsonBehaviorPair> {
        let mut on_empty = None;
        let mut on_error = None;
        loop {
            if on_error.is_none() {
                self.record_completion_lookahead_tokens(&[
                    TokenKind::Default,
                    TokenKind::ErrorP,
                    TokenKind::NullP,
                    TokenKind::TrueP,
                    TokenKind::FalseP,
                    TokenKind::Unknown,
                    TokenKind::EmptyP,
                ]);
            }
            if !matches!(
                self.peek_kind(),
                TokenKind::Default
                    | TokenKind::ErrorP
                    | TokenKind::NullP
                    | TokenKind::TrueP
                    | TokenKind::FalseP
                    | TokenKind::Unknown
                    | TokenKind::EmptyP
            ) {
                break;
            }
            let behavior = self.parse_json_behavior()?;
            self.expect(TokenKind::On)?;
            if allow_on_empty && self.consume(TokenKind::EmptyP) {
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
        let offset = self.offset();
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
                self.record_completion_tokens(&[TokenKind::ObjectP, TokenKind::Array]);
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
            btype,
            expr,
            parse_loc: offset as ParseLoc,
            ..JsonBehavior::default()
        })
    }

    pub(super) fn parse_json_object_agg(&mut self, offset: usize) -> Option<Node> {
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
            output,
            parse_loc: offset as ParseLoc,
            ..JsonAggConstructor::default()
        };
        self.parse_json_aggregate_decorations(&mut constructor)?;
        Some(node!(JsonObjectAgg {
            constructor: Some(Box::new(constructor)),
            arg: Some(Box::new(JsonKeyValue {
                key: Some(Box::new(key)),
                value: Some(Box::new(value)),
            })),
            absent_on_null,
            unique,
        }))
    }

    pub(super) fn parse_json_array_agg(&mut self, offset: usize) -> Option<Node> {
        let value = self.parse_json_value_expr()?;
        let mut agg_order = Vec::new();
        if self.consume_phrase(&[TokenKind::Order, TokenKind::By])? {
            let start = self.pos;
            let mut end = start;
            let mut depth = 0usize;
            while end < self.tokens.len() {
                let kind = self.tokens[end].kind;
                if depth == 0 && kind == TokenKind::Completion {
                    if parse_sort_list_tokens(self.tokens[start..end].to_vec(), None).is_ok()
                        && let Some(collector) = &self.completion
                    {
                        let mut collector = collector.borrow_mut();
                        collector.record_follow_tokens(&[
                            TokenKind::Absent,
                            TokenKind::NullP,
                            TokenKind::Returning,
                            TokenKind::Char(')'),
                        ]);
                        collector.record_follow_phrase(JSON_ABSENT_ON_NULL);
                        collector.record_follow_phrase(JSON_NULL_ON_NULL);
                    }
                    end += 1;
                    continue;
                }
                if depth == 0
                    && (matches!(
                        kind,
                        TokenKind::Returning | TokenKind::Char(')') | TokenKind::Eof
                    ) || (matches!(kind, TokenKind::Absent | TokenKind::NullP)
                        && (self.tokens.get(end + 1).has_kind(TokenKind::On)
                            || (self.tokens.get(end + 1).has_kind(TokenKind::Completion)
                                && parse_sort_list_tokens(
                                    self.tokens[start..end].to_vec(),
                                    None,
                                )
                                .is_ok()))))
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
            match parse_sort_list_tokens(self.tokens[start..end].to_vec(), self.completion.clone())
            {
                Ok(items) => agg_order = items,
                Err(error) => return self.fail_with(error),
            }
            self.pos = end;
        }
        let absent_on_null = self.parse_json_null_clause(true)?;
        let output = self.parse_json_output()?;
        self.expect(TokenKind::Char(')'))?;
        let mut constructor = JsonAggConstructor {
            output,
            agg_order,
            parse_loc: offset as ParseLoc,
            ..JsonAggConstructor::default()
        };
        self.parse_json_aggregate_decorations(&mut constructor)?;
        Some(node!(JsonArrayAgg {
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
pub(super) fn parse_sort_list_tokens(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<NodeList> {
    let offset = tokens.last().end_offset_or(0);
    tokens.push(Token::synthetic(TokenKind::Eof, offset));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    let items = parser.parse_sort_list_strict_until(&[TokenKind::Eof])?;
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after sort list"));
    }
    Ok(items)
}
