use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createstatistics.html
    // CREATE STATISTICS [ [ IF NOT EXISTS ] statistics_name ]
    //     ON ( expression )
    //     FROM table_name
    //
    // CREATE STATISTICS [ [ IF NOT EXISTS ] statistics_name ]
    //     [ ( statistics_kind [, ... ] ) ]
    //     ON { column_name | ( expression ) }, { column_name | ( expression ) } [, ...]
    //     FROM table_name
    pub(super) fn parse_create_stats(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Statistics)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::On,
            TokenKind::From,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if if_not_exists && defnames.is_empty() {
            return Err(self.error_here("IF NOT EXISTS requires a statistics object name"));
        }
        let stat_types = if self.consume(TokenKind::Char('(')) {
            let mut names = Vec::new();
            loop {
                let name = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("expected a statistics kind"))?;
                names.push(make_string_node(name));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
            self.expect(TokenKind::Char(')'))?;
            names
        } else {
            Vec::new()
        };
        self.expect(TokenKind::On)?;
        let stats_range = self.take_until_top_level_range(&[
            TokenKind::From,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let exprs = self.parse_stats_params_range(stats_range)?;
        self.expect(TokenKind::From)?;
        let relations = self.parse_from_clause_until(&[TokenKind::Char(';'), TokenKind::Eof])?;
        if relations.is_empty() {
            return Err(self.error_here("CREATE STATISTICS requires a FROM relation"));
        }
        Ok(Node::CreateStatsStmt(CreateStatsStmt {
            node_tag: NodeTag::CreateStatsStmt,
            defnames,
            stat_types,
            exprs,
            relations,
            if_not_exists,
            ..CreateStatsStmt::default()
        }))
    }

    fn parse_stats_params_range(&self, range: std::ops::Range<usize>) -> PResult<NodeList> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion_at(CompletionSlot::StatisticsExpression);
            }
            return Err(ParseError::new(
                location,
                "CREATE STATISTICS requires an ON item",
            ));
        }
        if self.tokens[range.end - 1].kind == TokenKind::Char(',') {
            if self.at_completion_cursor() {
                self.record_expression_completion_at(
                    CompletionSlot::StatisticsExpressionAfterComma,
                );
            }
            return Err(ParseError::new(
                location,
                "statistics parameter list cannot end with ','",
            ));
        }

        let mut ranges = Vec::new();
        let mut start = range.start;
        let mut depth = 0usize;
        for index in range.clone() {
            match self.tokens[index].kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                TokenKind::Char(',') if depth == 0 => {
                    ranges.push(start..index);
                    start = index + 1;
                }
                _ => {}
            }
        }
        ranges.push(start..range.end);

        ranges
            .into_iter()
            .enumerate()
            .map(|(index, item_range)| {
                let slot = if index == 0 {
                    CompletionSlot::StatisticsExpression
                } else {
                    CompletionSlot::StatisticsExpressionAfterComma
                };
                let tokens = &self.tokens[item_range.clone()];
                let item_location = tokens.first().map_or(location, Token::location);
                if tokens.len() == 1
                    && token_name_in_categories(
                        &tokens[0],
                        &[KeywordCategory::Unreserved, KeywordCategory::ColName],
                    )
                    .is_some()
                {
                    return Ok(Node::StatsElem(StatsElem {
                        node_tag: NodeTag::StatsElem,
                        name: token_name(&tokens[0]),
                        ..StatsElem::default()
                    }));
                }

                let expression = if tokens.first().map(|token| token.kind)
                    == Some(TokenKind::Char('('))
                {
                    let close = find_matching_close(tokens, 0).ok_or_else(|| {
                        ParseError::new(item_location, "unterminated statistics expression")
                    })?;
                    if close + 1 != tokens.len() {
                        return Err(ParseError::ranged(
                            tokens[close + 1].range,
                            "unexpected token after statistics expression",
                        ));
                    }
                    self.parse_expression_range_at(
                        slot,
                        item_range.start + 1..item_range.start + close,
                    )?
                } else {
                    let starts_with_cast =
                        tokens.first().map(|token| token.kind) == Some(TokenKind::Cast);
                    let expression = self.parse_expression_range_at(slot, item_range)?;
                    if !is_windowless_function_expression_node(&expression, starts_with_cast) {
                        return Err(ParseError::new(
                            item_location,
                            "statistics expressions must be parenthesized unless they are function calls",
                        ));
                    }
                    expression
                };
                Ok(Node::StatsElem(StatsElem {
                    node_tag: NodeTag::StatsElem,
                    expr: Some(Box::new(expression)),
                    ..StatsElem::default()
                }))
            })
            .collect()
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterstatistics.html
    // ALTER STATISTICS name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER STATISTICS name RENAME TO new_name
    // ALTER STATISTICS name SET SCHEMA new_schema
    // ALTER STATISTICS name SET STATISTICS { new_target | DEFAULT }
    pub(super) fn parse_alter_stats(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Statistics)?;
        let missing_ok = self.consume_if_exists()?;
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if defnames.is_empty() {
            return Err(self.error_here("ALTER STATISTICS requires a statistics object name"));
        }
        self.expect(TokenKind::Set)?;
        self.expect(TokenKind::Statistics)?;
        let stxstattarget = if self.consume(TokenKind::Default) {
            None
        } else {
            Some(Box::new(self.parse_signed_integer()?))
        };
        self.expect_statement_end()?;
        Ok(Node::AlterStatsStmt(AlterStatsStmt {
            node_tag: NodeTag::AlterStatsStmt,
            defnames,
            stxstattarget,
            missing_ok,
        }))
    }
}
