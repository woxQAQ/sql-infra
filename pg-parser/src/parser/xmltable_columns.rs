//! `XMLTABLE` column definitions and option boundaries.
//!
//! Ordinality, type, path, default, and nullability clauses are split from their
//! expression fragments without losing completion context.

use super::*;

pub(super) fn xmltable_column_from_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<RangeTableFuncCol> {
    let location = tokens.first().location_or(0);
    let eof_location = tokens.last().end_location_or(location);
    tokens.push(Token::synthetic(TokenKind::Eof, eof_location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    let column = parser.parse_xmltable_column_body()?;
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token in XMLTABLE column"));
    }
    Ok(column)
}

impl Parser {
    fn parse_xmltable_column_body(&mut self) -> PResult<RangeTableFuncCol> {
        let location = self.location();
        self.record_completion_slot(GrammarSlot::AnyName);
        let colname = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected an XMLTABLE column name"))?;
        self.record_completion_tokens(&[TokenKind::For]);
        if self.consume(TokenKind::For) {
            self.expect(TokenKind::Ordinality)?;
            self.record_completion_follow_tokens(COMMA_OR_CLOSE_PAREN_TOKENS);
            return Ok(RangeTableFuncCol {
                colname: Some(colname),
                for_ordinality: true,
                location: location as ParseLoc,
                ..RangeTableFuncCol::default()
            });
        }

        let option_starts = [
            TokenKind::Path,
            TokenKind::Default,
            TokenKind::Not,
            TokenKind::NullP,
            TokenKind::Eof,
        ];
        let type_name = self
            .parse_type_name_until(&option_starts)
            .map(Box::new)
            .ok_or_else(|| self.error_here("expected an XMLTABLE column type"))?;
        let mut column = RangeTableFuncCol {
            colname: Some(colname.clone()),
            type_name: Some(type_name),
            location: location as ParseLoc,
            ..RangeTableFuncCol::default()
        };
        let suffixes = [
            TokenKind::Path,
            TokenKind::Default,
            TokenKind::Not,
            TokenKind::NullP,
            TokenKind::Char(','),
            TokenKind::Char(')'),
        ];
        let mut nullability_seen = false;
        loop {
            self.record_completion_follow_tokens(&suffixes);
            match self.peek_kind() {
                TokenKind::Not => {
                    let token = self.advance().clone();
                    self.expect(TokenKind::NullP)?;
                    if nullability_seen {
                        return Err(ParseError::ranged(
                            token.range,
                            format!(
                                "conflicting or redundant NULL / NOT NULL declarations for column {colname:?}"
                            ),
                        ));
                    }
                    column.is_not_null = true;
                    nullability_seen = true;
                }
                TokenKind::NullP => {
                    let token = self.advance().clone();
                    if nullability_seen {
                        return Err(ParseError::ranged(
                            token.range,
                            format!(
                                "conflicting or redundant NULL / NOT NULL declarations for column {colname:?}"
                            ),
                        ));
                    }
                    column.is_not_null = false;
                    nullability_seen = true;
                }
                TokenKind::Path | TokenKind::Default => {
                    let is_path = self.peek_kind() == TokenKind::Path;
                    let option_location = self.advance().location();
                    let start = self.pos;
                    let available_end = self.tokens[start..]
                        .iter()
                        .position(|token| {
                            matches!(token.kind, TokenKind::Completion | TokenKind::Eof)
                        })
                        .map_or(self.tokens.len(), |end| start + end);
                    let end = xmltable_option_expression_end(&self.tokens, start, available_end);
                    let expression_tokens = self.tokens[start..end].to_vec();
                    self.pos = end;
                    self.record_expression_follow_tokens(&expression_tokens, &suffixes, true);
                    let expression = self.parse_b_expression_fragment_tokens(expression_tokens)?;
                    if is_path {
                        if column.colexpr.is_some() {
                            return Err(ParseError::syntax_exit(
                                option_location,
                                "only one PATH value per column is allowed",
                            ));
                        }
                        column.colexpr = Some(Box::new(expression));
                    } else {
                        if column.coldefexpr.is_some() {
                            return Err(ParseError::syntax_exit(
                                option_location,
                                "only one DEFAULT value is allowed",
                            ));
                        }
                        column.coldefexpr = Some(Box::new(expression));
                    }
                }
                TokenKind::Ident | TokenKind::UIdent => {
                    let token = self.peek().clone();
                    let option = token_name(&token).unwrap_or_default();
                    let message = if option == "__pg__is_not_null" {
                        format!("option name {option:?} cannot be used in XMLTABLE")
                    } else {
                        format!("unrecognized column option {option:?}")
                    };
                    return Err(ParseError::ranged(token.range, message));
                }
                TokenKind::Eof => break,
                _ => {
                    return Err(self.error_here("unsupported XMLTABLE column option"));
                }
            }
        }
        Ok(column)
    }
}

fn xmltable_option_starts_at(tokens: &[Token], index: usize) -> bool {
    match tokens[index].kind {
        TokenKind::Path | TokenKind::Default | TokenKind::NullP => true,
        TokenKind::Not => matches!(
            tokens.get(index + 1).map(|token| token.kind),
            Some(TokenKind::NullP | TokenKind::Completion)
        ),
        TokenKind::Ident | TokenKind::UIdent => true,
        _ => false,
    }
}

fn xmltable_option_expression_end(tokens: &[Token], start: usize, available_end: usize) -> usize {
    let mut depth = 0usize;
    for index in start..available_end {
        match tokens[index].kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
        if index > start
            && depth == 0
            && xmltable_option_starts_at(tokens, index)
            && parse_b_expression_tokens(tokens[start..index].to_vec()).is_ok()
        {
            return index;
        }
    }
    available_end
}
