use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-copy.html
    // COPY table_name [ ( column_name [, ...] ) ]
    //     FROM { 'filename' | PROGRAM 'command' | STDIN }
    //     [ [ WITH ] ( option [, ...] ) ]
    //     [ WHERE condition ]
    //
    // COPY { table_name [ ( column_name [, ...] ) ] | ( query ) }
    //     TO { 'filename' | PROGRAM 'command' | STDOUT }
    //     [ [ WITH ] ( option [, ...] ) ]
    //
    // where option can be one of:
    //
    //     FORMAT format_name
    //     FREEZE [ boolean ]
    //     DELIMITER 'delimiter_character'
    //     NULL 'null_string'
    //     DEFAULT 'default_string'
    //     HEADER [ boolean | MATCH ]
    //     QUOTE 'quote_character'
    //     ESCAPE 'escape_character'
    //     FORCE_QUOTE { ( column_name [, ...] ) | * }
    //     FORCE_NOT_NULL { ( column_name [, ...] ) | * }
    //     FORCE_NULL { ( column_name [, ...] ) | * }
    //     ON_ERROR error_action
    //     REJECT_LIMIT maxerror
    //     ENCODING 'encoding_name'
    //     LOG_VERBOSITY verbosity
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
            let relation = Some(Box::new(
                self.try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("COPY requires a relation or query"))?,
            ));
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
        let filename = match self.peek_kind() {
            TokenKind::SConst => self.consume_string_like(),
            TokenKind::Stdin | TokenKind::Stdout => {
                self.advance();
                None
            }
            _ => return Err(self.error_here("COPY requires a filename, STDIN, or STDOUT")),
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

    pub(super) fn parse_vacuum_relation_list(&mut self) -> PResult<NodeList> {
        let mut rels = Vec::new();
        if self.at_statement_end() {
            return Ok(rels);
        }
        loop {
            let relation = self.parse_relation_expr(false)?;
            let va_cols = if self.consume(TokenKind::Char('(')) {
                let columns = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                columns
            } else {
                Vec::new()
            };
            rels.push(Node::VacuumRelation(VacuumRelation {
                node_tag: NodeTag::VacuumRelation,
                relation: Some(Box::new(relation)),
                va_cols,
                ..VacuumRelation::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(rels)
    }

    pub(super) fn parse_copy_options(&mut self) -> PResult<NodeList> {
        self.consume(TokenKind::With);
        if self.at(TokenKind::Char('(')) {
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("COPY option list cannot be empty"));
            }
            let mut options = Vec::new();
            loop {
                let location = self.location();
                let tokens =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                options.push(Node::DefElem(parse_copy_generic_option(tokens, location)?));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected a COPY option after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
            return Ok(options);
        }

        let mut options = Vec::new();
        while !self.at_statement_end() && !self.at(TokenKind::Where) {
            let location = self.location();
            let (name, arg) = match self.peek_kind() {
                TokenKind::Binary => {
                    self.advance();
                    ("format", Some(make_string_node("binary")))
                }
                TokenKind::Csv => {
                    self.advance();
                    ("format", Some(make_string_node("csv")))
                }
                TokenKind::Json => {
                    self.advance();
                    ("format", Some(make_string_node("json")))
                }
                TokenKind::Freeze => {
                    self.advance();
                    ("freeze", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::HeaderP => {
                    self.advance();
                    ("header", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::Delimiter
                | TokenKind::NullP
                | TokenKind::Quote
                | TokenKind::Escape
                | TokenKind::Encoding => {
                    let kind = self.advance().kind;
                    if kind != TokenKind::Encoding {
                        self.consume(TokenKind::As);
                    }
                    if !self.at(TokenKind::SConst) {
                        return Err(self.error_here("COPY option requires a string value"));
                    }
                    let value = self
                        .consume_string_like()
                        .ok_or_else(|| self.error_here("COPY option requires a string value"))?;
                    let name = match kind {
                        TokenKind::Delimiter => "delimiter",
                        TokenKind::NullP => "null",
                        TokenKind::Quote => "quote",
                        TokenKind::Escape => "escape",
                        _ => "encoding",
                    };
                    (name, Some(make_string_node(value)))
                }
                TokenKind::Force => {
                    self.advance();
                    let name = if self.consume(TokenKind::Quote) {
                        "force_quote"
                    } else if self.consume(TokenKind::Not) {
                        self.expect(TokenKind::NullP)?;
                        "force_not_null"
                    } else {
                        self.expect(TokenKind::NullP)?;
                        "force_null"
                    };
                    let value = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let mut columns = Vec::new();
                        loop {
                            let column = match self.peek_kind() {
                                TokenKind::IConst => match self.advance().value.as_ref() {
                                    Some(TokenValue::Integer(value)) => value.to_string(),
                                    _ => unreachable!("IConst token requires an integer value"),
                                },
                                TokenKind::FConst | TokenKind::SConst => {
                                    match self.advance().value.as_ref() {
                                        Some(TokenValue::String(value)) => value.clone(),
                                        _ => unreachable!("literal token requires a string value"),
                                    }
                                }
                                _ => self.consume_col_label().ok_or_else(|| {
                                    self.error_here(
                                        "COPY FORCE option requires a column-list item or '*'",
                                    )
                                })?,
                            };
                            columns.push(make_string_node(column));
                            if !self.consume(TokenKind::Char(',')) {
                                break;
                            }
                        }
                        Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: columns,
                            location: -1,
                            ..AArrayExpr::default()
                        })
                    };
                    (name, Some(value))
                }
                _ => return Err(self.error_here("invalid COPY option")),
            };
            options.push(make_def_elem(name, arg, location));
        }
        Ok(options)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-vacuum.html
    // VACUUM [ ( option [, ...] ) ] [ table_and_columns [, ...] ]
    //
    // where option can be one of:
    //
    //     FULL [ boolean ]
    //     FREEZE [ boolean ]
    //     VERBOSE [ boolean ]
    //     ANALYZE [ boolean ]
    //     DISABLE_PAGE_SKIPPING [ boolean ]
    //     SKIP_LOCKED [ boolean ]
    //     INDEX_CLEANUP { AUTO | ON | OFF }
    //     PROCESS_MAIN [ boolean ]
    //     PROCESS_TOAST [ boolean ]
    //     TRUNCATE [ boolean ]
    //     PARALLEL integer
    //     SKIP_DATABASE_STATS [ boolean ]
    //     ONLY_DATABASE_STATS [ boolean ]
    //     BUFFER_USAGE_LIMIT size
    //
    // and table_and_columns is:
    //
    //     [ ONLY ] table_name [ * ] [ ( column_name [, ...] ) ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-analyze.html
    // ANALYZE [ ( option [, ...] ) ] [ table_and_columns [, ...] ]
    //
    // where option can be one of:
    //
    //     VERBOSE [ boolean ]
    //     SKIP_LOCKED [ boolean ]
    //     BUFFER_USAGE_LIMIT size
    //
    // and table_and_columns is:
    //
    //     [ ONLY ] table_name [ * ] [ ( column_name [, ...] ) ]
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
                    options.push(make_def_elem(name, None, token.location()));
                }
            }
            if matches!(self.peek_kind(), TokenKind::Analyze | TokenKind::Analyse) {
                let token = self.advance().clone();
                options.push(make_def_elem("analyze", None, token.location()));
            }
        } else if options.is_empty() && self.at(TokenKind::Verbose) {
            let token = self.advance().clone();
            options.push(make_def_elem("verbose", None, token.location()));
        }
        let rels = self.parse_vacuum_relation_list()?;
        Ok(Node::VacuumStmt(VacuumStmt {
            node_tag: NodeTag::VacuumStmt,
            options,
            rels,
            is_vacuumcmd,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-checkpoint.html
    // CHECKPOINT
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-discard.html
    // DISCARD { ALL | PLANS | SEQUENCES | TEMPORARY | TEMP }
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-lock.html
    // LOCK [ TABLE ] [ ONLY ] name [ * ] [, ...] [ IN lockmode MODE ] [ NOWAIT ]
    //
    // where lockmode is one of:
    //
    //     ACCESS SHARE | ROW SHARE | ROW EXCLUSIVE | SHARE UPDATE EXCLUSIVE
    //     | SHARE | SHARE ROW EXCLUSIVE | EXCLUSIVE | ACCESS EXCLUSIVE
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
        let mode = match self.peek_kind() {
            TokenKind::Access => {
                self.advance();
                match self.peek_kind() {
                    TokenKind::Share => {
                        self.advance();
                        1
                    }
                    TokenKind::Exclusive => {
                        self.advance();
                        8
                    }
                    _ => return Err(self.error_here("ACCESS requires SHARE or EXCLUSIVE")),
                }
            }
            TokenKind::Row => {
                self.advance();
                match self.peek_kind() {
                    TokenKind::Share => {
                        self.advance();
                        2
                    }
                    TokenKind::Exclusive => {
                        self.advance();
                        3
                    }
                    _ => return Err(self.error_here("ROW requires SHARE or EXCLUSIVE")),
                }
            }
            TokenKind::Share => {
                self.advance();
                match self.peek_kind() {
                    TokenKind::Update => {
                        self.advance();
                        self.expect(TokenKind::Exclusive)?;
                        4
                    }
                    TokenKind::Row => {
                        self.advance();
                        self.expect(TokenKind::Exclusive)?;
                        6
                    }
                    _ => 5,
                }
            }
            TokenKind::Exclusive => {
                self.advance();
                7
            }
            _ => return Err(self.error_here("invalid LOCK mode")),
        };
        Ok(mode)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-listen.html
    // LISTEN channel
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-unlisten.html
    // UNLISTEN { channel | * }
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-notify.html
    // NOTIFY channel [ , payload ]
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-load.html
    // LOAD 'filename'
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-refreshmaterializedview.html
    // REFRESH MATERIALIZED VIEW [ CONCURRENTLY ] name
    //     [ WITH [ NO ] DATA ]
    pub(super) fn parse_refresh(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Refresh)?;
        self.expect(TokenKind::Materialized)?;
        self.expect(TokenKind::View)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        let relation = Some(Box::new(self.try_parse_qualified_range_var().ok_or_else(
            || self.error_here("REFRESH MATERIALIZED VIEW requires a relation"),
        )?));
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-reindex.html
    // REINDEX [ ( option [, ...] ) ] { INDEX | TABLE | SCHEMA } [ CONCURRENTLY ] name
    // REINDEX [ ( option [, ...] ) ] { DATABASE | SYSTEM } [ CONCURRENTLY ] [ name ]
    //
    // where option can be one of:
    //
    //     CONCURRENTLY [ boolean ]
    //     TABLESPACE new_tablespace
    //     VERBOSE [ boolean ]
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
                Some(Box::new(self.try_parse_qualified_range_var().ok_or_else(
                    || self.error_here("REINDEX requires a relation name"),
                )?)),
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-cluster.html
    // CLUSTER [ ( option [, ...] ) ] [ table_name [ USING index_name ] ]
    //
    // where option can be one of:
    //
    //     VERBOSE [ boolean ]
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-truncate.html
    // TRUNCATE [ TABLE ] [ ONLY ] name [ * ] [, ... ]
    //     [ RESTART IDENTITY | CONTINUE IDENTITY ] [ CASCADE | RESTRICT ]
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

fn parse_copy_generic_option(mut tokens: Vec<Token>, location: usize) -> PResult<DefElem> {
    let eof_location = tokens.last().map_or(location, Token::end_location);
    tokens.push(Token::synthetic(TokenKind::Eof, eof_location));
    let mut parser = Parser { tokens, pos: 0 };
    let name = if parser.consume(TokenKind::FormatLa) {
        "format".to_owned()
    } else {
        parser
            .consume_col_label()
            .ok_or_else(|| parser.error_here("expected a COPY option name"))?
    };
    let arg = if parser.at(TokenKind::Eof) {
        None
    } else if parser.consume(TokenKind::Char('(')) {
        if parser.at(TokenKind::Char(')')) {
            return Err(parser.error_here("COPY option argument list cannot be empty"));
        }
        let mut values = Vec::new();
        loop {
            let value = parser
                .consume_opt_boolean_or_string()
                .ok_or_else(|| parser.error_here("expected a COPY option string value"))?;
            values.push(make_string_node(value));
            if !parser.consume(TokenKind::Char(',')) {
                break;
            }
            if parser.at(TokenKind::Char(')')) {
                return Err(parser.error_here("expected a COPY option value after ','"));
            }
        }
        parser.expect(TokenKind::Char(')'))?;
        Some(Node::AArrayExpr(AArrayExpr {
            node_tag: NodeTag::AArrayExpr,
            elements: values,
            location: -1,
            ..AArrayExpr::default()
        }))
    } else if parser.consume(TokenKind::Char('*')) {
        Some(Node::AStar(AStar {
            node_tag: NodeTag::AStar,
        }))
    } else if parser.consume(TokenKind::Default) {
        Some(make_string_node("default"))
    } else if parser.at_any(&[
        TokenKind::IConst,
        TokenKind::FConst,
        TokenKind::Char('+'),
        TokenKind::Char('-'),
    ]) {
        Some(parser.parse_numeric_only()?)
    } else {
        Some(make_string_node(
            parser
                .consume_opt_boolean_or_string()
                .ok_or_else(|| parser.error_here("invalid COPY option value"))?,
        ))
    };
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after COPY option"));
    }
    Ok(DefElem {
        node_tag: NodeTag::DefElem,
        defname: Some(name),
        arg: arg.map(Box::new),
        location: location as ParseLoc,
        ..DefElem::default()
    })
}
