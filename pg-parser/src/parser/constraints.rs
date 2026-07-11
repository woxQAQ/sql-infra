use super::*;

impl Parser {
    pub(super) fn parse_column_constraint_element(
        &mut self,
        location: usize,
    ) -> PResult<Constraint> {
        let mut constraint = Constraint {
            node_tag: NodeTag::Constraint,
            location: location as ParseLoc,
            ..Constraint::default()
        };
        if self.consume(TokenKind::Not) {
            if self.consume(TokenKind::NullP) {
                constraint.contype = ConstrType::Notnull;
                constraint.is_enforced = true;
                constraint.initially_valid = true;
                if self.consume(TokenKind::No) {
                    self.expect(TokenKind::Inherit)?;
                    constraint.is_no_inherit = true;
                }
            } else if self.consume(TokenKind::Deferrable) {
                constraint.contype = ConstrType::AttrNotDeferrable;
            } else if self.consume(TokenKind::Enforced) {
                constraint.contype = ConstrType::AttrNotEnforced;
            } else {
                return Err(self.error_here("NOT requires NULL, DEFERRABLE, or ENFORCED"));
            }
        } else if self.consume(TokenKind::NullP) {
            constraint.contype = ConstrType::Null;
        } else if self.consume(TokenKind::Unique) {
            constraint.contype = ConstrType::Unique;
            self.parse_unique_null_treatment(&mut constraint)?;
            self.parse_index_constraint_options(&mut constraint)?;
        } else if self.consume(TokenKind::Primary) {
            self.expect(TokenKind::Key)?;
            constraint.contype = ConstrType::Primary;
            self.parse_index_constraint_options(&mut constraint)?;
        } else if self.consume(TokenKind::Check) {
            constraint.contype = ConstrType::Check;
            constraint.is_enforced = true;
            constraint.initially_valid = true;
            self.expect(TokenKind::Char('('))?;
            constraint.raw_expr = Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
            self.expect(TokenKind::Char(')'))?;
            if self.consume(TokenKind::No) {
                self.expect(TokenKind::Inherit)?;
                constraint.is_no_inherit = true;
            }
        } else if self.consume(TokenKind::Default) {
            constraint.contype = ConstrType::Default;
            constraint.raw_expr = Some(self.parse_b_expr_box_strict_until(&[
                TokenKind::Constraint,
                TokenKind::Collate,
                TokenKind::Not,
                TokenKind::NullP,
                TokenKind::Unique,
                TokenKind::Primary,
                TokenKind::Check,
                TokenKind::Generated,
                TokenKind::References,
                TokenKind::Deferrable,
                TokenKind::Initially,
                TokenKind::Enforced,
                TokenKind::Eof,
            ])?);
        } else if self.consume(TokenKind::Generated) {
            let generated_when = if self.consume(TokenKind::Always) {
                b'a'
            } else if self.consume(TokenKind::By) {
                self.expect(TokenKind::Default)?;
                b'd'
            } else {
                return Err(self.error_here("GENERATED requires ALWAYS or BY DEFAULT"));
            };
            self.expect(TokenKind::As)?;
            constraint.generated_when = generated_when;
            if self.consume(TokenKind::IdentityP) {
                constraint.contype = ConstrType::Identity;
                if self.consume(TokenKind::Char('(')) {
                    constraint.options = self.parse_parenthesized_sequence_options_body()?;
                }
            } else {
                if generated_when != b'a' {
                    return Err(self.error_here("generated columns require GENERATED ALWAYS"));
                }
                constraint.contype = ConstrType::Generated;
                self.expect(TokenKind::Char('('))?;
                constraint.raw_expr =
                    Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
                self.expect(TokenKind::Char(')'))?;
                constraint.generated_kind = if self.consume(TokenKind::Stored) {
                    b's'
                } else {
                    self.consume(TokenKind::Virtual);
                    b'v'
                };
            }
        } else if self.consume(TokenKind::References) {
            constraint.contype = ConstrType::Foreign;
            constraint.is_enforced = true;
            constraint.initially_valid = true;
            self.parse_references_clause(&mut constraint, false)?;
        } else if self.consume(TokenKind::Deferrable) {
            constraint.contype = ConstrType::AttrDeferrable;
        } else if self.consume(TokenKind::Initially) {
            constraint.contype = if self.consume(TokenKind::Deferred) {
                ConstrType::AttrDeferred
            } else {
                self.expect(TokenKind::Immediate)?;
                ConstrType::AttrImmediate
            };
        } else if self.consume(TokenKind::Enforced) {
            constraint.contype = ConstrType::AttrEnforced;
        } else {
            return Err(self.error_here("invalid column constraint"));
        }
        Ok(constraint)
    }

    pub(super) fn parse_table_constraint(&mut self) -> PResult<Constraint> {
        let location = self.location();
        let conname = if self.consume(TokenKind::Constraint) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?,
            )
        } else {
            None
        };
        let mut constraint = Constraint {
            node_tag: NodeTag::Constraint,
            conname,
            location: location as ParseLoc,
            ..Constraint::default()
        };
        if self.consume(TokenKind::Check) {
            constraint.contype = ConstrType::Check;
            constraint.is_enforced = true;
            self.expect(TokenKind::Char('('))?;
            constraint.raw_expr = Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
            self.expect(TokenKind::Char(')'))?;
        } else if self.consume(TokenKind::Not) {
            self.expect(TokenKind::NullP)?;
            constraint.contype = ConstrType::Notnull;
            constraint.keys = vec![make_string_node(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("NOT NULL requires a column name"))?,
            )];
        } else if self.consume(TokenKind::Unique) {
            constraint.contype = ConstrType::Unique;
            let has_null_treatment = self.at(TokenKind::NullsP);
            self.parse_unique_null_treatment(&mut constraint)?;
            if has_null_treatment
                && self.at(TokenKind::Using)
                && self.peek_kind_n(1) == TokenKind::Index
            {
                return Err(
                    self.error_here("NULLS DISTINCT/NOT DISTINCT is not allowed with USING INDEX")
                );
            }
            self.parse_table_index_constraint(&mut constraint)?;
        } else if self.consume(TokenKind::Primary) {
            self.expect(TokenKind::Key)?;
            constraint.contype = ConstrType::Primary;
            self.parse_table_index_constraint(&mut constraint)?;
        } else if self.consume(TokenKind::Foreign) {
            self.expect(TokenKind::Key)?;
            constraint.contype = ConstrType::Foreign;
            constraint.is_enforced = true;
            self.expect(TokenKind::Char('('))?;
            (constraint.fk_attrs, constraint.fk_with_period) =
                self.parse_column_and_period_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            self.expect(TokenKind::References)?;
            self.parse_references_clause(&mut constraint, true)?;
        } else if self.consume(TokenKind::Exclude) {
            constraint.contype = ConstrType::Exclusion;
            if self.consume(TokenKind::Using) {
                constraint.access_method = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("USING requires an access method"))?,
                );
            } else {
                constraint.access_method = Some("btree".to_owned());
            }
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("EXCLUDE requires at least one element"));
            }
            loop {
                let expr_tokens = self.take_until_top_level(&[TokenKind::With]);
                let expr_location = expr_tokens
                    .first()
                    .map_or(self.location(), |token| token.location);
                let starts_parenthesized =
                    expr_tokens.first().map(|token| token.kind) == Some(TokenKind::Char('('));
                let starts_with_cast =
                    expr_tokens.first().map(|token| token.kind) == Some(TokenKind::Cast);
                let index_elem = parse_index_elem_tokens(expr_tokens)?;
                if let Some(expression) = index_elem.expr.as_deref()
                    && !starts_parenthesized
                    && !is_windowless_function_expression_node(expression, starts_with_cast)
                {
                    return Err(ParseError::new(
                        expr_location,
                        "exclusion expressions must be parenthesized unless they are function calls",
                    ));
                }
                self.expect(TokenKind::With)?;
                let operator_location = self.location();
                let operator_tokens = if self.consume(TokenKind::Operator) {
                    self.expect(TokenKind::Char('('))?;
                    let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
                    self.expect(TokenKind::Char(')'))?;
                    tokens
                } else {
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')])
                };
                if operator_tokens.is_empty() {
                    return Err(self.error_here("EXCLUDE element requires an operator"));
                }
                validate_operator_name_tokens(&operator_tokens, operator_location)?;
                let operator = name_list_node(parse_operator_name_tokens(
                    operator_tokens,
                    operator_location,
                )?);
                constraint.exclusions.push(Node::AArrayExpr(AArrayExpr {
                    node_tag: NodeTag::AArrayExpr,
                    elements: vec![Node::IndexElem(index_elem), operator],
                    ..AArrayExpr::default()
                }));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected an EXCLUDE element after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
            if self.consume(TokenKind::Include) {
                self.expect(TokenKind::Char('('))?;
                constraint.including = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
            }
            self.parse_index_constraint_options(&mut constraint)?;
            if self.consume(TokenKind::Where) {
                self.expect(TokenKind::Char('('))?;
                constraint.where_clause =
                    Some(self.parse_expr_box_strict_until(&[TokenKind::Char(')')])?);
                self.expect(TokenKind::Char(')'))?;
            }
        } else {
            return Err(self.error_here("invalid table constraint"));
        }
        self.parse_constraint_attribute_spec(&mut constraint)?;
        constraint.initially_valid = !constraint.skip_validation;
        Ok(constraint)
    }

    pub(super) fn parse_unique_null_treatment(
        &mut self,
        constraint: &mut Constraint,
    ) -> PResult<()> {
        if self.consume(TokenKind::NullsP) {
            constraint.nulls_not_distinct = self.consume(TokenKind::Not);
            self.expect(TokenKind::Distinct)?;
        }
        Ok(())
    }

    pub(super) fn parse_table_index_constraint(
        &mut self,
        constraint: &mut Constraint,
    ) -> PResult<()> {
        if self.consume(TokenKind::Using) {
            self.expect(TokenKind::Index)?;
            constraint.indexname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING INDEX requires an index name"))?,
            );
            return Ok(());
        }
        self.expect(TokenKind::Char('('))?;
        constraint.keys = self.parse_parenthesized_name_list_body()?;
        if self.consume(TokenKind::Without) {
            self.expect(TokenKind::Overlaps)?;
            constraint.without_overlaps = true;
        }
        self.expect(TokenKind::Char(')'))?;
        if self.consume(TokenKind::Include) {
            self.expect(TokenKind::Char('('))?;
            constraint.including = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
        }
        self.parse_index_constraint_options(constraint)
    }

    pub(super) fn parse_index_constraint_options(
        &mut self,
        constraint: &mut Constraint,
    ) -> PResult<()> {
        if self.consume(TokenKind::With) {
            constraint.options = self.parse_parenthesized_reloptions()?;
        }
        if self.consume(TokenKind::Using) {
            self.expect(TokenKind::Index)?;
            self.expect(TokenKind::Tablespace)?;
            constraint.indexspace = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?,
            );
        }
        Ok(())
    }

    pub(super) fn parse_references_clause(
        &mut self,
        constraint: &mut Constraint,
        allow_period: bool,
    ) -> PResult<()> {
        constraint.pktable = Some(Box::new(
            self.try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("REFERENCES requires a table name"))?,
        ));
        if self.consume(TokenKind::Char('(')) {
            if allow_period {
                (constraint.pk_attrs, constraint.pk_with_period) =
                    self.parse_column_and_period_list_body()?;
            } else {
                constraint.pk_attrs = self.parse_parenthesized_name_list_body()?;
            }
            self.expect(TokenKind::Char(')'))?;
        }
        constraint.fk_matchtype = if self.consume(TokenKind::Match) {
            if self.consume(TokenKind::Full) {
                b'f'
            } else if self.consume(TokenKind::Partial) {
                return Err(self.error_here("MATCH PARTIAL is not implemented by PostgreSQL"));
            } else {
                self.expect(TokenKind::Simple)?;
                b's'
            }
        } else {
            b's'
        };
        constraint.fk_upd_action = b'a';
        constraint.fk_del_action = b'a';
        let mut update_seen = false;
        let mut delete_seen = false;
        for _ in 0..2 {
            if !self.consume(TokenKind::On) {
                break;
            }
            let is_update = if self.consume(TokenKind::Update) {
                true
            } else {
                self.expect(TokenKind::DeleteP)?;
                false
            };
            let (action, cols) = self.parse_foreign_key_action()?;
            if is_update {
                if update_seen {
                    return Err(self.error_here("multiple ON UPDATE clauses are not allowed"));
                }
                update_seen = true;
                if !cols.is_empty() {
                    return Err(self.error_here("ON UPDATE SET columns are not supported"));
                }
                constraint.fk_upd_action = action;
            } else {
                if delete_seen {
                    return Err(self.error_here("multiple ON DELETE clauses are not allowed"));
                }
                delete_seen = true;
                constraint.fk_del_action = action;
                constraint.fk_del_set_cols = cols;
            }
        }
        Ok(())
    }

    pub(super) fn parse_column_and_period_list_body(&mut self) -> PResult<(NodeList, bool)> {
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("foreign key column list cannot be empty"));
        }
        let mut columns = Vec::new();
        let mut with_period = false;
        loop {
            columns.push(make_string_node(self.consume_col_id().ok_or_else(
                || self.error_here("expected a foreign key column name"),
            )?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Period)
                && self.peek_kind_n(1) != TokenKind::Char(')')
                && self.peek_kind_n(2) == TokenKind::Char(')')
            {
                self.advance();
                columns.push(make_string_node(self.consume_col_id().ok_or_else(
                    || self.error_here("PERIOD requires a foreign key column name"),
                )?));
                with_period = true;
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a foreign key column after ','"));
            }
        }
        Ok((columns, with_period))
    }

    pub(super) fn parse_foreign_key_action(&mut self) -> PResult<(u8, NodeList)> {
        if self.consume(TokenKind::No) {
            self.expect(TokenKind::Action)?;
            return Ok((b'a', Vec::new()));
        }
        if self.consume(TokenKind::Restrict) {
            return Ok((b'r', Vec::new()));
        }
        if self.consume(TokenKind::Cascade) {
            return Ok((b'c', Vec::new()));
        }
        if self.consume(TokenKind::Set) {
            let action = if self.consume(TokenKind::NullP) {
                b'n'
            } else {
                self.expect(TokenKind::Default)?;
                b'd'
            };
            let cols = if self.consume(TokenKind::Char('(')) {
                let cols = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                cols
            } else {
                Vec::new()
            };
            return Ok((action, cols));
        }
        Err(self.error_here("invalid foreign key action"))
    }

    pub(super) fn parse_constraint_attribute_spec(
        &mut self,
        constraint: &mut Constraint,
    ) -> PResult<()> {
        let supports_deferrable = matches!(
            constraint.contype,
            ConstrType::Unique | ConstrType::Primary | ConstrType::Exclusion | ConstrType::Foreign
        );
        let supports_enforcement =
            matches!(constraint.contype, ConstrType::Check | ConstrType::Foreign);
        let supports_not_valid = matches!(
            constraint.contype,
            ConstrType::Check | ConstrType::Notnull | ConstrType::Foreign
        );
        let supports_no_inherit =
            matches!(constraint.contype, ConstrType::Check | ConstrType::Notnull);
        let mut saw_deferrable = None;
        let mut saw_initially = None;
        let mut saw_enforced = None;
        while !self.at_any(&[TokenKind::Char(','), TokenKind::Char(';'), TokenKind::Eof]) {
            if self.consume(TokenKind::Deferrable) {
                if !supports_deferrable {
                    return Err(self.error_here("this constraint cannot be marked DEFERRABLE"));
                }
                if saw_deferrable == Some(false) {
                    return Err(self.error_here("conflicting constraint properties"));
                }
                saw_deferrable = Some(true);
                constraint.deferrable = true;
            } else if self.consume(TokenKind::Initially) {
                if !supports_deferrable {
                    return Err(self.error_here("this constraint cannot be marked DEFERRABLE"));
                }
                let deferred = if self.consume(TokenKind::Deferred) {
                    true
                } else {
                    self.expect(TokenKind::Immediate)?;
                    false
                };
                if saw_initially.is_some_and(|previous| previous != deferred) {
                    return Err(self.error_here("conflicting constraint properties"));
                }
                saw_initially = Some(deferred);
                if deferred {
                    if saw_deferrable == Some(false) {
                        return Err(self.error_here(
                            "constraint declared INITIALLY DEFERRED must be DEFERRABLE",
                        ));
                    }
                    constraint.deferrable = true;
                    constraint.initdeferred = true;
                } else {
                    constraint.initdeferred = false;
                }
            } else if self.consume(TokenKind::Enforced) {
                if !supports_enforcement {
                    return Err(self.error_here("this constraint cannot be marked ENFORCED"));
                }
                if saw_enforced == Some(false) {
                    return Err(self.error_here("conflicting constraint properties"));
                }
                saw_enforced = Some(true);
                constraint.is_enforced = true;
            } else if self.consume(TokenKind::Not) {
                if self.consume(TokenKind::Deferrable) {
                    if !supports_deferrable {
                        return Err(self.error_here("this constraint cannot be marked DEFERRABLE"));
                    }
                    if saw_deferrable == Some(true) {
                        return Err(self.error_here("conflicting constraint properties"));
                    }
                    if saw_initially == Some(true) {
                        return Err(self.error_here(
                            "constraint declared INITIALLY DEFERRED must be DEFERRABLE",
                        ));
                    }
                    saw_deferrable = Some(false);
                    constraint.deferrable = false;
                } else if self.consume(TokenKind::Valid) {
                    if !supports_not_valid {
                        return Err(self.error_here("this constraint cannot be marked NOT VALID"));
                    }
                    constraint.skip_validation = true;
                } else if self.consume(TokenKind::Enforced) {
                    if !supports_enforcement {
                        return Err(
                            self.error_here("this constraint cannot be marked NOT ENFORCED")
                        );
                    }
                    if saw_enforced == Some(true) {
                        return Err(self.error_here("conflicting constraint properties"));
                    }
                    saw_enforced = Some(false);
                    constraint.is_enforced = false;
                    constraint.skip_validation = true;
                } else {
                    return Err(self.error_here("invalid constraint attribute after NOT"));
                }
            } else if self.consume(TokenKind::No) {
                self.expect(TokenKind::Inherit)?;
                if !supports_no_inherit {
                    return Err(self.error_here("this constraint cannot be marked NO INHERIT"));
                }
                constraint.is_no_inherit = true;
            } else {
                return Err(self.error_here("invalid constraint attribute"));
            }
        }
        Ok(())
    }
}
