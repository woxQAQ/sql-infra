use super::*;

impl Parser {
    pub(super) fn parse_where_or_current_clause(
        &mut self,
        slot: CompletionSlot,
        stops: &[TokenKind],
    ) -> PResult<Option<Box<Node>>> {
        if !self.consume(TokenKind::Where) {
            return Ok(None);
        }
        if self.consume(TokenKind::CurrentP) {
            self.expect(TokenKind::Of)?;
            let cursor_name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("CURRENT OF requires a cursor name"))?,
            );
            return Ok(Some(Box::new(Node::CurrentOfExpr(CurrentOfExpr {
                xpr: Expr::new(NodeTag::CurrentOfExpr),
                cursor_name,
                ..CurrentOfExpr::default()
            }))));
        }
        Ok(Some(self.parse_expr_box_strict_until_at(slot, stops)?))
    }

    pub(super) fn parse_returning_clause(&mut self) -> PResult<Option<Box<ReturningClause>>> {
        if !self.consume(TokenKind::Returning) {
            return Ok(None);
        }
        let mut options = Vec::new();
        if self.consume(TokenKind::With) {
            self.expect(TokenKind::Char('('))?;
            loop {
                let location = self.location();
                let option = if self.consume(TokenKind::Old) {
                    ReturningOptionKind::Old
                } else if self.consume(TokenKind::New) {
                    ReturningOptionKind::New
                } else {
                    return Err(self.error_here("expected OLD or NEW returning option"));
                };
                self.expect(TokenKind::As)?;
                let value = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("expected a returning option alias"))?;
                options.push(Node::ReturningOption(ReturningOption {
                    node_tag: NodeTag::ReturningOption,
                    option,
                    value: Some(value),
                    location: location as ParseLoc,
                }));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
            self.expect(TokenKind::Char(')'))?;
        }
        let exprs = self.parse_res_target_list_strict_until(
            CompletionSlot::ReturningExpression,
            CompletionSlot::ReturningExpressionAfterComma,
            &[TokenKind::Char(';'), TokenKind::Eof],
        )?;
        if exprs.is_empty() {
            return Err(self.error_here("RETURNING requires at least one expression"));
        }
        Ok(Some(Box::new(ReturningClause {
            node_tag: NodeTag::ReturningClause,
            options,
            exprs,
        })))
    }

    pub(super) fn parse_for_portion_of_clause(
        &mut self,
    ) -> PResult<Option<Box<ForPortionOfClause>>> {
        if self.peek_kind() != TokenKind::For || self.peek_kind_n(1) != TokenKind::Portion {
            return Ok(None);
        }
        self.advance();
        self.advance();
        self.expect(TokenKind::Of)?;
        let location = self.location();
        let range_name = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("expected a range name after FOR PORTION OF"))?;
        if self.consume(TokenKind::Char('(')) {
            let target_location = self.location();
            let target = self.parse_expr_box_strict_until_at(
                CompletionSlot::ForPortionTarget,
                &[TokenKind::Char(')')],
            )?;
            self.expect(TokenKind::Char(')'))?;
            return Ok(Some(Box::new(ForPortionOfClause {
                node_tag: NodeTag::ForPortionOfClause,
                range_name: Some(range_name),
                location: location as ParseLoc,
                target_location: target_location as ParseLoc,
                target: Some(target),
                ..ForPortionOfClause::default()
            })));
        }
        let target_location = self.expect(TokenKind::From)?.location();
        let target_start =
            self.parse_expr_box_strict_until_at(CompletionSlot::ForPortionStart, &[TokenKind::To])?;
        self.expect(TokenKind::To)?;
        let target_end = self.parse_expr_box_strict_until_at(
            CompletionSlot::ForPortionEnd,
            &[
                TokenKind::As,
                TokenKind::Set,
                TokenKind::Using,
                TokenKind::Where,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ],
        )?;
        Ok(Some(Box::new(ForPortionOfClause {
            node_tag: NodeTag::ForPortionOfClause,
            range_name: Some(range_name),
            location: location as ParseLoc,
            target_location: target_location as ParseLoc,
            target_start: Some(target_start),
            target_end: Some(target_end),
            ..ForPortionOfClause::default()
        })))
    }

    pub(super) fn parse_on_conflict_clause(&mut self) -> PResult<Option<Box<OnConflictClause>>> {
        if !self.consume(TokenKind::On) {
            return Ok(None);
        }
        let location = self.previous_location();
        self.expect(TokenKind::Conflict)?;
        let infer = if self.consume(TokenKind::Char('(')) {
            let infer_location = self.previous_location();
            let mut index_elems = Vec::new();
            while !self.at(TokenKind::Char(')')) {
                let range =
                    self.take_until_top_level_range(&[TokenKind::Char(','), TokenKind::Char(')')]);
                let slot = if index_elems.is_empty() {
                    CompletionSlot::OnConflictInferenceElement
                } else {
                    CompletionSlot::OnConflictInferenceElementAfterComma
                };
                index_elems.push(Node::IndexElem(self.parse_index_elem_range(slot, range)?));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected an inference element after ','"));
                }
            }
            if index_elems.is_empty() {
                return Err(self.error_here("ON CONFLICT inference list cannot be empty"));
            }
            self.expect(TokenKind::Char(')'))?;
            let where_clause = if self.consume(TokenKind::Where) {
                Some(self.parse_expr_box_strict_until_at(
                    CompletionSlot::OnConflictInferenceWhere,
                    &[TokenKind::Do],
                )?)
            } else {
                None
            };
            Some(Box::new(InferClause {
                node_tag: NodeTag::InferClause,
                index_elems,
                where_clause,
                location: infer_location as ParseLoc,
                ..InferClause::default()
            }))
        } else if self.consume(TokenKind::On) {
            let infer_location = self.previous_location();
            self.expect(TokenKind::Constraint)?;
            let conname = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a constraint name"))?;
            Some(Box::new(InferClause {
                node_tag: NodeTag::InferClause,
                conname: Some(conname),
                location: infer_location as ParseLoc,
                ..InferClause::default()
            }))
        } else {
            None
        };
        self.expect(TokenKind::Do)?;
        let (action, lock_strength, target_list, where_clause) = match self.peek_kind() {
            TokenKind::Nothing => {
                self.advance();
                (
                    OnConflictAction::Nothing,
                    LockClauseStrength::None,
                    Vec::new(),
                    None,
                )
            }
            TokenKind::Update => {
                self.advance();
                self.expect(TokenKind::Set)?;
                let target_list = self.parse_set_clause_list_until(
                    CompletionSlot::OnConflictSetTarget,
                    CompletionSlot::OnConflictSetTargetAfterComma,
                    CompletionSlot::OnConflictSetValue,
                    &[
                        TokenKind::Where,
                        TokenKind::Returning,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ],
                )?;
                let where_clause = if self.consume(TokenKind::Where) {
                    Some(self.parse_expr_box_strict_until_at(
                        CompletionSlot::OnConflictUpdateWhere,
                        &[TokenKind::Returning, TokenKind::Char(';'), TokenKind::Eof],
                    )?)
                } else {
                    None
                };
                (
                    OnConflictAction::Update,
                    LockClauseStrength::None,
                    target_list,
                    where_clause,
                )
            }
            TokenKind::Select => {
                self.advance();
                let lock_strength = if self.consume(TokenKind::For) {
                    self.parse_locking_strength()?
                } else {
                    LockClauseStrength::None
                };
                let where_clause = if self.consume(TokenKind::Where) {
                    Some(self.parse_expr_box_strict_until_at(
                        CompletionSlot::OnConflictSelectWhere,
                        &[TokenKind::Returning, TokenKind::Char(';'), TokenKind::Eof],
                    )?)
                } else {
                    None
                };
                (
                    OnConflictAction::Select,
                    lock_strength,
                    Vec::new(),
                    where_clause,
                )
            }
            _ => {
                return Err(
                    self.error_here("expected NOTHING, UPDATE, or SELECT after ON CONFLICT DO")
                );
            }
        };
        Ok(Some(Box::new(OnConflictClause {
            node_tag: NodeTag::OnConflictClause,
            action,
            infer,
            lock_strength,
            target_list,
            where_clause,
            location: location as ParseLoc,
        })))
    }

    pub(super) fn parse_set_clause_list_until(
        &mut self,
        first_target_slot: CompletionSlot,
        continuation_target_slot: CompletionSlot,
        value_slot: CompletionSlot,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        if self.at_completion_cursor() {
            self.record_completion_at(
                first_target_slot,
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            );
            return Err(self.completion_stop());
        }
        let mut targets = Vec::new();
        while !self.at_any(stops) {
            if self.consume(TokenKind::Char('(')) {
                let mut names = Vec::new();
                loop {
                    let location = self.location();
                    let name = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("expected an assignment target"))?;
                    let indirection = self.parse_assignment_indirection()?;
                    names.push((name, indirection, location));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                self.expect(TokenKind::Char('='))?;
                let source = self.parse_expr_box_strict_until_at(
                    value_slot,
                    &extend_stops(stops, TokenKind::Char(',')),
                )?;
                let ncolumns = names.len() as i32;
                for (index, (name, indirection, location)) in names.into_iter().enumerate() {
                    targets.push(Node::ResTarget(ResTarget {
                        node_tag: NodeTag::ResTarget,
                        name: Some(name),
                        indirection,
                        val: Some(Box::new(Node::MultiAssignRef(MultiAssignRef {
                            node_tag: NodeTag::MultiAssignRef,
                            source: Some(source.clone()),
                            colno: index as i32 + 1,
                            ncolumns,
                        }))),
                        location: location as ParseLoc,
                    }));
                }
            } else {
                let location = self.location();
                let name = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("expected an assignment target"))?;
                let indirection = self.parse_assignment_indirection()?;
                self.expect(TokenKind::Char('='))?;
                let val = self.parse_expr_box_strict_until_at(
                    value_slot,
                    &extend_stops(stops, TokenKind::Char(',')),
                )?;
                targets.push(Node::ResTarget(ResTarget {
                    node_tag: NodeTag::ResTarget,
                    name: Some(name),
                    indirection,
                    val: Some(val),
                    location: location as ParseLoc,
                }));
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                if self.at_completion_cursor() {
                    self.record_completion_at(
                        continuation_target_slot,
                        Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
                    );
                    return Err(self.completion_stop());
                }
                return Err(self.error_here("expected an assignment after ','"));
            }
        }
        if targets.is_empty() {
            return Err(self.error_here("SET requires at least one assignment"));
        }
        Ok(targets)
    }

    pub(super) fn parse_assignment_indirection(&mut self) -> PResult<NodeList> {
        let mut indirection = Vec::new();
        loop {
            if self.consume(TokenKind::Char('.')) {
                let name = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("expected a field name after '.'"))?;
                indirection.push(make_string_node(name));
            } else if self.consume(TokenKind::Char('[')) {
                let lower_or_index =
                    self.take_until_top_level_range(&[TokenKind::Char(':'), TokenKind::Char(']')]);
                if lower_or_index.is_empty() && self.at_completion_cursor() {
                    self.record_expression_completion_at(
                        CompletionSlot::AssignmentSubscriptLowerOrIndex,
                    );
                }
                let (is_slice, lidx, uidx) = if self.consume(TokenKind::Char(':')) {
                    let upper = self.take_until_top_level_range(&[TokenKind::Char(']')]);
                    if upper.is_empty() && self.at_completion_cursor() {
                        self.record_expression_completion_at(CompletionSlot::AssignmentSliceUpper);
                    }
                    (
                        true,
                        if lower_or_index.is_empty() {
                            None
                        } else {
                            Some(Box::new(self.parse_expression_range_at(
                                CompletionSlot::AssignmentSubscriptLowerOrIndex,
                                lower_or_index,
                            )?))
                        },
                        if upper.is_empty() {
                            None
                        } else {
                            Some(Box::new(self.parse_expression_range_at(
                                CompletionSlot::AssignmentSliceUpper,
                                upper,
                            )?))
                        },
                    )
                } else {
                    if lower_or_index.is_empty() {
                        return Err(self.error_here("assignment subscript cannot be empty"));
                    }
                    (
                        false,
                        None,
                        Some(Box::new(self.parse_expression_range_at(
                            CompletionSlot::AssignmentSubscriptLowerOrIndex,
                            lower_or_index,
                        )?)),
                    )
                };
                self.expect(TokenKind::Char(']'))?;
                indirection.push(Node::AIndices(AIndices {
                    node_tag: NodeTag::AIndices,
                    is_slice,
                    lidx,
                    uidx,
                }));
            } else {
                break;
            }
        }
        Ok(indirection)
    }

    pub(super) fn parse_merge_when_clauses(&mut self) -> PResult<NodeList> {
        let mut clauses = Vec::new();
        while self.consume(TokenKind::When) {
            let match_kind = if self.consume(TokenKind::Matched) {
                MergeMatchKind::Matched
            } else {
                self.expect(TokenKind::Not)?;
                self.expect(TokenKind::Matched)?;
                if self.consume(TokenKind::By) {
                    if self.consume(TokenKind::Source) {
                        MergeMatchKind::NotMatchedBySource
                    } else {
                        self.expect(TokenKind::Target)?;
                        MergeMatchKind::NotMatchedByTarget
                    }
                } else {
                    MergeMatchKind::NotMatchedByTarget
                }
            };
            let condition = if self.consume(TokenKind::And) {
                Some(self.parse_expr_box_strict_until_at(
                    CompletionSlot::MergeWhenCondition,
                    &[TokenKind::Then],
                )?)
            } else {
                None
            };
            self.expect(TokenKind::Then)?;

            let (command_type, override_, target_list, values) = match self.peek_kind() {
                TokenKind::Update => {
                    self.advance();
                    self.expect(TokenKind::Set)?;
                    let target_list = self.parse_set_clause_list_until(
                        CompletionSlot::MergeSetTarget,
                        CompletionSlot::MergeSetTargetAfterComma,
                        CompletionSlot::MergeSetValue,
                        &[
                            TokenKind::When,
                            TokenKind::Returning,
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ],
                    )?;
                    (
                        CmdType::Update,
                        OverridingKind::NotSet,
                        target_list,
                        Vec::new(),
                    )
                }
                TokenKind::DeleteP => {
                    self.advance();
                    (
                        CmdType::Delete,
                        OverridingKind::NotSet,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                TokenKind::Do => {
                    self.advance();
                    self.expect(TokenKind::Nothing)?;
                    (
                        CmdType::Nothing,
                        OverridingKind::NotSet,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                TokenKind::Insert => {
                    self.advance();
                    let (override_, target_list, values) = self.parse_merge_insert_action()?;
                    (CmdType::Insert, override_, target_list, values)
                }
                _ => return Err(self.error_here("expected a MERGE action after THEN")),
            };
            let action_allowed = matches!(
                (match_kind, command_type),
                (
                    MergeMatchKind::Matched | MergeMatchKind::NotMatchedBySource,
                    CmdType::Update | CmdType::Delete | CmdType::Nothing
                ) | (
                    MergeMatchKind::NotMatchedByTarget,
                    CmdType::Insert | CmdType::Nothing
                )
            );
            if !action_allowed {
                return Err(self.error_here("MERGE action is not valid for this match kind"));
            }
            clauses.push(Node::MergeWhenClause(MergeWhenClause {
                node_tag: NodeTag::MergeWhenClause,
                match_kind,
                command_type,
                override_,
                condition,
                target_list,
                values,
            }));
        }
        Ok(clauses)
    }

    fn parse_merge_insert_action(&mut self) -> PResult<(OverridingKind, NodeList, NodeList)> {
        let target_list = if self.consume(TokenKind::Char('(')) {
            let target_list = self.parse_insert_column_list()?;
            self.expect(TokenKind::Char(')'))?;
            target_list
        } else {
            Vec::new()
        };
        let override_ = if self.consume(TokenKind::Overriding) {
            let override_ = if self.consume(TokenKind::User) {
                OverridingKind::UserValue
            } else if self.consume(TokenKind::SystemP) {
                OverridingKind::SystemValue
            } else {
                return Err(self.error_here("expected USER or SYSTEM after OVERRIDING"));
            };
            self.expect(TokenKind::ValueP)?;
            override_
        } else {
            OverridingKind::NotSet
        };
        let values = if self.consume(TokenKind::Default) {
            if !target_list.is_empty() || override_ != OverridingKind::NotSet {
                return Err(self.error_here(
                    "MERGE INSERT DEFAULT VALUES does not accept columns or OVERRIDING",
                ));
            }
            self.expect(TokenKind::Values)?;
            Vec::new()
        } else {
            self.expect(TokenKind::Values)?;
            self.expect(TokenKind::Char('('))?;
            let values = self.parse_expr_list_strict_until_at(
                CompletionSlot::MergeInsertValue,
                CompletionSlot::MergeInsertValueAfterComma,
                &[TokenKind::Char(')')],
            )?;
            if values.is_empty() {
                return Err(self.error_here("MERGE INSERT VALUES cannot be empty"));
            }
            self.expect(TokenKind::Char(')'))?;
            values
        };
        Ok((override_, target_list, values))
    }
}
