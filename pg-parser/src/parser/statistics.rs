//! Extended-statistics creation and alteration.
//!
//! Statistics targets, expression/column elements, kinds, options, and ownership
//! actions are parsed into their dedicated raw statements.

use super::*;

fn parse_stats_params_with_completion(
    tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<NodeList> {
    let offset = tokens.first().offset_or(0);
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            offset,
            "CREATE STATISTICS requires an ON item",
        ));
    }
    if tokens.last().has_kind(TokenKind::Char(',')) {
        return Err(ParseError::syntax_exit(
            offset,
            "statistics parameter list cannot end with ','",
        ));
    }
    split_top_level_commas(tokens)
        .into_iter()
        .map(|tokens| {
            let item_offset = tokens.first().offset_or(offset);
            if tokens.len() == 1
                && token_name_in_categories(
                    &tokens[0],
                    &[KeywordCategory::Unreserved, KeywordCategory::ColName],
                )
                .is_some()
            {
                return Ok(node!(StatsElem {
                    name: token_name(&tokens[0]),
                    ..StatsElem::default()
                }));
            }

            let expression = if tokens.first().has_kind(TokenKind::Char('('))
            {
                let close = match find_matching_close(&tokens, 0) {
                    Some(close) => close,
                    None
                        if tokens.last().has_kind(TokenKind::Completion) =>
                    {
                        tokens.len()
                    }
                    None => {
                        return Err(ParseError::syntax_exit(
                            item_offset,
                            "unterminated statistics expression",
                        ));
                    }
                };
                if close == tokens.len() {
                    return parse_expression_tokens_with_completion(
                        tokens[1..].to_vec(),
                        completion.clone(),
                    )
                    .map(|expression| {
                        node!(StatsElem {
                            expr: Some(Box::new(expression)),
                            ..StatsElem::default()
                        })
                    });
                }
                if close + 1 != tokens.len() {
                    return Err(ParseError::at_loc(
                        tokens[close + 1].loc,
                        "unexpected token after statistics expression",
                    ));
                }
                parse_expression_tokens_with_completion(
                    tokens[1..close].to_vec(),
                    completion.clone(),
                )?
            } else {
                let starts_with_cast = tokens.first().has_kind(TokenKind::Cast);
                let expression =
                    parse_expression_tokens_with_completion(tokens, completion.clone())?;
                if !is_windowless_function_expression_node(&expression, starts_with_cast) {
                    return Err(ParseError::syntax_exit(
                        item_offset,
                        "statistics expressions must be parenthesized unless they are function calls",
                    ));
                }
                expression
            };
            Ok(node!(StatsElem {
                expr: Some(Box::new(expression)),
                ..StatsElem::default()
            }))
        })
        .collect()
}

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
        let name_stops = [
            TokenKind::Char('('),
            TokenKind::On,
            TokenKind::From,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        self.record_completion_slot(GrammarSlot::Statistics);
        self.record_completion_qualified_name_slot(GrammarSlot::Statistics, &name_stops);
        let defnames = self.parse_name_list_until_keywords(&name_stops);
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
        let mut stats_tokens =
            self.take_until_top_level(&[TokenKind::From, TokenKind::Char(';'), TokenKind::Eof]);
        self.request_completion_membership_recovery();
        if tokens_end_at_top_level(&stats_tokens) {
            self.record_completion_tokens(&[TokenKind::From]);
        }
        self.append_completion_marker(&mut stats_tokens);
        let exprs = parse_stats_params_with_completion(stats_tokens, self.completion.clone())?;
        self.expect(TokenKind::From)?;
        let owner_start = self.pos;
        let relation = self.parse_relation_expr_with_slot(GrammarSlot::Table)?;
        let owner_end = self.pos;
        self.push_completion_membership_owner_from_tokens(
            &[GrammarSlot::Column],
            &[
                ObjectType::Table,
                ObjectType::View,
                ObjectType::Matview,
                ObjectType::ForeignTable,
            ],
            owner_start,
            owner_end,
        );
        self.expect_statement_end()?;
        let relations = vec![Node::RangeVar(relation)];
        Ok(node!(CreateStatsStmt {
            defnames,
            stat_types,
            exprs,
            relations,
            if_not_exists,
            ..CreateStatsStmt::default()
        }))
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
        let name_stops = [TokenKind::Set, TokenKind::Char(';'), TokenKind::Eof];
        self.record_completion_slot(GrammarSlot::Statistics);
        self.record_completion_qualified_name_slot(GrammarSlot::Statistics, &name_stops);
        let defnames = self.parse_name_list_until_keywords_allow_initial_stop(&name_stops);
        if defnames.is_empty() {
            return Err(self.error_here("ALTER STATISTICS requires a statistics object name"));
        }
        self.record_completion_tokens(&[TokenKind::Rename, TokenKind::Owner]);
        self.expect(TokenKind::Set)?;
        self.record_completion_tokens(&[TokenKind::Schema, TokenKind::Statistics]);
        self.expect(TokenKind::Statistics)?;
        let stxstattarget = if self.consume(TokenKind::Default) {
            None
        } else {
            Some(Box::new(self.parse_signed_integer()?))
        };
        self.expect_statement_end()?;
        Ok(node!(AlterStatsStmt {
            defnames,
            stxstattarget,
            missing_ok,
        }))
    }
}
