use super::*;

impl Parser {
    pub(super) fn parse_json_table(&mut self, lateral: bool) -> PResult<JsonTable> {
        let location = self.expect(TokenKind::JsonTable)?.location();
        self.expect(TokenKind::Char('('))?;

        let context_location = self.location();
        let context_tokens = self.take_until_top_level(&[TokenKind::Char(',')]);
        if self.at_completion()
            && parse_json_value_expr_tokens_with_completion(context_tokens.clone(), None).is_ok()
        {
            self.record_completion_follow_tokens(&[TokenKind::Char(',')]);
        }
        let context_item = self
            .parse_json_value_fragment_tokens(context_tokens)
            .map_err(|mut error| {
                if error.location() == 0 {
                    error.reanchor(context_location);
                }
                error
            })?;
        self.expect(TokenKind::Char(','))?;

        let pathspec = self.parse_json_table_path_spec(false)?.ok_or_else(|| {
            self.error_here("JSON_TABLE requires a string constant path specification")
        })?;
        let passing = self.parse_json_table_passing_clause()?;

        self.expect(TokenKind::Columns)?;
        self.expect(TokenKind::Char('('))?;
        let columns = self.parse_json_table_column_list()?;
        self.expect(TokenKind::Char(')'))?;

        let on_error = self.parse_json_table_on_error_clause()?;
        self.expect(TokenKind::Char(')'))?;
        let alias = self.parse_optional_alias_clause()?;

        Ok(JsonTable {
            node_tag: NodeTag::JsonTable,
            context_item: Some(Box::new(context_item)),
            pathspec: Some(Box::new(pathspec)),
            passing,
            columns,
            on_error,
            alias,
            lateral,
            location: location as ParseLoc,
        })
    }

    pub(super) fn parse_json_table_passing_clause(&mut self) -> PResult<NodeList> {
        if !self.consume(TokenKind::Passing) {
            return Ok(Vec::new());
        }

        let mut arguments = Vec::new();
        loop {
            let location = self.location();
            let value_tokens = self.take_until_top_level(&[TokenKind::As]);
            if self.at_completion()
                && parse_json_value_expr_tokens_with_completion(value_tokens.clone(), None).is_ok()
            {
                self.record_completion_follow_tokens(&[TokenKind::As]);
            }
            let value = self.parse_json_value_fragment_tokens(value_tokens)?;
            self.expect(TokenKind::As)?;
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("JSON PASSING argument requires a name"))?;
            arguments.push(Node::JsonArgument(JsonArgument {
                node_tag: NodeTag::JsonArgument,
                val: Some(Box::new(value)),
                name: Some(name),
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.peek_kind() == TokenKind::Columns {
                return Err(ParseError::syntax_exit(
                    location,
                    "expected a JSON PASSING argument after ','",
                ));
            }
        }
        Ok(arguments)
    }

    pub(super) fn parse_json_table_column_list(&mut self) -> PResult<NodeList> {
        let mut columns = Vec::new();
        loop {
            if self.at(TokenKind::Char(')')) {
                if columns.is_empty() {
                    return Err(self.error_here("JSON_TABLE COLUMNS requires at least one column"));
                }
                break;
            }
            columns.push(Node::JsonTableColumn(self.parse_json_table_column()?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a JSON_TABLE column after ','"));
            }
        }
        Ok(columns)
    }

    pub(super) fn parse_json_table_column(&mut self) -> PResult<JsonTableColumn> {
        let location = self.location();
        if self.consume(TokenKind::Nested) {
            self.consume(TokenKind::Path);
            let pathspec = self.parse_json_table_path_spec(false)?.ok_or_else(|| {
                self.error_here("NESTED JSON_TABLE column requires a string path")
            })?;
            self.expect(TokenKind::Columns)?;
            self.expect(TokenKind::Char('('))?;
            let columns = self.parse_json_table_column_list()?;
            self.expect(TokenKind::Char(')'))?;
            return Ok(JsonTableColumn {
                node_tag: NodeTag::JsonTableColumn,
                coltype: JsonTableColumnType::Nested,
                pathspec: Some(Box::new(pathspec)),
                columns,
                location: location as ParseLoc,
                ..JsonTableColumn::default()
            });
        }

        let name = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected a JSON_TABLE column name"))?;
        if self.consume(TokenKind::For) {
            self.expect(TokenKind::Ordinality)?;
            return Ok(JsonTableColumn {
                node_tag: NodeTag::JsonTableColumn,
                coltype: JsonTableColumnType::ForOrdinality,
                name: Some(name),
                location: location as ParseLoc,
                ..JsonTableColumn::default()
            });
        }

        let type_name = self
            .parse_type_name_until(&[
                TokenKind::Format,
                TokenKind::Path,
                TokenKind::Exists,
                TokenKind::With,
                TokenKind::Without,
                TokenKind::Keep,
                TokenKind::Omit,
                TokenKind::Default,
                TokenKind::ErrorP,
                TokenKind::NullP,
                TokenKind::TrueP,
                TokenKind::FalseP,
                TokenKind::Unknown,
                TokenKind::EmptyP,
                TokenKind::Char(','),
                TokenKind::Char(')'),
                TokenKind::Eof,
            ])
            .map(Box::new)
            .ok_or_else(|| self.error_here("JSON_TABLE column requires a type name"))?;

        let exists = self.consume(TokenKind::Exists);
        let explicit_format = if exists {
            None
        } else {
            self.parse_json_format_clause()?.map(Box::new)
        };
        let pathspec = self.parse_json_table_path_spec(true)?.map(Box::new);

        if exists {
            let on_error = self.parse_json_table_on_error_clause()?;
            return Ok(JsonTableColumn {
                node_tag: NodeTag::JsonTableColumn,
                coltype: JsonTableColumnType::Exists,
                name: Some(name),
                type_name: Some(type_name),
                pathspec,
                format: Some(Box::new(default_json_format())),
                wrapper: JsonWrapper::None,
                quotes: JsonQuotes::Unspec,
                on_error,
                location: location as ParseLoc,
                ..JsonTableColumn::default()
            });
        }

        let wrapper = self.parse_json_wrapper_clause()?;
        let quotes = self.parse_json_quotes_clause()?;
        let (on_empty, on_error) = self.parse_json_table_behavior_clauses()?;
        Ok(JsonTableColumn {
            node_tag: NodeTag::JsonTableColumn,
            coltype: if explicit_format.is_some() {
                JsonTableColumnType::Formatted
            } else {
                JsonTableColumnType::Regular
            },
            name: Some(name),
            type_name: Some(type_name),
            pathspec,
            format: explicit_format.or_else(|| Some(Box::new(default_json_format()))),
            wrapper,
            quotes,
            on_empty,
            on_error,
            location: location as ParseLoc,
            ..JsonTableColumn::default()
        })
    }

    pub(super) fn parse_json_table_path_spec(
        &mut self,
        require_path_keyword: bool,
    ) -> PResult<Option<JsonTablePathSpec>> {
        if require_path_keyword && !self.consume(TokenKind::Path) {
            return Ok(None);
        }
        if !require_path_keyword {
            self.consume(TokenKind::Path);
        }
        if !self.at(TokenKind::SConst) {
            return Ok(None);
        }
        let token = self.advance().clone();
        let value = token_name(&token)
            .ok_or_else(|| ParseError::ranged(token.range, "invalid JSON path string"))?;
        let (name, name_location) = if self.consume(TokenKind::As) {
            let name_location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("JSON path AS requires a name"))?;
            (Some(name), name_location as ParseLoc)
        } else {
            (None, -1)
        };
        Ok(Some(JsonTablePathSpec {
            node_tag: NodeTag::JsonTablePathSpec,
            string: Some(Box::new(Node::AConst(AConst::string(
                value,
                token.location() as ParseLoc,
            )))),
            name,
            name_location,
            location: token.location() as ParseLoc,
        }))
    }

    pub(super) fn parse_json_format_clause(&mut self) -> PResult<Option<JsonFormat>> {
        if !self.consume(TokenKind::Format) {
            return Ok(None);
        }
        let location = self.previous_location();
        self.expect(TokenKind::Json)?;
        let encoding = if self.consume(TokenKind::Encoding) {
            let token = self.peek().clone();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("JSON ENCODING requires a name"))?;
            match name.to_ascii_lowercase().as_str() {
                "utf8" => JsonEncoding::Utf8,
                "utf16" => JsonEncoding::Utf16,
                "utf32" => JsonEncoding::Utf32,
                _ => {
                    return Err(ParseError::ranged(
                        token.range,
                        format!("unrecognized JSON encoding: {name}"),
                    ));
                }
            }
        } else {
            JsonEncoding::Default
        };
        Ok(Some(JsonFormat {
            node_tag: NodeTag::JsonFormat,
            format_type: JsonFormatType::Json,
            encoding,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_json_wrapper_clause(&mut self) -> PResult<JsonWrapper> {
        if self.consume(TokenKind::Without) {
            self.consume(TokenKind::Array);
            self.expect(TokenKind::Wrapper)?;
            Ok(JsonWrapper::None)
        } else if self.consume(TokenKind::With) {
            let conditional = self.consume(TokenKind::Conditional);
            if !conditional {
                self.consume(TokenKind::Unconditional);
            }
            self.consume(TokenKind::Array);
            self.expect(TokenKind::Wrapper)?;
            Ok(if conditional {
                JsonWrapper::Conditional
            } else {
                JsonWrapper::Unconditional
            })
        } else {
            Ok(JsonWrapper::Unspec)
        }
    }

    pub(super) fn parse_json_quotes_clause(&mut self) -> PResult<JsonQuotes> {
        let quotes = if self.consume(TokenKind::Keep) {
            JsonQuotes::Keep
        } else if self.consume(TokenKind::Omit) {
            JsonQuotes::Omit
        } else {
            return Ok(JsonQuotes::Unspec);
        };
        self.expect(TokenKind::Quotes)?;
        if self.consume(TokenKind::On) {
            self.expect(TokenKind::Scalar)?;
            self.expect(TokenKind::StringP)?;
        }
        Ok(quotes)
    }

    pub(super) fn parse_json_table_behavior_clauses(&mut self) -> PResult<JsonBehaviorPair> {
        let mut on_empty = None;
        let mut on_error = None;
        self.record_completion_follow_tokens(json_behavior_tokens());
        while json_behavior_starts(self.peek_kind()) {
            let behavior = self.parse_json_table_behavior()?;
            self.expect(TokenKind::On)?;
            if self.consume(TokenKind::EmptyP) {
                if on_error.is_some() {
                    return Err(self.error_here("JSON ON EMPTY must precede ON ERROR"));
                }
                if on_empty.is_some() {
                    return Err(self.error_here("duplicate JSON ON EMPTY clause"));
                }
                on_empty = Some(Box::new(behavior));
            } else {
                self.expect(TokenKind::ErrorP)?;
                if on_error.is_some() {
                    return Err(self.error_here("duplicate JSON ON ERROR clause"));
                }
                on_error = Some(Box::new(behavior));
            }
            self.record_completion_follow_tokens(json_behavior_tokens());
        }
        Ok((on_empty, on_error))
    }

    pub(super) fn parse_json_table_on_error_clause(
        &mut self,
    ) -> PResult<Option<Box<JsonBehavior>>> {
        self.record_completion_follow_tokens(json_behavior_tokens());
        if !json_behavior_starts(self.peek_kind()) {
            return Ok(None);
        }
        let behavior = self.parse_json_table_behavior()?;
        self.expect(TokenKind::On)?;
        self.expect(TokenKind::ErrorP)?;
        Ok(Some(Box::new(behavior)))
    }

    pub(super) fn parse_json_table_behavior(&mut self) -> PResult<JsonBehavior> {
        let location = self.location();
        let (btype, expr) = match self.peek_kind() {
            TokenKind::Default => {
                self.advance();
                let tokens = self.take_until_top_level(&[TokenKind::On]);
                (
                    JsonBehaviorType::Default,
                    Some(Box::new(self.parse_expression_fragment_tokens(tokens)?)),
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
                    _ => return Err(self.error_here("EMPTY requires ARRAY or OBJECT")),
                }
            }
            _ => return Err(self.error_here("expected a JSON behavior")),
        };
        Ok(JsonBehavior {
            node_tag: NodeTag::JsonBehavior,
            btype,
            expr,
            location: location as ParseLoc,
            ..JsonBehavior::default()
        })
    }
}

fn json_behavior_tokens() -> &'static [TokenKind] {
    &[
        TokenKind::Default,
        TokenKind::ErrorP,
        TokenKind::NullP,
        TokenKind::TrueP,
        TokenKind::FalseP,
        TokenKind::Unknown,
        TokenKind::EmptyP,
    ]
}
