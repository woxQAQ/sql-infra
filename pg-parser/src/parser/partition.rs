//! Table partition specifications and partition bounds.
//!
//! Range, list, hash, default, and multi-column bound forms are normalized into
//! PostgreSQL partition AST nodes.

use super::*;

impl Parser {
    pub(super) fn parse_partition_spec(&mut self) -> PResult<PartitionSpec> {
        let location = self.expect(TokenKind::Partition)?.location();
        self.expect(TokenKind::By)?;
        let strategy_name = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected a partition strategy"))?;
        let strategy = match strategy_name.to_ascii_lowercase().as_str() {
            "list" => PartitionStrategy::List,
            "range" => PartitionStrategy::Range,
            "hash" => PartitionStrategy::Hash,
            _ => return Err(self.error_here("partition strategy must be LIST, RANGE, or HASH")),
        };
        self.expect(TokenKind::Char('('))?;
        let mut part_params = Vec::new();
        while !self.at(TokenKind::Char(')')) {
            let elem_location = self.location();
            let mut tokens = self.take_until_top_level(COMMA_OR_CLOSE_PAREN_TOKENS);
            if tokens_end_at_top_level(&tokens)
                && parse_index_elem_tokens_with_completion(tokens.clone(), None).is_ok()
            {
                self.record_completion_tokens(COMMA_OR_CLOSE_PAREN_TOKENS);
            }
            self.append_completion_marker(&mut tokens);
            let starts_parenthesized = tokens.first().has_kind(TokenKind::Char('('));
            let starts_with_cast = tokens.first().has_kind(TokenKind::Cast);
            let parsed = parse_index_elem_tokens_with_completion(tokens, self.completion.clone())?;
            if !parsed.opclassopts.is_empty()
                || parsed.ordering != SortByDir::Default
                || parsed.nulls_ordering != SortByNulls::Default
            {
                return Err(self.error_here(
                    "partition keys do not support opclass options, ordering, or NULLS ordering",
                ));
            }
            if let Some(expression) = parsed.expr.as_deref()
                && !starts_parenthesized
                && !is_windowless_function_expression_node(expression, starts_with_cast)
            {
                return Err(ParseError::syntax_exit(
                    elem_location,
                    "partition expressions must be parenthesized unless they are function calls",
                ));
            }
            let elem = PartitionElem {
                name: parsed.name,
                expr: parsed.expr,
                collation: parsed.collation,
                opclass: parsed.opclass,
                location: elem_location as ParseLoc,
            };
            part_params.push(Node::PartitionElem(elem));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a partition key after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        if part_params.is_empty() {
            return Err(self.error_here("partition key cannot be empty"));
        }
        Ok(PartitionSpec {
            strategy,
            part_params,
            location: location as ParseLoc,
        })
    }

    fn parse_partition_range_datums(&mut self, empty_error: &str) -> PResult<NodeList> {
        let mut datums = Vec::new();
        while self.at_completion() || !self.at(TokenKind::Char(')')) {
            let location = self.location();
            if self.consume(TokenKind::Minvalue) {
                datums.push(node!(PartitionRangeDatum {
                    kind: PartitionRangeDatumKind::Minvalue,
                    value: None,
                    location: location as ParseLoc,
                }));
            } else if self.consume(TokenKind::Maxvalue) {
                datums.push(node!(PartitionRangeDatum {
                    kind: PartitionRangeDatumKind::Maxvalue,
                    value: None,
                    location: location as ParseLoc,
                }));
            } else {
                let mut tokens = self.take_until_top_level(COMMA_OR_CLOSE_PAREN_TOKENS);
                self.record_expression_follow_tokens(&tokens, COMMA_OR_CLOSE_PAREN_TOKENS, false);
                self.append_completion_marker(&mut tokens);
                if tokens.is_empty() {
                    return Err(self.error_here("expected a partition bound value"));
                }
                let value =
                    parse_expression_tokens_with_completion(tokens, self.completion.clone())?;
                datums.push(node!(PartitionRangeDatum {
                    kind: PartitionRangeDatumKind::Value,
                    value: Some(Box::new(value)),
                    location: location as ParseLoc,
                }));
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a partition bound value after ','"));
            }
        }
        if datums.is_empty() {
            return Err(self.error_here(empty_error));
        }
        Ok(datums)
    }

    pub(super) fn parse_partition_bound(&mut self) -> PResult<PartitionBoundSpec> {
        let location = self.location();
        if self.consume(TokenKind::Default) {
            return Ok(PartitionBoundSpec {
                is_default: true,
                location: location as ParseLoc,
                ..PartitionBoundSpec::default()
            });
        }
        self.expect(TokenKind::For)?;
        self.expect(TokenKind::Values)?;
        if self.consume(TokenKind::InP) {
            let location = self.previous_location();
            self.expect(TokenKind::Char('('))?;
            let listdatums = self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?;
            if listdatums.is_empty() {
                return Err(self.error_here("list partition bound cannot be empty"));
            }
            self.expect(TokenKind::Char(')'))?;
            return Ok(PartitionBoundSpec {
                strategy: b'l',
                listdatums,
                location: location as ParseLoc,
                ..PartitionBoundSpec::default()
            });
        }
        if self.consume(TokenKind::From) {
            let location = self.previous_location();
            self.expect(TokenKind::Char('('))?;
            let lowerdatums =
                self.parse_partition_range_datums("range partition lower bound cannot be empty")?;
            self.expect(TokenKind::Char(')'))?;
            self.expect(TokenKind::To)?;
            self.expect(TokenKind::Char('('))?;
            let upperdatums =
                self.parse_partition_range_datums("range partition upper bound cannot be empty")?;
            self.expect(TokenKind::Char(')'))?;
            return Ok(PartitionBoundSpec {
                strategy: b'r',
                lowerdatums,
                upperdatums,
                location: location as ParseLoc,
                ..PartitionBoundSpec::default()
            });
        }
        let location = self.expect(TokenKind::With)?.location();
        self.expect(TokenKind::Char('('))?;
        let mut modulus = None;
        let mut remainder = None;
        loop {
            let name = self
                .consume_non_reserved_word()
                .ok_or_else(|| self.error_here("expected MODULUS or REMAINDER"))?;
            let value = match self.advance().value {
                Some(TokenValue::Integer(value)) => value,
                _ => return Err(self.error_here("expected an integer partition bound")),
            };
            match name.as_str() {
                "modulus" => {
                    if modulus.replace(value).is_some() {
                        return Err(self.error_here("MODULUS specified more than once"));
                    }
                }
                "remainder" => {
                    if remainder.replace(value).is_some() {
                        return Err(self.error_here("REMAINDER specified more than once"));
                    }
                }
                _ => return Err(self.error_here("expected MODULUS or REMAINDER")),
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(PartitionBoundSpec {
            strategy: b'h',
            modulus: modulus.ok_or_else(|| self.error_here("missing MODULUS"))?,
            remainder: remainder.ok_or_else(|| self.error_here("missing REMAINDER"))?,
            location: location as ParseLoc,
            ..PartitionBoundSpec::default()
        })
    }
}
