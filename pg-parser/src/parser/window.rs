use super::*;

impl Parser {
    pub(super) fn parse_window_clause_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut windows = Vec::new();
        while !self.at_any(stops) {
            let name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("WINDOW requires a name"))?,
            );
            self.expect(TokenKind::As)?;
            let location = self.expect(TokenKind::Char('('))?.location();
            let mut window = self.parse_window_specification_body(location)?;
            window.name = name;
            windows.push(Node::WindowDef(window));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a window definition after ','"));
            }
        }
        if windows.is_empty() {
            return Err(self.error_here("WINDOW requires at least one definition"));
        }
        Ok(windows)
    }

    pub(super) fn parse_window_specification_body(
        &mut self,
        location: usize,
    ) -> PResult<WindowDef> {
        let mut window = WindowDef {
            node_tag: NodeTag::WindowDef,
            location: location as ParseLoc,
            frame_options: FRAMEOPTION_DEFAULTS,
            ..WindowDef::default()
        };
        if !self.at_any(&[
            TokenKind::Partition,
            TokenKind::Order,
            TokenKind::Rows,
            TokenKind::Range,
            TokenKind::Groups,
            TokenKind::Char(')'),
        ]) {
            window.refname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("invalid referenced window name"))?,
            );
        }
        if self.consume(TokenKind::Partition) {
            self.expect(TokenKind::By)?;
            window.partition_clause = self.parse_expr_list_strict_until(&[
                TokenKind::Order,
                TokenKind::Rows,
                TokenKind::Range,
                TokenKind::Groups,
                TokenKind::Char(')'),
            ])?;
            if window.partition_clause.is_empty() {
                return Err(self.error_here("PARTITION BY requires an expression"));
            }
        }
        if self.consume(TokenKind::Order) {
            self.expect(TokenKind::By)?;
            window.order_clause = self.parse_sort_list_strict_until(&[
                TokenKind::Rows,
                TokenKind::Range,
                TokenKind::Groups,
                TokenKind::Char(')'),
            ])?;
            if window.order_clause.is_empty() {
                return Err(self.error_here("ORDER BY requires a sort expression"));
            }
        }
        if matches!(
            self.peek_kind(),
            TokenKind::Rows | TokenKind::Range | TokenKind::Groups
        ) {
            self.parse_window_frame(&mut window)?;
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(window)
    }

    pub(super) fn parse_window_frame(&mut self, window: &mut WindowDef) -> PResult<()> {
        let frame_mode = match self.peek_kind() {
            TokenKind::Rows => FRAMEOPTION_ROWS,
            TokenKind::Range => FRAMEOPTION_RANGE,
            TokenKind::Groups => FRAMEOPTION_GROUPS,
            _ => return Err(self.error_here("expected ROWS, RANGE, or GROUPS")),
        };
        self.advance();
        window.frame_options = FRAMEOPTION_NONDEFAULT | frame_mode;
        if self.consume(TokenKind::Between) {
            let (start_options, start_offset) = self.parse_window_frame_bound()?;
            self.expect(TokenKind::And)?;
            let (end_start_options, end_offset) = self.parse_window_frame_bound()?;
            let frame_options = start_options | (end_start_options << 1) | FRAMEOPTION_BETWEEN;
            if frame_options & FRAMEOPTION_START_UNBOUNDED_FOLLOWING != 0
                || frame_options & FRAMEOPTION_END_UNBOUNDED_PRECEDING != 0
                || (frame_options & FRAMEOPTION_START_CURRENT_ROW != 0
                    && frame_options & FRAMEOPTION_END_OFFSET_PRECEDING != 0)
                || (frame_options & FRAMEOPTION_START_OFFSET_FOLLOWING != 0
                    && frame_options
                        & (FRAMEOPTION_END_OFFSET_PRECEDING | FRAMEOPTION_END_CURRENT_ROW)
                        != 0)
            {
                return Err(self.error_here("invalid window frame bounds"));
            }
            window.frame_options |= frame_options;
            window.start_offset = start_offset;
            window.end_offset = end_offset;
        } else {
            let (start_options, start_offset) = self.parse_window_frame_bound()?;
            if start_options
                & (FRAMEOPTION_START_UNBOUNDED_FOLLOWING | FRAMEOPTION_START_OFFSET_FOLLOWING)
                != 0
            {
                return Err(self.error_here("invalid single-bound window frame"));
            }
            window.frame_options |= start_options | FRAMEOPTION_END_CURRENT_ROW;
            window.start_offset = start_offset;
        }
        if self.consume(TokenKind::Exclude) {
            window.frame_options |= match self.peek_kind() {
                TokenKind::CurrentP => {
                    self.advance();
                    self.expect(TokenKind::Row)?;
                    FRAMEOPTION_EXCLUDE_CURRENT_ROW
                }
                TokenKind::GroupP => {
                    self.advance();
                    FRAMEOPTION_EXCLUDE_GROUP
                }
                TokenKind::Ties => {
                    self.advance();
                    FRAMEOPTION_EXCLUDE_TIES
                }
                TokenKind::No => {
                    self.advance();
                    self.expect(TokenKind::Others)?;
                    0
                }
                _ => return Err(self.error_here("invalid window frame exclusion")),
            };
        }
        Ok(())
    }

    pub(super) fn parse_window_frame_bound(&mut self) -> PResult<(i32, Option<Box<Node>>)> {
        if self.consume(TokenKind::Unbounded) {
            if self.consume(TokenKind::Preceding) {
                return Ok((FRAMEOPTION_START_UNBOUNDED_PRECEDING, None));
            }
            self.expect(TokenKind::Following)?;
            return Ok((FRAMEOPTION_START_UNBOUNDED_FOLLOWING, None));
        }
        if self.consume(TokenKind::CurrentP) {
            self.expect(TokenKind::Row)?;
            return Ok((FRAMEOPTION_START_CURRENT_ROW, None));
        }
        let offset =
            self.parse_expr_box_strict_until(&[TokenKind::Preceding, TokenKind::Following])?;
        if self.consume(TokenKind::Preceding) {
            Ok((FRAMEOPTION_START_OFFSET_PRECEDING, Some(offset)))
        } else {
            self.expect(TokenKind::Following)?;
            Ok((FRAMEOPTION_START_OFFSET_FOLLOWING, Some(offset)))
        }
    }

    pub(super) fn parse_locking_clause_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut clauses = Vec::new();
        while self.consume(TokenKind::For) {
            if self.consume(TokenKind::Read) {
                self.expect(TokenKind::Only)?;
                if !clauses.is_empty() {
                    return Err(self
                        .error_here("FOR READ ONLY cannot be combined with row-locking clauses"));
                }
                return Ok(Vec::new());
            }
            let strength = self.parse_locking_strength()?;
            let locked_rels = if self.consume(TokenKind::Of) {
                let mut relations = Vec::new();
                loop {
                    relations.push(Node::RangeVar(
                        self.try_parse_qualified_range_var()
                            .ok_or_else(|| self.error_here("OF requires a relation name"))?,
                    ));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at_any(&[
                        TokenKind::Nowait,
                        TokenKind::Skip,
                        TokenKind::For,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]) {
                        return Err(self.error_here("expected a relation after ','"));
                    }
                }
                relations
            } else {
                Vec::new()
            };
            let wait_policy = if self.consume(TokenKind::Nowait) {
                LockWaitPolicy::Error
            } else if self.consume(TokenKind::Skip) {
                self.expect(TokenKind::Locked)?;
                LockWaitPolicy::Skip
            } else {
                LockWaitPolicy::Block
            };
            clauses.push(Node::LockingClause(LockingClause {
                node_tag: NodeTag::LockingClause,
                locked_rels,
                strength,
                wait_policy,
            }));
            if self.at_any(stops) || !self.at(TokenKind::For) {
                break;
            }
        }
        Ok(clauses)
    }

    pub(super) fn parse_locking_strength(&mut self) -> PResult<LockClauseStrength> {
        match self.peek_kind() {
            TokenKind::Update => {
                self.advance();
                Ok(LockClauseStrength::Forupdate)
            }
            TokenKind::No => {
                self.advance();
                self.expect(TokenKind::Key)?;
                self.expect(TokenKind::Update)?;
                Ok(LockClauseStrength::Fornokeyupdate)
            }
            TokenKind::Share => {
                self.advance();
                Ok(LockClauseStrength::Forshare)
            }
            TokenKind::Key => {
                self.advance();
                self.expect(TokenKind::Share)?;
                Ok(LockClauseStrength::Forkeyshare)
            }
            _ => Err(self.error_here("expected UPDATE, NO KEY UPDATE, SHARE, or KEY SHARE")),
        }
    }
}
