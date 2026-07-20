use super::*;

impl Parser {
    pub(super) fn parse_create_prop_graph(&mut self, relpersistence: u8) -> PResult<Node> {
        self.expect(TokenKind::Graph)?;
        let mut pgname = self
            .try_parse_qualified_range_var()
            .ok_or_else(|| self.error_here("CREATE PROPERTY GRAPH requires a name"))?;
        pgname.relpersistence = relpersistence;
        let pgname = Some(Box::new(pgname));
        let vertex_tables = if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
            self.advance();
            self.expect(TokenKind::Tables)?;
            self.parse_prop_graph_vertex_list()?
        } else {
            Vec::new()
        };
        let edge_tables = if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
            self.advance();
            self.expect(TokenKind::Tables)?;
            self.parse_prop_graph_edge_list()?
        } else {
            Vec::new()
        };
        self.expect_statement_end()?;
        Ok(Node::CreatePropGraphStmt(CreatePropGraphStmt {
            node_tag: NodeTag::CreatePropGraphStmt,
            pgname,
            vertex_tables,
            edge_tables,
        }))
    }

    pub(super) fn parse_alter_prop_graph(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Graph)?;
        let pgname = Some(Box::new(
            self.try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("ALTER PROPERTY GRAPH requires a name"))?,
        ));
        let mut stmt = AlterPropGraphStmt {
            node_tag: NodeTag::AlterPropGraphStmt,
            pgname,
            ..AlterPropGraphStmt::default()
        };
        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
                    self.advance();
                    self.expect(TokenKind::Tables)?;
                    stmt.add_vertex_tables = self.parse_prop_graph_vertex_list()?;
                    if self.consume(TokenKind::AddP) {
                        if !matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
                            return Err(self.error_here("second ADD must introduce EDGE TABLES"));
                        }
                        self.advance();
                        self.expect(TokenKind::Tables)?;
                        stmt.add_edge_tables = self.parse_prop_graph_edge_list()?;
                    }
                } else if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
                    self.advance();
                    self.expect(TokenKind::Tables)?;
                    stmt.add_edge_tables = self.parse_prop_graph_edge_list()?;
                } else {
                    return Err(self.error_here("ADD requires VERTEX TABLES or EDGE TABLES"));
                }
            }
            TokenKind::Drop => {
                self.advance();
                let vertex = if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
                    self.advance();
                    true
                } else if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
                    self.advance();
                    false
                } else {
                    return Err(self.error_here("DROP requires VERTEX TABLES or EDGE TABLES"));
                };
                self.expect(TokenKind::Tables)?;
                self.expect(TokenKind::Char('('))?;
                let names = self.parse_parenthesized_name_list_body()?;
                self.expect(TokenKind::Char(')'))?;
                if vertex {
                    stmt.drop_vertex_tables = names;
                } else {
                    stmt.drop_edge_tables = names;
                }
                stmt.drop_behavior = self.parse_drop_behavior();
            }
            TokenKind::Alter => {
                self.advance();
                stmt.element_kind =
                    if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
                        self.advance();
                        AlterPropGraphElementKind::Vertex
                    } else if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship)
                    {
                        self.advance();
                        AlterPropGraphElementKind::Edge
                    } else {
                        return Err(self.error_here("ALTER requires VERTEX or EDGE TABLE"));
                    };
                self.expect(TokenKind::Table)?;
                stmt.element_alias = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("ALTER graph element requires an alias"))?,
                );
                match self.peek_kind() {
                    TokenKind::AddP => {
                        while self.consume(TokenKind::AddP) {
                            let location = self.previous_location();
                            self.expect(TokenKind::Label)?;
                            let label = Some(
                                self.consume_col_id()
                                    .ok_or_else(|| self.error_here("ADD LABEL requires a name"))?,
                            );
                            let properties = Some(Box::new(self.parse_prop_graph_properties()?));
                            stmt.add_labels.push(Node::PropGraphLabelAndProperties(
                                PropGraphLabelAndProperties {
                                    node_tag: NodeTag::PropGraphLabelAndProperties,
                                    label,
                                    properties,
                                    location: location as ParseLoc,
                                },
                            ));
                        }
                    }
                    TokenKind::Drop => {
                        self.advance();
                        self.expect(TokenKind::Label)?;
                        stmt.drop_label = Some(
                            self.consume_col_id()
                                .ok_or_else(|| self.error_here("DROP LABEL requires a name"))?,
                        );
                        stmt.drop_behavior = self.parse_drop_behavior();
                    }
                    TokenKind::Alter => {
                        self.advance();
                        self.expect(TokenKind::Label)?;
                        stmt.alter_label = Some(
                            self.consume_col_id()
                                .ok_or_else(|| self.error_here("ALTER LABEL requires a name"))?,
                        );
                        if self.consume(TokenKind::AddP) {
                            stmt.add_properties =
                                Some(Box::new(self.parse_prop_graph_add_properties()?));
                        } else if self.consume(TokenKind::Drop) {
                            self.expect(TokenKind::Properties)?;
                            self.expect(TokenKind::Char('('))?;
                            stmt.drop_properties = self.parse_parenthesized_name_list_body()?;
                            self.expect(TokenKind::Char(')'))?;
                            stmt.drop_behavior = self.parse_drop_behavior();
                        } else {
                            return Err(
                                self.error_here("ALTER LABEL requires ADD or DROP PROPERTIES")
                            );
                        }
                    }
                    _ => {
                        return Err(self.error_here("unsupported graph element alteration"));
                    }
                }
            }
            _ => {
                return Err(self.error_here("ALTER PROPERTY GRAPH requires ADD, DROP, or ALTER"));
            }
        }
        self.expect_statement_end()?;
        Ok(Node::AlterPropGraphStmt(stmt))
    }

    pub(super) fn parse_prop_graph_vertex_list(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("vertex table list cannot be empty"));
        }
        let mut vertices = Vec::new();
        loop {
            let location = self.location();
            let mut vtable = self
                .try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("expected a vertex table name"))?;
            if self.consume(TokenKind::As) {
                let aliasname = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("AS requires a graph table alias"))?;
                vtable.alias = Some(Box::new(Alias {
                    node_tag: NodeTag::Alias,
                    aliasname: Some(aliasname),
                    ..Alias::default()
                }));
            }
            let vkey = self.parse_optional_key_clause()?;
            let labels = self.parse_prop_graph_labels()?;
            vertices.push(Node::PropGraphVertex(PropGraphVertex {
                node_tag: NodeTag::PropGraphVertex,
                vtable: Some(Box::new(vtable)),
                vkey,
                labels,
                location: location as ParseLoc,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a vertex table after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(vertices)
    }

    pub(super) fn parse_prop_graph_edge_list(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("edge table list cannot be empty"));
        }
        let mut edges = Vec::new();
        loop {
            let location = self.location();
            let mut etable = self
                .try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("expected an edge table name"))?;
            if self.consume(TokenKind::As) {
                let aliasname = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("AS requires a graph table alias"))?;
                etable.alias = Some(Box::new(Alias {
                    node_tag: NodeTag::Alias,
                    aliasname: Some(aliasname),
                    ..Alias::default()
                }));
            }
            let ekey = self.parse_optional_key_clause()?;
            let (esrckey, esrcvertex, esrcvertexcols) =
                self.parse_prop_graph_endpoint(TokenKind::Source, "SOURCE")?;
            let (edestkey, edestvertex, edestvertexcols) =
                self.parse_prop_graph_endpoint(TokenKind::Destination, "DESTINATION")?;
            let labels = self.parse_prop_graph_labels()?;
            edges.push(Node::PropGraphEdge(PropGraphEdge {
                node_tag: NodeTag::PropGraphEdge,
                etable: Some(Box::new(etable)),
                ekey,
                esrckey,
                esrcvertex,
                esrcvertexcols,
                edestkey,
                edestvertex,
                edestvertexcols,
                labels,
                location: location as ParseLoc,
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an edge table after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(edges)
    }

    pub(super) fn parse_prop_graph_endpoint(
        &mut self,
        keyword: TokenKind,
        label: &str,
    ) -> PResult<(NodeList, Option<std::string::String>, NodeList)> {
        self.expect(keyword)?;
        if self.consume(TokenKind::Key) {
            self.expect(TokenKind::Char('('))?;
            let key = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            self.expect(TokenKind::References)?;
            let vertex =
                Some(self.consume_col_id().ok_or_else(|| {
                    self.error_here(format!("{label} REFERENCES requires an alias"))
                })?);
            self.expect(TokenKind::Char('('))?;
            let columns = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            Ok((key, vertex, columns))
        } else {
            let vertex = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here(format!("{label} requires a vertex alias")))?,
            );
            Ok((Vec::new(), vertex, Vec::new()))
        }
    }

    pub(super) fn parse_optional_key_clause(&mut self) -> PResult<NodeList> {
        if self.consume(TokenKind::Key) {
            self.expect(TokenKind::Char('('))?;
            let columns = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            Ok(columns)
        } else {
            Ok(Vec::new())
        }
    }

    pub(super) fn parse_prop_graph_properties(&mut self) -> PResult<PropGraphProperties> {
        let location = self.location();
        if self.consume(TokenKind::No) {
            self.expect(TokenKind::Properties)?;
            return Ok(PropGraphProperties {
                node_tag: NodeTag::PropGraphProperties,
                location: location as ParseLoc,
                ..PropGraphProperties::default()
            });
        }
        self.expect(TokenKind::Properties)?;
        if self.consume(TokenKind::All) {
            self.expect(TokenKind::Columns)?;
            return Ok(PropGraphProperties {
                node_tag: NodeTag::PropGraphProperties,
                all: true,
                location: location as ParseLoc,
                ..PropGraphProperties::default()
            });
        }
        self.expect(TokenKind::Char('('))?;
        let properties = self.parse_res_target_list_strict_until(
            CompletionSlot::PropertyGraphPropertyExpression,
            CompletionSlot::PropertyGraphPropertyExpressionAfterComma,
            &[TokenKind::Char(')')],
        )?;
        if properties.is_empty() {
            return Err(self.error_here("PROPERTIES list cannot be empty"));
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(PropGraphProperties {
            node_tag: NodeTag::PropGraphProperties,
            properties,
            location: location as ParseLoc,
            ..PropGraphProperties::default()
        })
    }

    pub(super) fn parse_prop_graph_add_properties(&mut self) -> PResult<PropGraphProperties> {
        let location = self.previous_location();
        self.expect(TokenKind::Properties)?;
        self.expect(TokenKind::Char('('))?;
        let properties = self.parse_res_target_list_strict_until(
            CompletionSlot::PropertyGraphPropertyExpression,
            CompletionSlot::PropertyGraphPropertyExpressionAfterComma,
            &[TokenKind::Char(')')],
        )?;
        if properties.is_empty() {
            return Err(self.error_here("ADD PROPERTIES list cannot be empty"));
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(PropGraphProperties {
            node_tag: NodeTag::PropGraphProperties,
            properties,
            location: location as ParseLoc,
            ..PropGraphProperties::default()
        })
    }

    pub(super) fn parse_prop_graph_labels(&mut self) -> PResult<NodeList> {
        let mut labels = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Properties | TokenKind::No) {
            let location = self.location();
            let properties = Some(Box::new(self.parse_prop_graph_properties()?));
            labels.push(Node::PropGraphLabelAndProperties(
                PropGraphLabelAndProperties {
                    node_tag: NodeTag::PropGraphLabelAndProperties,
                    properties,
                    location: location as ParseLoc,
                    ..PropGraphLabelAndProperties::default()
                },
            ));
            return Ok(labels);
        }
        while matches!(self.peek_kind(), TokenKind::Label | TokenKind::Default) {
            let location = self.location();
            let label = if self.consume(TokenKind::Label) {
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("LABEL requires a name"))?,
                )
            } else {
                self.expect(TokenKind::Default)?;
                self.expect(TokenKind::Label)?;
                None
            };
            let properties = if matches!(self.peek_kind(), TokenKind::Properties | TokenKind::No) {
                Some(Box::new(self.parse_prop_graph_properties()?))
            } else {
                Some(Box::new(PropGraphProperties {
                    node_tag: NodeTag::PropGraphProperties,
                    all: true,
                    location: -1,
                    ..PropGraphProperties::default()
                }))
            };
            labels.push(Node::PropGraphLabelAndProperties(
                PropGraphLabelAndProperties {
                    node_tag: NodeTag::PropGraphLabelAndProperties,
                    label,
                    properties,
                    location: location as ParseLoc,
                },
            ));
        }
        if labels.is_empty() {
            labels.push(Node::PropGraphLabelAndProperties(
                PropGraphLabelAndProperties {
                    node_tag: NodeTag::PropGraphLabelAndProperties,
                    properties: Some(Box::new(PropGraphProperties {
                        node_tag: NodeTag::PropGraphProperties,
                        all: true,
                        location: -1,
                        ..PropGraphProperties::default()
                    })),
                    location: -1,
                    ..PropGraphLabelAndProperties::default()
                },
            ));
        }
        Ok(labels)
    }
}
