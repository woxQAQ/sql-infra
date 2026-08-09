//! Logical-replication publication and subscription statements.
//!
//! Publication object lists, row filters, column lists, subscription options, and
//! alter/drop actions retain their PostgreSQL-specific constraints here.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createsubscription.html
    // CREATE SUBSCRIPTION subscription_name
    //     CONNECTION 'conninfo'
    //     PUBLICATION publication_name [, ...]
    //     [ WITH ( subscription_parameter [= value] [, ... ] ) ]
    pub(super) fn parse_create_subscription(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Subscription)?;
        self.record_completion_slot(completion::GrammarSlot::Subscription);
        let subname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE SUBSCRIPTION requires a name"))?,
        );
        let (servername, conninfo) = if self.consume(TokenKind::Connection) {
            (
                None,
                Some(self.consume_required_string("CONNECTION requires a string")?),
            )
        } else if self.consume(TokenKind::Server) {
            self.record_completion_slot(completion::GrammarSlot::ForeignServer);
            (
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("SERVER requires a name"))?,
                ),
                None,
            )
        } else {
            return Err(self.error_here("CREATE SUBSCRIPTION requires CONNECTION or SERVER"));
        };
        self.expect(TokenKind::Publication)?;
        let publication = self.parse_publication_name_list()?;
        let options = if self.consume(TokenKind::With) {
            self.parse_subscription_option_list()?
        } else {
            Vec::new()
        };
        Ok(node!(CreateSubscriptionStmt {
            subname,
            servername,
            conninfo,
            publication,
            options,
        }))
    }
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterpublication.html
    // ALTER PUBLICATION name ADD publication_object [, ...]
    // ALTER PUBLICATION name SET publication_object [, ...]
    // ALTER PUBLICATION name DROP publication_drop_object [, ...]
    // ALTER PUBLICATION name SET ( publication_parameter [= value] [, ... ] )
    // ALTER PUBLICATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PUBLICATION name RENAME TO new_name
    //
    // where publication_object is one of:
    //
    //     TABLE table_and_columns [, ... ]
    //     TABLES IN SCHEMA { schema_name | CURRENT_SCHEMA } [, ... ]
    //
    // and publication_drop_object is one of:
    //
    //     TABLE [ ONLY ] table_name [ * ] [, ... ]
    //     TABLES IN SCHEMA { schema_name | CURRENT_SCHEMA } [, ... ]
    //
    // and table_and_columns is:
    //
    //     [ ONLY ] table_name [ * ] [ ( column_name [, ... ] ) ] [ WHERE ( expression ) ]
    pub(super) fn parse_alter_publication(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Publication)?;
        self.record_completion_slot(completion::GrammarSlot::Publication);
        let pubname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER PUBLICATION requires a name"))?,
        );
        self.record_completion_tokens(&[
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Set,
            TokenKind::Rename,
            TokenKind::Owner,
        ]);
        let action = match self.peek_kind() {
            TokenKind::AddP => AlterPublicationAction::AddObjects,
            TokenKind::Drop => AlterPublicationAction::DropObjects,
            TokenKind::Set => AlterPublicationAction::SetObjects,
            _ => return Err(self.error_here("expected ADD, DROP, or SET after publication name")),
        };
        self.advance();
        if action == AlterPublicationAction::SetObjects {
            self.record_completion_tokens(&[TokenKind::Char('(')]);
        }
        let (pubobjects, for_all_tables, for_all_sequences, options) = if self
            .at(TokenKind::Char('('))
        {
            if action != AlterPublicationAction::SetObjects {
                return Err(self.error_here("only ALTER PUBLICATION SET accepts options"));
            }
            (Vec::new(), false, false, self.parse_def_elem_list()?)
        } else {
            let (objects, all_tables, all_sequences) = self.parse_publication_objects()?;
            if action != AlterPublicationAction::SetObjects && (all_tables || all_sequences) {
                return Err(self
                    .error_here("ALL TABLES/SEQUENCES is only valid with ALTER PUBLICATION SET"));
            }
            (objects, all_tables, all_sequences, Vec::new())
        };
        Ok(node!(AlterPublicationStmt {
            pubname,
            options,
            pubobjects,
            action,
            for_all_tables,
            for_all_sequences,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersubscription.html
    // ALTER SUBSCRIPTION name CONNECTION 'conninfo'
    // ALTER SUBSCRIPTION name SET PUBLICATION publication_name [, ...] [ WITH ( publication_option
    // [= value] [, ... ] ) ] ALTER SUBSCRIPTION name ADD PUBLICATION publication_name [, ...] [
    // WITH ( publication_option [= value] [, ... ] ) ] ALTER SUBSCRIPTION name DROP PUBLICATION
    // publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name REFRESH PUBLICATION [ WITH ( refresh_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name ENABLE
    // ALTER SUBSCRIPTION name DISABLE
    // ALTER SUBSCRIPTION name SET ( subscription_parameter [= value] [, ... ] )
    // ALTER SUBSCRIPTION name SKIP ( skip_option = value )
    // ALTER SUBSCRIPTION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SUBSCRIPTION name RENAME TO new_name
    pub(super) fn parse_alter_subscription(&mut self) -> PResult<Node> {
        let alter_location = self.previous_location();
        self.expect(TokenKind::Subscription)?;
        self.record_completion_slot(completion::GrammarSlot::Subscription);
        let subname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER SUBSCRIPTION requires a name"))?,
        );
        let mut stmt = AlterSubscriptionStmt {
            subname,
            ..AlterSubscriptionStmt::default()
        };
        self.record_completion_tokens(&[
            TokenKind::Connection,
            TokenKind::Server,
            TokenKind::Set,
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Refresh,
            TokenKind::EnableP,
            TokenKind::DisableP,
            TokenKind::Skip,
            TokenKind::Rename,
            TokenKind::Owner,
        ]);
        stmt.kind = match self.peek_kind() {
            TokenKind::Connection => {
                self.advance();
                stmt.conninfo =
                    Some(self.consume_required_string("CONNECTION requires a string literal")?);
                AlterSubscriptionType::Connection
            }
            TokenKind::Server => {
                self.advance();
                self.record_completion_slot(completion::GrammarSlot::ForeignServer);
                stmt.servername = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("SERVER requires a server name"))?,
                );
                AlterSubscriptionType::Server
            }
            TokenKind::Set => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::Char('('), TokenKind::Publication]);
                if self.at(TokenKind::Char('(')) {
                    stmt.options = self.parse_subscription_option_list()?;
                    AlterSubscriptionType::Options
                } else {
                    self.expect(TokenKind::Publication)?;
                    stmt.publication = self.parse_publication_name_list()?;
                    AlterSubscriptionType::SetPublication
                }
            }
            TokenKind::AddP => {
                self.advance();
                self.expect(TokenKind::Publication)?;
                stmt.publication = self.parse_publication_name_list()?;
                AlterSubscriptionType::AddPublication
            }
            TokenKind::Drop => {
                self.advance();
                self.expect(TokenKind::Publication)?;
                stmt.publication = self.parse_publication_name_list()?;
                AlterSubscriptionType::DropPublication
            }
            TokenKind::Refresh => {
                self.advance();
                if self.consume(TokenKind::Publication) {
                    AlterSubscriptionType::RefreshPublication
                } else if self.consume(TokenKind::Sequences) {
                    AlterSubscriptionType::RefreshSequences
                } else {
                    return Err(self.error_here("REFRESH requires PUBLICATION or SEQUENCES"));
                }
            }
            TokenKind::EnableP => {
                self.advance();
                stmt.options.push(make_def_elem(
                    "enabled",
                    Some(node!(Boolean::new(true))),
                    alter_location,
                ));
                AlterSubscriptionType::Enabled
            }
            TokenKind::DisableP => {
                self.advance();
                stmt.options.push(make_def_elem(
                    "enabled",
                    Some(node!(Boolean::new(false))),
                    alter_location,
                ));
                AlterSubscriptionType::Enabled
            }
            TokenKind::Skip => {
                self.advance();
                stmt.options = self.parse_parenthesized_def_elem_list_strict()?;
                AlterSubscriptionType::Skip
            }
            _ => return Err(self.error_here("unsupported ALTER SUBSCRIPTION action")),
        };
        if matches!(
            stmt.kind,
            AlterSubscriptionType::SetPublication
                | AlterSubscriptionType::AddPublication
                | AlterSubscriptionType::DropPublication
                | AlterSubscriptionType::RefreshPublication
        ) && self.consume(TokenKind::With)
        {
            stmt.options = self.parse_subscription_option_list()?;
        }
        self.expect_statement_end()?;
        Ok(Node::AlterSubscriptionStmt(stmt))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropsubscription.html
    // DROP SUBSCRIPTION [ IF EXISTS ] name [ CASCADE | RESTRICT ]
    pub(super) fn parse_drop_subscription(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Subscription)?;
        let missing_ok = self.consume_if_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Subscription);
        let subname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DROP SUBSCRIPTION requires a name"))?,
        );
        let behavior = self.parse_drop_behavior();
        self.expect_statement_end()?;
        Ok(node!(DropSubscriptionStmt {
            subname,
            missing_ok,
            behavior,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createpublication.html
    // CREATE PUBLICATION name
    //     [ FOR ALL TABLES
    //       | FOR publication_object [, ... ] ]
    //     [ WITH ( publication_parameter [= value] [, ... ] ) ]
    //
    // where publication_object is one of:
    //
    //     TABLE table_and_columns [, ... ]
    //     TABLES IN SCHEMA { schema_name | CURRENT_SCHEMA } [, ... ]
    //
    // and table_and_columns is:
    //
    //     [ ONLY ] table_name [ * ] [ ( column_name [, ... ] ) ] [ WHERE ( expression ) ]
    pub(super) fn parse_create_publication(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Publication)?;
        self.record_completion_slot(completion::GrammarSlot::Publication);
        let pubname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE PUBLICATION requires a name"))?,
        );
        let mut for_all_tables = false;
        let mut for_all_sequences = false;
        let mut pubobjects = Vec::new();
        if self.consume(TokenKind::For) {
            (pubobjects, for_all_tables, for_all_sequences) = self.parse_publication_objects()?;
        }
        let options = if self.consume(TokenKind::With) {
            self.parse_def_elem_list()?
        } else {
            Vec::new()
        };
        Ok(node!(CreatePublicationStmt {
            pubname,
            options,
            pubobjects,
            for_all_tables,
            for_all_sequences,
        }))
    }

    pub(super) fn parse_publication_objects(&mut self) -> PResult<(NodeList, bool, bool)> {
        let mut objects = Vec::new();
        let mut for_all_tables = false;
        let mut for_all_sequences = false;
        let mut all_objects_mode = None;
        let mut continuation = None;
        loop {
            self.record_completion_tokens(&[TokenKind::All, TokenKind::Table, TokenKind::Tables]);
            let location = self.location();
            match self.peek_kind() {
                TokenKind::All => {
                    self.advance();
                    if all_objects_mode == Some(false) {
                        return Err(self.error_here(
                            "ALL TABLES/SEQUENCES cannot be mixed with publication object lists",
                        ));
                    }
                    all_objects_mode = Some(true);
                    let except_tables = if self.consume(TokenKind::Tables) {
                        if for_all_tables {
                            return Err(self.error_here("ALL TABLES can be specified only once"));
                        }
                        for_all_tables = true;
                        if self.consume(TokenKind::Except) {
                            self.expect(TokenKind::Char('('))?;
                            self.expect(TokenKind::Table)?;
                            let mut tables = Vec::new();
                            loop {
                                self.consume(TokenKind::Table);
                                let table_location = self.location();
                                let relation = self.parse_relation_expr_with_slot(
                                    completion::GrammarSlot::Table,
                                )?;
                                tables.push(node!(PublicationObjSpec {
                                    pubobjtype: PublicationObjSpecType::ExceptTable,
                                    pubtable: Some(Box::new(PublicationTable {
                                        relation: Some(Box::new(relation)),
                                        except: true,
                                        ..PublicationTable::default()
                                    })),
                                    location: table_location as ParseLoc,
                                    ..PublicationObjSpec::default()
                                }));
                                if !self.consume(TokenKind::Char(',')) {
                                    break;
                                }
                            }
                            self.expect(TokenKind::Char(')'))?;
                            tables
                        } else {
                            Vec::new()
                        }
                    } else if self.consume(TokenKind::Sequences) {
                        if for_all_sequences {
                            return Err(self.error_here("ALL SEQUENCES can be specified only once"));
                        }
                        for_all_sequences = true;
                        Vec::new()
                    } else {
                        return Err(self.error_here("expected TABLES or SEQUENCES after ALL"));
                    };
                    objects.extend(except_tables);
                }
                TokenKind::Tables => {
                    self.advance();
                    if all_objects_mode == Some(true) {
                        return Err(self.error_here(
                            "publication object lists cannot be mixed with ALL TABLES/SEQUENCES",
                        ));
                    }
                    all_objects_mode = Some(false);
                    self.expect(TokenKind::InP)?;
                    self.expect(TokenKind::Schema)?;
                    self.record_completion_slot(completion::GrammarSlot::Schema);
                    let location = self.location();
                    let current = self.consume(TokenKind::CurrentSchema);
                    let name = if current {
                        None
                    } else {
                        Some(
                            self.consume_col_id()
                                .ok_or_else(|| self.error_here("expected a schema name"))?,
                        )
                    };
                    continuation = Some(PublicationObjSpecType::TablesInSchema);
                    objects.push(node!(PublicationObjSpec {
                        pubobjtype: if current {
                            PublicationObjSpecType::TablesInCurSchema
                        } else {
                            PublicationObjSpecType::TablesInSchema
                        },
                        name,
                        location: location as ParseLoc,
                        ..PublicationObjSpec::default()
                    }));
                }
                _ => {
                    if all_objects_mode == Some(true) {
                        return Err(self.error_here("expected ALL TABLES or ALL SEQUENCES"));
                    }
                    all_objects_mode = Some(false);
                    let explicit_table = self.consume(TokenKind::Table);
                    if !explicit_table && continuation.is_none() {
                        return Err(self.error_here(
                            "TABLE or TABLES IN SCHEMA must precede a publication object",
                        ));
                    }
                    if !explicit_table
                        && continuation == Some(PublicationObjSpecType::TablesInSchema)
                    {
                        self.record_completion_slot(completion::GrammarSlot::Schema);
                        let current = self.consume(TokenKind::CurrentSchema);
                        let name = if current {
                            None
                        } else {
                            Some(self.consume_col_id().ok_or_else(|| {
                                self.error_here("expected a schema name in publication object list")
                            })?)
                        };
                        objects.push(node!(PublicationObjSpec {
                            pubobjtype: if current {
                                PublicationObjSpecType::TablesInCurSchema
                            } else {
                                PublicationObjSpecType::TablesInSchema
                            },
                            name,
                            location: location as ParseLoc,
                            ..PublicationObjSpec::default()
                        }));
                        if !self.consume(TokenKind::Char(',')) {
                            break;
                        }
                        continue;
                    }
                    let relation =
                        self.parse_relation_expr_with_slot(completion::GrammarSlot::Table)?;
                    let columns = self.parse_optional_column_name_list()?;
                    let where_clause = if self.consume(TokenKind::Where) {
                        Some(self.parse_parenthesized_expr_box()?)
                    } else {
                        None
                    };
                    continuation = Some(PublicationObjSpecType::Table);
                    objects.push(node!(PublicationObjSpec {
                        pubobjtype: PublicationObjSpecType::Table,
                        pubtable: Some(Box::new(PublicationTable {
                            relation: Some(Box::new(relation)),
                            where_clause,
                            columns,
                            ..PublicationTable::default()
                        })),
                        location: if explicit_table {
                            0
                        } else {
                            location as ParseLoc
                        },
                        ..PublicationObjSpec::default()
                    }));
                }
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        if objects.is_empty() {
            return Err(self.error_here("expected a publication object"));
        }
        Ok((objects, for_all_tables, for_all_sequences))
    }

    pub(super) fn parse_publication_name_list(&mut self) -> PResult<NodeList> {
        let mut publications = Vec::new();
        loop {
            self.record_completion_slot(completion::GrammarSlot::Publication);
            if self.at_completion() {
                return Err(self.error_here("expected a publication name"));
            }
            if self.at_any(&[TokenKind::With, TokenKind::Char(';'), TokenKind::Eof]) {
                break;
            }
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a publication name"))?;
            publications.push(make_string_node(name));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(&[TokenKind::With, TokenKind::Char(';'), TokenKind::Eof]) {
                return Err(self.error_here("expected a publication name after ','"));
            }
        }
        if publications.is_empty() {
            return Err(self.error_here("PUBLICATION requires at least one name"));
        }
        Ok(publications)
    }
}
