//! Table columns, typed-table elements, and column qualifiers.
//!
//! Column definitions, generated/identity forms, defaults, storage, compression,
//! and embedded constraints are parsed before `create_table` assembles the table.

use super::*;

impl Parser {
    pub(super) fn parse_insert_column_list(&mut self) -> PResult<NodeList> {
        self.record_completion_slot(completion::GrammarSlot::Column);
        let mut cols = Vec::new();
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("column list cannot be empty"));
        }
        while !self.at(TokenKind::Char(')')) {
            let location = self.location();
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a column name"))?;
            let indirection = self.parse_assignment_indirection()?;
            cols.push(Node::ResTarget(ResTarget {
                name: Some(name),
                indirection,
                location: location as ParseLoc,
                ..ResTarget::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a column name after ','"));
            }
        }
        Ok(cols)
    }

    pub(super) fn parse_table_elements(&mut self) -> PResult<NodeList> {
        let mut elements = Vec::new();
        while !self.at(TokenKind::Char(')')) {
            if self.consume(TokenKind::Like) {
                let relation = self
                    .try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                    .ok_or_else(|| self.error_here("expected a relation after LIKE"))?;
                let mut options = 0u32;
                self.record_completion_follow_tokens(&[TokenKind::Including, TokenKind::Excluding]);
                while matches!(
                    self.peek_kind(),
                    TokenKind::Including | TokenKind::Excluding
                ) {
                    let include = self.consume(TokenKind::Including);
                    if !include {
                        self.expect(TokenKind::Excluding)?;
                    }
                    self.record_completion_tokens(&[
                        TokenKind::Comments,
                        TokenKind::Compression,
                        TokenKind::Constraints,
                        TokenKind::Defaults,
                        TokenKind::Generated,
                        TokenKind::IdentityP,
                        TokenKind::Indexes,
                        TokenKind::Statistics,
                        TokenKind::Storage,
                        TokenKind::All,
                    ]);
                    let option = match self.advance().kind {
                        TokenKind::Comments => TableLikeOption::Comments as u32,
                        TokenKind::Compression => TableLikeOption::Compression as u32,
                        TokenKind::Constraints => TableLikeOption::Constraints as u32,
                        TokenKind::Defaults => TableLikeOption::Defaults as u32,
                        TokenKind::Generated => TableLikeOption::Generated as u32,
                        TokenKind::IdentityP => TableLikeOption::Identity as u32,
                        TokenKind::Indexes => TableLikeOption::Indexes as u32,
                        TokenKind::Statistics => TableLikeOption::Statistics as u32,
                        TokenKind::Storage => TableLikeOption::Storage as u32,
                        TokenKind::All => TableLikeOption::All as u32,
                        other => {
                            return Err(
                                self.error_here(format!("unsupported LIKE option {:?}", other))
                            );
                        }
                    };
                    if include {
                        options |= option;
                    } else {
                        options &= !option;
                    }
                    self.record_completion_follow_tokens(&[
                        TokenKind::Including,
                        TokenKind::Excluding,
                    ]);
                }
                elements.push(Node::TableLikeClause(TableLikeClause {
                    relation: Some(Box::new(relation)),
                    options,
                    ..TableLikeClause::default()
                }));
            } else {
                let location = self.location();
                let mut chunk =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                self.record_completion_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
                self.append_completion_marker(&mut chunk);
                elements.push(
                    parse_table_element_tokens_with_completion(chunk, self.completion.clone())
                        .map_err(|mut error| {
                            if error.location() == 0 {
                                error.reanchor(location);
                            }
                            error
                        })?,
                );
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a table element after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(elements)
    }

    pub(super) fn parse_typed_table_elements(&mut self) -> PResult<NodeList> {
        let mut elements = Vec::new();
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("typed table element list cannot be empty"));
        }
        while !self.at(TokenKind::Char(')')) {
            let location = self.location();
            let mut chunk =
                self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            self.record_completion_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
            self.append_completion_marker(&mut chunk);
            elements.push(
                parse_typed_table_element_tokens_with_completion(chunk, self.completion.clone())
                    .map_err(|mut error| {
                        if error.location() == 0 {
                            error.reanchor(location);
                        }
                        error
                    })?,
            );
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a typed table element after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(elements)
    }

    pub(super) fn parse_table_element_inner(&mut self) -> PResult<Node> {
        self.record_completion_tokens(&[
            TokenKind::Constraint,
            TokenKind::Check,
            TokenKind::Not,
            TokenKind::Unique,
            TokenKind::Primary,
            TokenKind::Foreign,
            TokenKind::Exclude,
        ]);
        let node = if matches!(
            self.peek_kind(),
            TokenKind::Constraint
                | TokenKind::Check
                | TokenKind::Not
                | TokenKind::Unique
                | TokenKind::Primary
                | TokenKind::Foreign
                | TokenKind::Exclude
        ) {
            Node::Constraint(self.parse_table_constraint()?)
        } else {
            Node::ColumnDef(self.parse_column_definition()?)
        };
        self.expect(TokenKind::Eof)?;
        Ok(node)
    }

    pub(super) fn parse_typed_table_element_inner(&mut self) -> PResult<Node> {
        self.record_completion_tokens(&[
            TokenKind::Constraint,
            TokenKind::Check,
            TokenKind::Not,
            TokenKind::Unique,
            TokenKind::Primary,
            TokenKind::Foreign,
            TokenKind::Exclude,
        ]);
        let node = if matches!(
            self.peek_kind(),
            TokenKind::Constraint
                | TokenKind::Check
                | TokenKind::Not
                | TokenKind::Unique
                | TokenKind::Primary
                | TokenKind::Foreign
                | TokenKind::Exclude
        ) {
            Node::Constraint(self.parse_table_constraint()?)
        } else {
            Node::ColumnDef(self.parse_typed_column_options()?)
        };
        self.expect(TokenKind::Eof)?;
        Ok(node)
    }

    pub(super) fn parse_column_definition(&mut self) -> PResult<ColumnDef> {
        let location = self.location();
        let colname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("expected a column name"))?,
        );
        let type_name = Some(Box::new(
            self.parse_type_name_until(&[
                TokenKind::Storage,
                TokenKind::Compression,
                TokenKind::Options,
                TokenKind::Constraint,
                TokenKind::Collate,
                TokenKind::Not,
                TokenKind::NullP,
                TokenKind::Unique,
                TokenKind::Primary,
                TokenKind::Check,
                TokenKind::Default,
                TokenKind::Generated,
                TokenKind::References,
                TokenKind::Deferrable,
                TokenKind::Initially,
                TokenKind::Enforced,
                TokenKind::Eof,
            ])
            .ok_or_else(|| self.error_here("column requires a data type"))?,
        ));
        let storage_name = if self.consume(TokenKind::Storage) {
            Some(if self.consume(TokenKind::Default) {
                "default".to_owned()
            } else {
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("STORAGE requires a mode"))?
            })
        } else {
            None
        };
        let compression = if self.consume(TokenKind::Compression) {
            Some(if self.consume(TokenKind::Default) {
                "default".to_owned()
            } else {
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("COMPRESSION requires a method"))?
            })
        } else {
            None
        };
        let fdwoptions = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };

        let (constraints, coll_clause) = self.parse_column_qualifiers()?;

        Ok(ColumnDef {
            colname,
            type_name,
            compression,
            is_local: true,
            storage_name,
            coll_clause,
            constraints,
            fdwoptions,
            location: location as ParseLoc,
            ..ColumnDef::default()
        })
    }

    pub(super) fn parse_typed_column_options(&mut self) -> PResult<ColumnDef> {
        let location = self.location();
        self.record_completion_slot(completion::GrammarSlot::Column);
        let colname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("expected a typed table column name"))?,
        );
        if self.consume(TokenKind::With) {
            self.expect(TokenKind::Options)?;
        }
        let (constraints, coll_clause) = self.parse_column_qualifiers()?;
        Ok(ColumnDef {
            colname,
            is_local: true,
            coll_clause,
            constraints,
            location: location as ParseLoc,
            ..ColumnDef::default()
        })
    }

    pub(super) fn parse_column_qualifiers(
        &mut self,
    ) -> PResult<(NodeList, Option<Box<CollateClause>>)> {
        let mut constraints = Vec::new();
        let mut coll_clause = None;
        while !self.at(TokenKind::Eof) {
            if self.consume(TokenKind::Collate) {
                self.record_completion_slot(completion::GrammarSlot::Collation);
                let coll_location = self.previous_location();
                let collname = self.parse_name_list();
                if collname.is_empty() {
                    return Err(self.error_here("COLLATE requires a collation name"));
                }
                if coll_clause.is_some() {
                    return Err(self.error_here("multiple COLLATE clauses are not allowed"));
                }
                coll_clause = Some(Box::new(CollateClause {
                    collname,
                    location: coll_location as ParseLoc,
                    ..CollateClause::default()
                }));
                continue;
            }
            let con_location = self.location();
            let conname = if self.consume(TokenKind::Constraint) {
                self.record_completion_slot(completion::GrammarSlot::Constraint);
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?,
                )
            } else {
                None
            };
            let mut constraint = self.parse_column_constraint_element(con_location)?;
            if conname.is_some()
                && matches!(
                    constraint.contype,
                    ConstrType::AttrDeferrable
                        | ConstrType::AttrNotDeferrable
                        | ConstrType::AttrDeferred
                        | ConstrType::AttrImmediate
                        | ConstrType::AttrEnforced
                        | ConstrType::AttrNotEnforced
                )
            {
                return Err(self.error_here("CONSTRAINT name must precede a column constraint"));
            }
            constraint.conname = conname;
            constraints.push(Node::Constraint(constraint));
        }
        Ok((constraints, coll_clause))
    }
}
pub(super) fn parse_table_element_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<Node> {
    let location = tokens.last().map_or(0, Token::end_location);
    tokens.push(Token::synthetic(TokenKind::Eof, location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    parser.parse_table_element_inner()
}
pub(super) fn parse_typed_table_element_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<Node> {
    let location = tokens.last().map_or(0, Token::end_location);
    tokens.push(Token::synthetic(TokenKind::Eof, location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    parser.parse_typed_table_element_inner()
}
