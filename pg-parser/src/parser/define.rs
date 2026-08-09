//! Definition-style `CREATE` statements and their option lists.
//!
//! PostgreSQL aggregates, operators, types, collations, and text-search objects
//! reuse `DefElem` shapes but retain object-specific name parsing.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis subset — definition-based CREATE commands
    // Sources:
    // - https://www.postgresql.org/docs/18/sql-createaggregate.html
    // - https://www.postgresql.org/docs/18/sql-createoperator.html
    // - https://www.postgresql.org/docs/18/sql-createcollation.html
    //
    // CREATE [ OR REPLACE ] AGGREGATE name ( aggregate_signature ) ( definition )
    // CREATE [ OR REPLACE ] AGGREGATE name ( old_style_definition )
    // CREATE OPERATOR name ( definition )
    // CREATE COLLATION [ IF NOT EXISTS ] name ( definition )
    // CREATE COLLATION [ IF NOT EXISTS ] name FROM existing_collation
    pub(super) fn parse_define(&mut self, kind: ObjectType, replace: bool) -> PResult<Node> {
        self.advance();
        if kind == ObjectType::Operator {
            self.record_completion_tokens(&[TokenKind::Class, TokenKind::Family]);
        }
        if replace && kind != ObjectType::Aggregate {
            return Err(self.error_here("OR REPLACE is only supported for CREATE AGGREGATE here"));
        }
        let mut if_not_exists = false;
        if kind == ObjectType::Collation {
            if_not_exists = self.consume_if_not_exists()?;
        }
        let name_slot = completion::object_type_slot(kind);
        self.record_completion_slot(name_slot);
        let (defnames, args, definition, oldstyle) = if kind == ObjectType::Aggregate {
            self.record_completion_qualified_name_slot(name_slot, &[TokenKind::Char('(')]);
            let defnames = self.parse_name_list();
            if defnames.is_empty() {
                return Err(self.error_here("CREATE AGGREGATE requires a name"));
            }
            self.expect(TokenKind::Char('('))?;
            let first = self.take_until_top_level(&[TokenKind::Char(')')]);
            if self.at_completion() {
                self.record_completion_slot(completion::GrammarSlot::Type);
                let mut active_start = 0usize;
                let mut depth = 0usize;
                let mut saw_order_by = false;
                for (index, token) in first.iter().enumerate() {
                    match token.kind {
                        TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                        TokenKind::Char(')') | TokenKind::Char(']') => {
                            depth = depth.saturating_sub(1)
                        }
                        TokenKind::Order
                            if depth == 0
                                && first.get(index + 1).map(|token| token.kind)
                                    == Some(TokenKind::By) =>
                        {
                            saw_order_by = true;
                            active_start = index + 2;
                        }
                        TokenKind::Char(',') if depth == 0 => active_start = index + 1,
                        _ => {}
                    }
                }
                if !saw_order_by
                    && (first.is_empty() || parse_aggregate_args(first.clone()).is_ok())
                {
                    self.record_completion_phrase(&[TokenKind::Order, TokenKind::By]);
                }
                let mut active_parameter = first[active_start..].to_vec();
                self.append_completion_marker(&mut active_parameter);
                let _ = function_parameter_from_tokens_with_completion(
                    active_parameter,
                    self.completion.clone(),
                );
            }
            self.expect(TokenKind::Char(')'))?;
            let aggregate_args = parse_aggregate_args(first.clone());
            if aggregate_args.is_ok() {
                self.record_completion_tokens(&[TokenKind::Char('(')]);
            }
            if self.at(TokenKind::Char('(')) {
                (
                    defnames,
                    aggregate_args?,
                    self.parse_parenthesized_definition_for(Some(kind))?,
                    false,
                )
            } else {
                (
                    defnames,
                    Vec::new(),
                    parse_old_aggregate_definition(first)?,
                    true,
                )
            }
        } else if kind == ObjectType::Operator {
            self.record_completion_qualified_name_slot(name_slot, &[TokenKind::Char('(')]);
            let operator_location = self.location();
            let tokens = self.take_until_top_level(&[TokenKind::Char('(')]);
            let defnames = parse_operator_name_tokens(tokens, operator_location)?;
            (
                defnames,
                Vec::new(),
                self.parse_parenthesized_definition_for(Some(kind))?,
                false,
            )
        } else {
            let name_stops = [
                TokenKind::Char('('),
                TokenKind::From,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ];
            self.record_completion_qualified_name_slot(name_slot, &name_stops);
            let defnames = self.parse_name_list_until_keywords(&name_stops);
            if defnames.is_empty() {
                return Err(self.error_here("CREATE COLLATION requires a name"));
            }
            let definition = if self.consume(TokenKind::From) {
                let from_location = self.location();
                self.record_completion_slot(completion::GrammarSlot::Collation);
                let from = self.parse_name_list();
                if from.is_empty() {
                    return Err(self.error_here("COLLATION FROM requires a source collation"));
                }
                vec![make_def_elem(
                    "from",
                    Some(name_list_node(from)),
                    from_location,
                )]
            } else {
                self.parse_parenthesized_definition_for(Some(kind))?
            };
            (defnames, Vec::new(), definition, false)
        };
        Ok(Node::DefineStmt(DefineStmt {
            kind,
            oldstyle,
            defnames,
            args,
            definition,
            if_not_exists,
            replace,
        }))
    }
}
