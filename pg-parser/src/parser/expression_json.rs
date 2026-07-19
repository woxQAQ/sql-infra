use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_json_expression(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        match token.kind {
            TokenKind::JsonObject => {
                if self.json_object_uses_standard_syntax() {
                    self.parse_json_object_constructor(token.location())
                } else {
                    let first = self.parse_function_argument()?;
                    let args = self.parse_plain_function_arguments_after(first)?;
                    self.expect(TokenKind::Char(')'))?;
                    Some(Node::FuncCall(FuncCall {
                        node_tag: NodeTag::FuncCall,
                        funcname: system_type_names("json_object"),
                        args,
                        location: token.location() as ParseLoc,
                        ..FuncCall::default()
                    }))
                }
            }
            TokenKind::JsonArray => self.parse_json_array_constructor(token.location()),
            TokenKind::Json => {
                let expr = self.parse_json_value_expr()?;
                let unique_keys = self.parse_json_unique_keys()?;
                self.expect(TokenKind::Char(')'))?;
                Some(Node::JsonParseExpr(JsonParseExpr {
                    node_tag: NodeTag::JsonParseExpr,
                    expr: Some(Box::new(expr)),
                    unique_keys,
                    location: token.location() as ParseLoc,
                    ..JsonParseExpr::default()
                }))
            }
            TokenKind::JsonScalar => {
                let expr = self.parse_expr(0)?;
                self.expect(TokenKind::Char(')'))?;
                Some(Node::JsonScalarExpr(JsonScalarExpr {
                    node_tag: NodeTag::JsonScalarExpr,
                    expr: Some(Box::new(expr)),
                    location: token.location() as ParseLoc,
                    ..JsonScalarExpr::default()
                }))
            }
            TokenKind::JsonSerialize => {
                let expr = self.parse_json_value_expr()?;
                let output = self.parse_json_output()?;
                self.expect(TokenKind::Char(')'))?;
                Some(Node::JsonSerializeExpr(JsonSerializeExpr {
                    node_tag: NodeTag::JsonSerializeExpr,
                    expr: Some(Box::new(expr)),
                    output,
                    location: token.location() as ParseLoc,
                }))
            }
            TokenKind::JsonQuery | TokenKind::JsonExists | TokenKind::JsonValue => {
                self.parse_json_func(token)
            }
            TokenKind::JsonObjectagg => self.parse_json_object_agg(token.location()),
            TokenKind::JsonArrayagg => self.parse_json_array_agg(token.location()),
            _ => None,
        }
    }

    pub(super) fn parse_json_value_expr(&mut self) -> Option<JsonValueExpr> {
        let raw_expr = self.parse_expr(0)?;
        let format = self.parse_json_format()?;
        Some(JsonValueExpr {
            node_tag: NodeTag::JsonValueExpr,
            raw_expr: Some(Box::new(raw_expr)),
            format: Some(Box::new(format.unwrap_or_else(default_json_format))),
            ..JsonValueExpr::default()
        })
    }

    pub(super) fn json_object_uses_standard_syntax(&self) -> bool {
        if matches!(
            self.peek_kind(),
            TokenKind::Char(')') | TokenKind::Returning
        ) {
            return true;
        }
        let mut depth = 0usize;
        for token in &self.tokens[self.pos..self.end] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::ValueP | TokenKind::Char(':') if depth == 0 => return true,
                _ => {}
            }
        }
        false
    }

    pub(super) fn parse_json_format(&mut self) -> Option<Option<JsonFormat>> {
        if !self.consume(TokenKind::Format) {
            return Some(None);
        }
        let location = self.previous_location();
        self.expect(TokenKind::Json)?;
        let encoding = if self.consume(TokenKind::Encoding) {
            let name = self
                .consume_identifier_in_categories(&[
                    KeywordCategory::Unreserved,
                    KeywordCategory::ColName,
                ])
                .or_else(|| self.fail("JSON ENCODING requires a name"))?;
            match name.to_ascii_lowercase().as_str() {
                "utf8" => JsonEncoding::Utf8,
                "utf16" => JsonEncoding::Utf16,
                "utf32" => JsonEncoding::Utf32,
                _ => return self.fail(format!("unrecognized JSON encoding: {name}")),
            }
        } else {
            JsonEncoding::Default
        };
        Some(Some(JsonFormat {
            node_tag: NodeTag::JsonFormat,
            format_type: JsonFormatType::Json,
            encoding,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_json_output(&mut self) -> Option<Option<Box<JsonOutput>>> {
        if !self.consume(TokenKind::Returning) {
            return Some(None);
        }
        let mut type_tokens = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Format
                | TokenKind::With
                | TokenKind::Without
                | TokenKind::NullP
                | TokenKind::Absent
                | TokenKind::ErrorP
                | TokenKind::Default
                | TokenKind::EmptyP
                | TokenKind::Keep
                | TokenKind::Omit
                | TokenKind::Char(')')
                | TokenKind::Eof
        ) {
            type_tokens.push(self.advance().clone());
        }
        let type_name = tokens_to_type_name(type_tokens).map(Box::new)?;
        let format = Some(Box::new(
            self.parse_json_format()?
                .unwrap_or_else(default_json_format),
        ));
        Some(Some(Box::new(JsonOutput {
            node_tag: NodeTag::JsonOutput,
            type_name: Some(type_name),
            returning: Some(Box::new(JsonReturning {
                node_tag: NodeTag::JsonReturning,
                format,
                ..JsonReturning::default()
            })),
        })))
    }

    pub(super) fn parse_json_unique_keys(&mut self) -> Option<bool> {
        if self.consume(TokenKind::With) {
            self.expect(TokenKind::Unique)?;
            self.consume(TokenKind::Keys);
            Some(true)
        } else if self.consume(TokenKind::Without) {
            self.expect(TokenKind::Unique)?;
            self.consume(TokenKind::Keys);
            Some(false)
        } else {
            Some(false)
        }
    }

    pub(super) fn parse_json_null_clause(&mut self, array_default: bool) -> Option<bool> {
        if self.consume(TokenKind::Absent) {
            self.expect(TokenKind::On)?;
            self.expect(TokenKind::NullP)?;
            Some(true)
        } else if self.consume(TokenKind::NullP) {
            self.expect(TokenKind::On)?;
            self.expect(TokenKind::NullP)?;
            Some(false)
        } else {
            Some(array_default)
        }
    }

    pub(super) fn parse_json_object_constructor(&mut self, location: usize) -> Option<Node> {
        let mut exprs = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Absent
                | TokenKind::NullP
                | TokenKind::With
                | TokenKind::Without
                | TokenKind::Returning
                | TokenKind::Char(')')
                | TokenKind::Eof
        ) {
            let key = self.parse_expr(0)?;
            if !self.consume(TokenKind::ValueP) {
                self.expect(TokenKind::Char(':'))?;
            }
            let value = self.parse_json_value_expr()?;
            exprs.push(Node::JsonKeyValue(JsonKeyValue {
                node_tag: NodeTag::JsonKeyValue,
                key: Some(Box::new(key)),
                value: Some(Box::new(value)),
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if matches!(
                self.peek_kind(),
                TokenKind::Absent
                    | TokenKind::NullP
                    | TokenKind::With
                    | TokenKind::Without
                    | TokenKind::Returning
                    | TokenKind::Char(')')
                    | TokenKind::Eof
            ) {
                return self.fail("expected a JSON object member after ','");
            }
        }
        let absent_on_null = self.parse_json_null_clause(false)?;
        let unique = self.parse_json_unique_keys()?;
        let output = self.parse_json_output()?;
        self.expect(TokenKind::Char(')'))?;
        Some(Node::JsonObjectConstructor(JsonObjectConstructor {
            node_tag: NodeTag::JsonObjectConstructor,
            exprs,
            output,
            absent_on_null,
            unique,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_json_array_constructor(&mut self, location: usize) -> Option<Node> {
        if self.starts_statement() {
            let range = self.take_until_balanced_range(TokenKind::Char(')'));
            let mut depth = 0usize;
            let mut suffix_start = range.end;
            for (relative_index, token) in self.tokens[range.clone()].iter().enumerate() {
                let index = range.start + relative_index;
                match token.kind {
                    TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                    TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                    TokenKind::Format | TokenKind::Returning
                        if depth == 0 && self.select_range_is_valid(range.start..index) =>
                    {
                        suffix_start = index;
                        break;
                    }
                    _ => {}
                }
            }
            let query = self.parse_nested_select_range(range.start..suffix_start)?;
            let mut suffix = self.expression_view(suffix_start..range.end);
            let format = match suffix.parse_json_format() {
                Some(format) => Some(Box::new(format.unwrap_or_else(default_json_format))),
                None => {
                    if self.error.is_none() {
                        self.error = suffix.error.take();
                    }
                    return None;
                }
            };
            let output = match suffix.parse_json_output() {
                Some(output) => output,
                None => {
                    if self.error.is_none() {
                        self.error = suffix.error.take();
                    }
                    return None;
                }
            };
            if !suffix.at(TokenKind::Eof) {
                return self.fail("unexpected token after JSON_ARRAY query clauses");
            }
            self.expect(TokenKind::Char(')'))?;
            return Some(Node::JsonArrayQueryConstructor(JsonArrayQueryConstructor {
                node_tag: NodeTag::JsonArrayQueryConstructor,
                query: Some(Box::new(query)),
                output,
                format,
                absent_on_null: true,
                location: location as ParseLoc,
            }));
        }
        let mut exprs = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Absent
                | TokenKind::NullP
                | TokenKind::Returning
                | TokenKind::Char(')')
                | TokenKind::Eof
        ) {
            exprs.push(Node::JsonValueExpr(self.parse_json_value_expr()?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if matches!(
                self.peek_kind(),
                TokenKind::Absent
                    | TokenKind::NullP
                    | TokenKind::Returning
                    | TokenKind::Char(')')
                    | TokenKind::Eof
            ) {
                return self.fail("expected a JSON array element after ','");
            }
        }
        let absent_on_null = self.parse_json_null_clause(true)?;
        let output = self.parse_json_output()?;
        self.expect(TokenKind::Char(')'))?;
        Some(Node::JsonArrayConstructor(JsonArrayConstructor {
            node_tag: NodeTag::JsonArrayConstructor,
            exprs,
            output,
            absent_on_null,
            location: location as ParseLoc,
        }))
    }
}

impl Parser {
    pub(super) fn parse_json_value_expr_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> PResult<JsonValueExpr> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            return Err(ParseError::new(
                location,
                "expected a JSON value expression",
            ));
        }
        let mut parser = self.expression_view(range);
        let value = parser.parse_json_value_expr().ok_or_else(|| {
            parser
                .error
                .take()
                .unwrap_or_else(|| ParseError::new(location, "invalid JSON value expression"))
        })?;
        if !parser.at(TokenKind::Eof) {
            return Err(ParseError::new(
                parser.location(),
                "unexpected token after JSON value expression",
            ));
        }
        Ok(value)
    }
}

pub(super) fn default_json_format() -> JsonFormat {
    JsonFormat {
        node_tag: NodeTag::JsonFormat,
        location: -1,
        ..JsonFormat::default()
    }
}

pub(super) fn json_behavior_starts(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Default
            | TokenKind::ErrorP
            | TokenKind::NullP
            | TokenKind::TrueP
            | TokenKind::FalseP
            | TokenKind::Unknown
            | TokenKind::EmptyP
    )
}
