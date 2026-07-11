use super::*;

impl Parser {
    pub(super) fn parse_copy(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Copy)?;
        let mut options = Vec::new();
        let leading_binary = self.consume(TokenKind::Binary);
        if leading_binary {
            options.push(make_def_elem(
                "format",
                Some(make_string_node("binary")),
                self.previous_location(),
            ));
        }

        let (relation, query, attlist) = if self.consume(TokenKind::Char('(')) {
            if leading_binary {
                return Err(self.error_here("BINARY is not allowed before a COPY query"));
            }
            let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            self.expect(TokenKind::Char(')'))?;
            let query = parse_preparable_statement_tokens(tokens)?;
            (None, Some(Box::new(query)), Vec::new())
        } else {
            let relation =
                Some(Box::new(self.parse_plain_range_var().ok_or_else(|| {
                    self.error_here("COPY requires a relation or query")
                })?));
            let attlist = if self.consume(TokenKind::Char('(')) {
                let columns = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                columns
            } else {
                Vec::new()
            };
            (relation, None, attlist)
        };

        let is_from = if self.consume(TokenKind::From) {
            if query.is_some() {
                return Err(self.error_here("COPY query only supports TO"));
            }
            true
        } else {
            self.expect(TokenKind::To)?;
            false
        };
        let is_program = self.consume(TokenKind::Program);
        let filename = if self.at(TokenKind::SConst) {
            self.consume_string_like()
        } else if self.consume(TokenKind::Stdin) || self.consume(TokenKind::Stdout) {
            None
        } else {
            return Err(self.error_here("COPY requires a filename, STDIN, or STDOUT"));
        };
        if is_program && filename.is_none() {
            return Err(self.error_here("STDIN/STDOUT is not allowed with PROGRAM"));
        }

        if query.is_none() && (self.at(TokenKind::Using) || self.at(TokenKind::Delimiters)) {
            let location = self.location();
            self.consume(TokenKind::Using);
            self.expect(TokenKind::Delimiters)?;
            let delimiter = self.consume_required_string("DELIMITERS requires a string literal")?;
            options.push(make_def_elem(
                "delimiter",
                Some(make_string_node(delimiter)),
                location,
            ));
        }
        options.extend(self.parse_copy_options()?);
        let where_clause = if self.consume(TokenKind::Where) {
            if !is_from {
                return Err(self.error_here("WHERE clause is not allowed with COPY TO"));
            }
            Some(self.parse_expr_box_strict_until(&[TokenKind::Char(';'), TokenKind::Eof])?)
        } else {
            None
        };
        Ok(Node::CopyStmt(CopyStmt {
            node_tag: NodeTag::CopyStmt,
            relation,
            query,
            attlist,
            is_from,
            is_program,
            filename,
            options,
            where_clause,
        }))
    }

    pub(super) fn parse_vacuum(&mut self) -> PResult<Node> {
        let is_vacuumcmd = self.consume(TokenKind::Vacuum);
        if !is_vacuumcmd {
            self.advance();
        }
        let mut options = if self.at(TokenKind::Char('(')) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        if options.is_empty() && is_vacuumcmd {
            for (kind, name) in [
                (TokenKind::Full, "full"),
                (TokenKind::Freeze, "freeze"),
                (TokenKind::Verbose, "verbose"),
            ] {
                if self.at(kind) {
                    let token = self.advance().clone();
                    options.push(make_def_elem(name, None, token.location));
                }
            }
            if matches!(self.peek_kind(), TokenKind::Analyze | TokenKind::Analyse) {
                let token = self.advance().clone();
                options.push(make_def_elem("analyze", None, token.location));
            }
        } else if options.is_empty() && self.at(TokenKind::Verbose) {
            let token = self.advance().clone();
            options.push(make_def_elem("verbose", None, token.location));
        }
        let rels = self.parse_vacuum_relation_list()?;
        Ok(Node::VacuumStmt(VacuumStmt {
            node_tag: NodeTag::VacuumStmt,
            options,
            rels,
            is_vacuumcmd,
        }))
    }

    pub(super) fn parse_checkpoint(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Checkpoint)?;
        let options = if self.at(TokenKind::Char('(')) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        Ok(Node::CheckPointStmt(CheckPointStmt {
            node_tag: NodeTag::CheckPointStmt,
            options,
        }))
    }

    pub(super) fn parse_discard(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Discard)?;
        let target = match self.advance().kind {
            TokenKind::All => DiscardMode::All,
            TokenKind::Plans => DiscardMode::Plans,
            TokenKind::Sequences => DiscardMode::Sequences,
            TokenKind::Temp | TokenKind::Temporary => DiscardMode::Temp,
            _ => return Err(self.error_here("DISCARD requires ALL, PLANS, SEQUENCES, or TEMP")),
        };
        Ok(Node::DiscardStmt(DiscardStmt {
            node_tag: NodeTag::DiscardStmt,
            target,
        }))
    }

    pub(super) fn parse_lock(&mut self) -> PResult<Node> {
        self.expect(TokenKind::LockP)?;
        self.consume(TokenKind::Table);
        let mut relations = Vec::new();
        loop {
            let relation = self.parse_relation_expr(false)?;
            relations.push(Node::RangeVar(relation));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        let mode = if self.consume(TokenKind::InP) {
            let mode = self.parse_lock_mode()?;
            self.expect(TokenKind::Mode)?;
            mode
        } else {
            8
        };
        let nowait = self.consume(TokenKind::Nowait);
        Ok(Node::LockStmt(LockStmt {
            node_tag: NodeTag::LockStmt,
            relations,
            mode,
            nowait,
        }))
    }

    pub(super) fn parse_lock_mode(&mut self) -> PResult<i32> {
        let mode = if self.consume(TokenKind::Access) {
            if self.consume(TokenKind::Share) {
                1
            } else {
                self.expect(TokenKind::Exclusive)?;
                8
            }
        } else if self.consume(TokenKind::Row) {
            if self.consume(TokenKind::Share) {
                2
            } else {
                self.expect(TokenKind::Exclusive)?;
                3
            }
        } else if self.consume(TokenKind::Share) {
            if self.consume(TokenKind::Update) {
                self.expect(TokenKind::Exclusive)?;
                4
            } else if self.consume(TokenKind::Row) {
                self.expect(TokenKind::Exclusive)?;
                6
            } else {
                5
            }
        } else if self.consume(TokenKind::Exclusive) {
            7
        } else {
            return Err(self.error_here("invalid LOCK mode"));
        };
        Ok(mode)
    }

    pub(super) fn parse_listen(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Listen)?;
        let conditionname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("LISTEN requires a channel name"))?,
        );
        Ok(Node::ListenStmt(ListenStmt {
            node_tag: NodeTag::ListenStmt,
            conditionname,
        }))
    }

    pub(super) fn parse_unlisten(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Unlisten)?;
        let conditionname = if self.consume(TokenKind::Char('*')) {
            None
        } else {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("UNLISTEN requires a channel name or '*'"))?,
            )
        };
        Ok(Node::UnlistenStmt(UnlistenStmt {
            node_tag: NodeTag::UnlistenStmt,
            conditionname,
        }))
    }

    pub(super) fn parse_notify(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Notify)?;
        let conditionname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("NOTIFY requires a channel name"))?,
        );
        let payload = if self.consume(TokenKind::Char(',')) {
            if !self.at(TokenKind::SConst) {
                return Err(self.error_here("NOTIFY payload must be a string"));
            }
            Some(
                self.consume_string_like()
                    .ok_or_else(|| self.error_here("NOTIFY payload must be a string"))?,
            )
        } else {
            None
        };
        Ok(Node::NotifyStmt(NotifyStmt {
            node_tag: NodeTag::NotifyStmt,
            conditionname,
            payload,
        }))
    }

    pub(super) fn parse_load(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Load)?;
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here("LOAD requires a string filename"));
        }
        let filename = Some(
            self.consume_string_like()
                .ok_or_else(|| self.error_here("LOAD requires a filename"))?,
        );
        Ok(Node::LoadStmt(LoadStmt {
            node_tag: NodeTag::LoadStmt,
            filename,
        }))
    }

    pub(super) fn parse_refresh(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Refresh)?;
        self.expect(TokenKind::Materialized)?;
        self.expect(TokenKind::View)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        let relation = Some(Box::new(self.parse_plain_range_var().ok_or_else(|| {
            self.error_here("REFRESH MATERIALIZED VIEW requires a relation")
        })?));
        let skip_data = if self.consume(TokenKind::With) {
            let no = self.consume(TokenKind::No);
            self.expect(TokenKind::DataP)?;
            no
        } else {
            false
        };
        Ok(Node::RefreshMatViewStmt(RefreshMatViewStmt {
            node_tag: NodeTag::RefreshMatViewStmt,
            concurrent,
            skip_data,
            relation,
        }))
    }

    pub(super) fn parse_reindex(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Reindex)?;
        let mut params = if self.at(TokenKind::Char('(')) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        let kind = match self.advance().kind {
            TokenKind::Index => ReindexObjectType::Index,
            TokenKind::Table => ReindexObjectType::Table,
            TokenKind::Schema => ReindexObjectType::Schema,
            TokenKind::SystemP => ReindexObjectType::System,
            TokenKind::Database => ReindexObjectType::Database,
            _ => return Err(self.error_here("REINDEX requires an object type")),
        };
        if self.consume(TokenKind::Concurrently) {
            params.push(make_def_elem(
                "concurrently",
                None,
                self.previous_location(),
            ));
        }
        let (relation, name) = match kind {
            ReindexObjectType::Index | ReindexObjectType::Table => (
                Some(Box::new(self.parse_plain_range_var().ok_or_else(|| {
                    self.error_here("REINDEX requires a relation name")
                })?)),
                None,
            ),
            ReindexObjectType::Schema => (
                None,
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("REINDEX SCHEMA requires a name"))?,
                ),
            ),
            ReindexObjectType::System | ReindexObjectType::Database => {
                (None, self.consume_col_id())
            }
        };
        Ok(Node::ReindexStmt(ReindexStmt {
            node_tag: NodeTag::ReindexStmt,
            kind,
            relation,
            name,
            params,
        }))
    }

    pub(super) fn parse_repack(&mut self) -> PResult<Node> {
        if self.consume(TokenKind::Cluster) {
            return self.parse_cluster();
        }
        self.expect(TokenKind::Repack)?;
        let params = if self.at(TokenKind::Char('(')) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        let (relation, usingindex, indexname) = if self.at_statement_end() {
            (None, false, None)
        } else if self.consume(TokenKind::Using) {
            self.expect(TokenKind::Index)?;
            self.expect_statement_end()?;
            (None, true, None)
        } else {
            let relation = self.parse_relation_expr(false)?;
            let va_cols = if self.consume(TokenKind::Char('(')) {
                let columns = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                columns
            } else {
                Vec::new()
            };
            let relation = Some(Box::new(VacuumRelation {
                node_tag: NodeTag::VacuumRelation,
                relation: Some(Box::new(relation)),
                va_cols,
                ..VacuumRelation::default()
            }));
            if self.consume(TokenKind::Using) {
                self.expect(TokenKind::Index)?;
                (relation, true, self.consume_col_id())
            } else {
                (relation, false, None)
            }
        };
        Ok(Node::RepackStmt(RepackStmt {
            node_tag: NodeTag::RepackStmt,
            command: RepackCommand::Repack,
            relation,
            indexname,
            usingindex,
            params,
        }))
    }

    fn parse_cluster(&mut self) -> PResult<Node> {
        let parenthesized = self.at(TokenKind::Char('('));
        let mut params = if parenthesized {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        if !parenthesized && self.consume(TokenKind::Verbose) {
            params.push(make_def_elem("verbose", None, self.previous_location()));
        }
        if self.at_statement_end() {
            return Ok(Node::RepackStmt(RepackStmt {
                node_tag: NodeTag::RepackStmt,
                command: RepackCommand::Cluster,
                usingindex: true,
                params,
                ..RepackStmt::default()
            }));
        }

        let save = self.pos;
        if let Some(indexname) = self.consume_col_id()
            && self.consume(TokenKind::On)
        {
            let relation = self
                .try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("CLUSTER ON requires a relation"))?;
            return Ok(Node::RepackStmt(RepackStmt {
                node_tag: NodeTag::RepackStmt,
                command: RepackCommand::Cluster,
                relation: Some(Box::new(VacuumRelation {
                    node_tag: NodeTag::VacuumRelation,
                    relation: Some(Box::new(relation)),
                    ..VacuumRelation::default()
                })),
                indexname: Some(indexname),
                usingindex: true,
                params,
            }));
        }
        self.pos = save;

        let relation = self
            .try_parse_qualified_range_var()
            .ok_or_else(|| self.error_here("CLUSTER requires a relation"))?;
        let indexname = if self.consume(TokenKind::Using) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an index name"))?,
            )
        } else {
            None
        };
        Ok(Node::RepackStmt(RepackStmt {
            node_tag: NodeTag::RepackStmt,
            command: RepackCommand::Cluster,
            relation: Some(Box::new(VacuumRelation {
                node_tag: NodeTag::VacuumRelation,
                relation: Some(Box::new(relation)),
                ..VacuumRelation::default()
            })),
            indexname,
            usingindex: true,
            params,
        }))
    }

    pub(super) fn parse_truncate(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Truncate)?;
        self.consume(TokenKind::Table);
        let mut relations = Vec::new();
        loop {
            let relation = self.parse_relation_expr(false)?;
            relations.push(Node::RangeVar(relation));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        let restart_seqs = if self.consume(TokenKind::Restart) {
            self.expect(TokenKind::IdentityP)?;
            true
        } else if self.consume(TokenKind::ContinueP) {
            self.expect(TokenKind::IdentityP)?;
            false
        } else {
            false
        };
        let behavior = self.parse_drop_behavior();
        Ok(Node::TruncateStmt(TruncateStmt {
            node_tag: NodeTag::TruncateStmt,
            relations,
            restart_seqs,
            behavior,
        }))
    }
}
