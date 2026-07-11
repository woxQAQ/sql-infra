use super::*;

impl Parser {
    pub(super) fn parse_create_table(
        &mut self,
        foreign: bool,
        relpersistence: u8,
    ) -> PResult<Node> {
        self.expect(TokenKind::Table)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let mut relation_node = self
            .try_parse_qualified_range_var()
            .ok_or_else(|| self.error_here("CREATE TABLE requires a relation name"))?;
        relation_node.relpersistence = relpersistence;
        let relation = Some(Box::new(relation_node));
        if !foreign
            && self
                .has_top_level_token_before(TokenKind::As, &[TokenKind::Char(';'), TokenKind::Eof])
        {
            return self.parse_create_table_as_target(relation, if_not_exists);
        }
        let mut inh_relations = Vec::new();
        let mut partbound = None;
        let mut of_typename = None;
        let table_elts = if self.consume(TokenKind::Partition) {
            self.expect(TokenKind::Of)?;
            let parent = self
                .try_parse_qualified_range_var()
                .ok_or_else(|| self.error_here("expected a partitioned parent table"))?;
            inh_relations.push(Node::RangeVar(parent));
            let elements = if self.consume(TokenKind::Char('(')) {
                self.parse_typed_table_elements()?
            } else {
                Vec::new()
            };
            partbound = Some(Box::new(self.parse_partition_bound()?));
            elements
        } else if self.consume(TokenKind::Of) {
            let type_location = self.location();
            let names = self.consume_name_parts();
            if names.is_empty() {
                return Err(self.error_here("CREATE TABLE OF requires a type name"));
            }
            of_typename = Some(Box::new(TypeName {
                node_tag: NodeTag::TypeName,
                names: names.into_iter().map(make_string_node).collect(),
                location: type_location as ParseLoc,
                ..TypeName::default()
            }));
            if self.consume(TokenKind::Char('(')) {
                self.parse_typed_table_elements()?
            } else {
                Vec::new()
            }
        } else if self.consume(TokenKind::Char('(')) {
            self.parse_table_elements()?
        } else {
            return Err(
                self.error_here("CREATE TABLE requires a table element list, OF, or PARTITION OF")
            );
        };
        if self.consume(TokenKind::Inherits) {
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("INHERITS requires at least one parent relation"));
            }
            while !self.at(TokenKind::Char(')')) {
                let parent = self
                    .try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("expected an inherited relation"))?;
                inh_relations.push(Node::RangeVar(parent));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected an inherited relation after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
        }
        let (partspec, access_method, options, oncommit, tablespacename) = if foreign {
            (None, None, Vec::new(), OnCommitAction::Noop, None)
        } else {
            let partspec = if self.at(TokenKind::Partition) {
                Some(Box::new(self.parse_partition_spec()?))
            } else {
                None
            };
            let access_method = if self.consume(TokenKind::Using) {
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected an access method"))?,
                )
            } else {
                None
            };
            let options = if self.consume(TokenKind::With) {
                self.parse_parenthesized_reloptions()?
            } else {
                if self.consume(TokenKind::Without) {
                    self.expect(TokenKind::Oids)?;
                }
                Vec::new()
            };
            let oncommit = self.parse_on_commit_option()?;
            let tablespacename = if self.consume(TokenKind::Tablespace) {
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected a tablespace name"))?,
                )
            } else {
                None
            };
            (partspec, access_method, options, oncommit, tablespacename)
        };
        let create = CreateStmt {
            node_tag: NodeTag::CreateStmt,
            relation,
            table_elts,
            inh_relations,
            partbound,
            partspec,
            of_typename,
            options,
            oncommit,
            tablespacename,
            access_method,
            if_not_exists,
            ..CreateStmt::default()
        };
        if foreign {
            self.expect(TokenKind::Server)?;
            let servername = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("expected a foreign server name"))?,
            );
            let foreign_options = self.parse_options_clause()?;
            Ok(Node::CreateForeignTableStmt(CreateForeignTableStmt {
                base: create,
                servername,
                options: foreign_options,
            }))
        } else {
            Ok(Node::CreateStmt(create))
        }
    }

    fn parse_create_table_as_target(
        &mut self,
        relation: Option<Box<RangeVar>>,
        if_not_exists: bool,
    ) -> PResult<Node> {
        let col_names = if self.consume(TokenKind::Char('(')) {
            let names = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            names
        } else {
            Vec::new()
        };
        let access_method = if self.consume(TokenKind::Using) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            None
        };
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            if self.consume(TokenKind::Without) {
                self.expect(TokenKind::Oids)?;
            }
            Vec::new()
        };
        let on_commit = self.parse_on_commit_option()?;
        let table_space_name = if self.consume(TokenKind::Tablespace) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?,
            )
        } else {
            None
        };
        self.expect(TokenKind::As)?;
        if self.at(TokenKind::Execute) {
            let query = Some(Box::new(Node::ExecuteStmt(self.parse_execute_core()?)));
            let skip_data = self.parse_optional_with_data()?;
            self.expect_statement_end()?;
            return Ok(Node::CreateTableAsStmt(CreateTableAsStmt {
                node_tag: NodeTag::CreateTableAsStmt,
                query,
                into: Some(Box::new(IntoClause {
                    node_tag: NodeTag::IntoClause,
                    rel: relation,
                    col_names,
                    access_method,
                    options,
                    on_commit,
                    table_space_name,
                    skip_data,
                    ..IntoClause::default()
                })),
                objtype: ObjectType::Table,
                if_not_exists,
                ..CreateTableAsStmt::default()
            }));
        }
        let query_tokens = self.take_until_top_level(&[TokenKind::Char(';'), TokenKind::Eof]);
        let (query_tokens, skip_data) = split_with_data_suffix(query_tokens);
        let query = Some(Box::new(parse_select_statement_tokens(query_tokens)?));
        Ok(Node::CreateTableAsStmt(CreateTableAsStmt {
            node_tag: NodeTag::CreateTableAsStmt,
            query,
            into: Some(Box::new(IntoClause {
                node_tag: NodeTag::IntoClause,
                rel: relation,
                col_names,
                access_method,
                options,
                on_commit,
                table_space_name,
                skip_data,
                ..IntoClause::default()
            })),
            objtype: ObjectType::Table,
            if_not_exists,
            ..CreateTableAsStmt::default()
        }))
    }

    fn parse_on_commit_option(&mut self) -> PResult<OnCommitAction> {
        if !self.consume(TokenKind::On) {
            return Ok(OnCommitAction::Noop);
        }
        self.expect(TokenKind::Commit)?;
        if self.consume(TokenKind::Drop) {
            Ok(OnCommitAction::Drop)
        } else if self.consume(TokenKind::DeleteP) {
            self.expect(TokenKind::Rows)?;
            Ok(OnCommitAction::DeleteRows)
        } else {
            self.expect(TokenKind::Preserve)?;
            self.expect(TokenKind::Rows)?;
            Ok(OnCommitAction::PreserveRows)
        }
    }

    pub(super) fn parse_create_table_as(
        &mut self,
        objtype: ObjectType,
        relpersistence: u8,
    ) -> PResult<Node> {
        self.expect(TokenKind::View)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let mut relation = self
            .parse_plain_range_var()
            .ok_or_else(|| self.error_here("CREATE MATERIALIZED VIEW requires a name"))?;
        relation.relpersistence = relpersistence;
        let rel = Some(Box::new(relation));
        let col_names = if self.consume(TokenKind::Char('(')) {
            let names = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            names
        } else {
            Vec::new()
        };
        let access_method = if self.consume(TokenKind::Using) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            None
        };
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            Vec::new()
        };
        let table_space_name = if self.consume(TokenKind::Tablespace) {
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?,
            )
        } else {
            None
        };
        self.expect(TokenKind::As)?;
        let query_tokens = self.take_until_top_level(&[TokenKind::Char(';'), TokenKind::Eof]);
        let (query_tokens, skip_data) = split_with_data_suffix(query_tokens);
        let query = Some(Box::new(parse_select_statement_tokens(query_tokens)?));
        Ok(Node::CreateTableAsStmt(CreateTableAsStmt {
            node_tag: NodeTag::CreateTableAsStmt,
            query,
            into: Some(Box::new(IntoClause {
                node_tag: NodeTag::IntoClause,
                rel,
                col_names,
                access_method,
                options,
                table_space_name,
                skip_data,
                ..IntoClause::default()
            })),
            objtype,
            if_not_exists,
            ..CreateTableAsStmt::default()
        }))
    }
}
pub(super) fn split_with_data_suffix(mut tokens: Vec<Token>) -> (Vec<Token>, bool) {
    let len = tokens.len();
    if len >= 3
        && tokens[len - 3].kind == TokenKind::With
        && tokens[len - 2].kind == TokenKind::No
        && tokens[len - 1].kind == TokenKind::DataP
    {
        tokens.truncate(len - 3);
        return (tokens, true);
    }
    if len >= 2
        && tokens[len - 2].kind == TokenKind::With
        && tokens[len - 1].kind == TokenKind::DataP
    {
        tokens.truncate(len - 2);
    }
    (tokens, false)
}
