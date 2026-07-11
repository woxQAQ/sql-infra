use super::*;

impl Parser {
    pub(super) fn parse_plain_range_var(&mut self) -> Option<RangeVar> {
        let location = self.location();
        let parts = self.consume_qualified_name_parts();
        (!parts.is_empty()).then(|| range_var_from_parts(parts, location))
    }

    pub(super) fn parse_vacuum_relation_list(&mut self) -> PResult<NodeList> {
        let mut rels = Vec::new();
        if self.at_statement_end() {
            return Ok(rels);
        }
        loop {
            let relation = self.parse_relation_expr(false)?;
            let va_cols = if self.consume(TokenKind::Char('(')) {
                let columns = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                columns
            } else {
                Vec::new()
            };
            rels.push(Node::VacuumRelation(VacuumRelation {
                node_tag: NodeTag::VacuumRelation,
                relation: Some(Box::new(relation)),
                va_cols,
                ..VacuumRelation::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(rels)
    }

    pub(super) fn parse_copy_options(&mut self) -> PResult<NodeList> {
        self.consume(TokenKind::With);
        if self.at(TokenKind::Char('(')) {
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("COPY option list cannot be empty"));
            }
            let mut options = Vec::new();
            loop {
                let location = self.location();
                let tokens =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                options.push(Node::DefElem(parse_copy_generic_option(tokens, location)?));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected a COPY option after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
            return Ok(options);
        }

        let mut options = Vec::new();
        while !self.at_statement_end() && !self.at(TokenKind::Where) {
            let location = self.location();
            let (name, arg) = match self.peek_kind() {
                TokenKind::Binary => {
                    self.advance();
                    ("format", Some(make_string_node("binary")))
                }
                TokenKind::Csv => {
                    self.advance();
                    ("format", Some(make_string_node("csv")))
                }
                TokenKind::Json => {
                    self.advance();
                    ("format", Some(make_string_node("json")))
                }
                TokenKind::Freeze => {
                    self.advance();
                    ("freeze", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::HeaderP => {
                    self.advance();
                    ("header", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::Delimiter
                | TokenKind::NullP
                | TokenKind::Quote
                | TokenKind::Escape
                | TokenKind::Encoding => {
                    let kind = self.advance().kind;
                    if kind != TokenKind::Encoding {
                        self.consume(TokenKind::As);
                    }
                    if !self.at(TokenKind::SConst) {
                        return Err(self.error_here("COPY option requires a string value"));
                    }
                    let value = self
                        .consume_string_like()
                        .ok_or_else(|| self.error_here("COPY option requires a string value"))?;
                    let name = match kind {
                        TokenKind::Delimiter => "delimiter",
                        TokenKind::NullP => "null",
                        TokenKind::Quote => "quote",
                        TokenKind::Escape => "escape",
                        _ => "encoding",
                    };
                    (name, Some(make_string_node(value)))
                }
                TokenKind::Force => {
                    self.advance();
                    let name = if self.consume(TokenKind::Quote) {
                        "force_quote"
                    } else if self.consume(TokenKind::Not) {
                        self.expect(TokenKind::NullP)?;
                        "force_not_null"
                    } else {
                        self.expect(TokenKind::NullP)?;
                        "force_null"
                    };
                    let value = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let mut columns = Vec::new();
                        loop {
                            let column = match self.peek_kind() {
                                TokenKind::IConst => match self.advance().value.as_ref() {
                                    Some(TokenValue::Integer(value)) => value.to_string(),
                                    _ => unreachable!("IConst token requires an integer value"),
                                },
                                TokenKind::FConst | TokenKind::SConst => {
                                    match self.advance().value.as_ref() {
                                        Some(TokenValue::String(value)) => value.clone(),
                                        _ => unreachable!("literal token requires a string value"),
                                    }
                                }
                                _ => self.consume_col_label().ok_or_else(|| {
                                    self.error_here(
                                        "COPY FORCE option requires a column-list item or '*'",
                                    )
                                })?,
                            };
                            columns.push(make_string_node(column));
                            if !self.consume(TokenKind::Char(',')) {
                                break;
                            }
                        }
                        Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: columns,
                            location: -1,
                            ..AArrayExpr::default()
                        })
                    };
                    (name, Some(value))
                }
                _ => return Err(self.error_here("invalid COPY option")),
            };
            options.push(make_def_elem(name, arg, location));
        }
        Ok(options)
    }

    pub(super) fn parse_def_elem_list(&mut self) -> PResult<NodeList> {
        self.parse_parenthesized_def_elem_list_strict()
    }

    pub(super) fn parse_options_clause(&mut self) -> PResult<NodeList> {
        if self.at(TokenKind::Options) {
            self.parse_create_generic_options()
        } else {
            Ok(Vec::new())
        }
    }
}

fn parse_copy_generic_option(mut tokens: Vec<Token>, location: usize) -> PResult<DefElem> {
    let eof_location = tokens.last().map_or(location, |token| token.location);
    tokens.push(Token {
        kind: TokenKind::Eof,
        location: eof_location,
        value: None,
    });
    let mut parser = Parser { tokens, pos: 0 };
    let name = if parser.consume(TokenKind::FormatLa) {
        "format".to_owned()
    } else {
        parser
            .consume_col_label()
            .ok_or_else(|| parser.error_here("expected a COPY option name"))?
    };
    let arg = if parser.at(TokenKind::Eof) {
        None
    } else if parser.consume(TokenKind::Char('(')) {
        if parser.at(TokenKind::Char(')')) {
            return Err(parser.error_here("COPY option argument list cannot be empty"));
        }
        let mut values = Vec::new();
        loop {
            let value = parser
                .consume_opt_boolean_or_string()
                .ok_or_else(|| parser.error_here("expected a COPY option string value"))?;
            values.push(make_string_node(value));
            if !parser.consume(TokenKind::Char(',')) {
                break;
            }
            if parser.at(TokenKind::Char(')')) {
                return Err(parser.error_here("expected a COPY option value after ','"));
            }
        }
        parser.expect(TokenKind::Char(')'))?;
        Some(Node::AArrayExpr(AArrayExpr {
            node_tag: NodeTag::AArrayExpr,
            elements: values,
            location: -1,
            ..AArrayExpr::default()
        }))
    } else if parser.consume(TokenKind::Char('*')) {
        Some(Node::AStar(AStar {
            node_tag: NodeTag::AStar,
        }))
    } else if parser.consume(TokenKind::Default) {
        Some(make_string_node("default"))
    } else if parser.at_any(&[
        TokenKind::IConst,
        TokenKind::FConst,
        TokenKind::Char('+'),
        TokenKind::Char('-'),
    ]) {
        Some(parser.parse_numeric_only()?)
    } else {
        Some(make_string_node(
            parser
                .consume_opt_boolean_or_string()
                .ok_or_else(|| parser.error_here("invalid COPY option value"))?,
        ))
    };
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after COPY option"));
    }
    Ok(DefElem {
        node_tag: NodeTag::DefElem,
        defname: Some(name),
        arg: arg.map(Box::new),
        location: location as ParseLoc,
        ..DefElem::default()
    })
}
