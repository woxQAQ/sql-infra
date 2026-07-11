use super::*;

fn parse_stats_params(tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.first().map_or(0, |token| token.location);
    if tokens.is_empty() {
        return Err(ParseError::new(
            location,
            "CREATE STATISTICS requires an ON item",
        ));
    }
    if tokens.last().map(|token| token.kind) == Some(TokenKind::Char(',')) {
        return Err(ParseError::new(
            location,
            "statistics parameter list cannot end with ','",
        ));
    }
    split_top_level_commas(tokens)
        .into_iter()
        .map(|tokens| {
            let item_location = tokens.first().map_or(location, |token| token.location);
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
                let close = find_matching_close(&tokens, 0).ok_or_else(|| {
                    ParseError::new(item_location, "unterminated statistics expression")
                })?;
                if close + 1 != tokens.len() {
                    return Err(ParseError::new(
                        tokens[close + 1].location,
                        "unexpected token after statistics expression",
                    ));
                }
                parse_expression_tokens(tokens[1..close].to_vec())?
            } else {
                let starts_with_cast = tokens.first().map(|token| token.kind) == Some(TokenKind::Cast);
                let expression = parse_expression_tokens(tokens)?;
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

impl Parser {
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
        let stats_tokens =
            self.take_until_top_level(&[TokenKind::From, TokenKind::Char(';'), TokenKind::Eof]);
        let exprs = parse_stats_params(stats_tokens)?;
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
