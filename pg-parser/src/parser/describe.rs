use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-comment.html
    // COMMENT ON
    // {
    //   ACCESS METHOD object_name |
    //   AGGREGATE aggregate_name ( aggregate_signature ) |
    //   CAST (source_type AS target_type) |
    //   COLLATION object_name |
    //   COLUMN relation_name.column_name |
    //   CONSTRAINT constraint_name ON table_name |
    //   CONSTRAINT constraint_name ON DOMAIN domain_name |
    //   CONVERSION object_name |
    //   DATABASE object_name |
    //   DOMAIN object_name |
    //   EXTENSION object_name |
    //   EVENT TRIGGER object_name |
    //   FOREIGN DATA WRAPPER object_name |
    //   FOREIGN TABLE object_name |
    //   FUNCTION function_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   INDEX object_name |
    //   LARGE OBJECT large_object_oid |
    //   MATERIALIZED VIEW object_name |
    //   OPERATOR operator_name (left_type, right_type) |
    //   OPERATOR CLASS object_name USING index_method |
    //   OPERATOR FAMILY object_name USING index_method |
    //   POLICY policy_name ON table_name |
    //   [ PROCEDURAL ] LANGUAGE object_name |
    //   PROCEDURE procedure_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   PUBLICATION object_name |
    //   ROLE object_name |
    //   ROUTINE routine_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   RULE rule_name ON table_name |
    //   SCHEMA object_name |
    //   SEQUENCE object_name |
    //   SERVER object_name |
    //   STATISTICS object_name |
    //   SUBSCRIPTION object_name |
    //   TABLE object_name |
    //   TABLESPACE object_name |
    //   TEXT SEARCH CONFIGURATION object_name |
    //   TEXT SEARCH DICTIONARY object_name |
    //   TEXT SEARCH PARSER object_name |
    //   TEXT SEARCH TEMPLATE object_name |
    //   TRANSFORM FOR type_name LANGUAGE lang_name |
    //   TRIGGER trigger_name ON table_name |
    //   TYPE object_name |
    //   VIEW object_name
    // } IS { string_literal | NULL }
    //
    // where aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    pub(super) fn parse_comment(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Comment)?;
        self.expect(TokenKind::On)?;
        let (objtype, object) = self.parse_described_object(false)?;
        self.expect(TokenKind::Is)?;
        let comment = if self.consume(TokenKind::NullP) {
            None
        } else {
            if !self.at(TokenKind::SConst) {
                return Err(self.error_here("COMMENT text must be a string or NULL"));
            }
            self.consume_string_like()
        };
        Ok(Node::CommentStmt(CommentStmt {
            node_tag: NodeTag::CommentStmt,
            objtype,
            object: Some(Box::new(object)),
            comment,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-security-label.html
    // SECURITY LABEL [ FOR provider ] ON
    // {
    //   TABLE object_name |
    //   COLUMN table_name.column_name |
    //   AGGREGATE aggregate_name ( aggregate_signature ) |
    //   DATABASE object_name |
    //   DOMAIN object_name |
    //   EVENT TRIGGER object_name |
    //   FOREIGN TABLE object_name |
    //   FUNCTION function_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   LARGE OBJECT large_object_oid |
    //   MATERIALIZED VIEW object_name |
    //   [ PROCEDURAL ] LANGUAGE object_name |
    //   PROCEDURE procedure_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   PUBLICATION object_name |
    //   ROLE object_name |
    //   ROUTINE routine_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   SCHEMA object_name |
    //   SEQUENCE object_name |
    //   SUBSCRIPTION object_name |
    //   TABLESPACE object_name |
    //   TYPE object_name |
    //   VIEW object_name
    // } IS { string_literal | NULL }
    //
    // where aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    pub(super) fn parse_security_label(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Security)?;
        self.expect(TokenKind::Label)?;
        let provider = if self.consume(TokenKind::For) {
            Some(
                self.consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("SECURITY LABEL FOR requires a provider"))?,
            )
        } else {
            None
        };
        self.expect(TokenKind::On)?;
        let (objtype, object) = self.parse_described_object(true)?;
        self.expect(TokenKind::Is)?;
        let label = if self.consume(TokenKind::NullP) {
            None
        } else {
            if !self.at(TokenKind::SConst) {
                return Err(self.error_here("security label must be a string or NULL"));
            }
            self.consume_string_like()
        };
        Ok(Node::SecLabelStmt(SecLabelStmt {
            node_tag: NodeTag::SecLabelStmt,
            objtype,
            object: Some(Box::new(object)),
            provider,
            label,
        }))
    }

    fn parse_described_object(&mut self, security_label: bool) -> PResult<(ObjectType, Node)> {
        if self.consume(TokenKind::Column) {
            return Ok((ObjectType::Column, self.parse_described_column_until_is()?));
        }
        if self.consume(TokenKind::TypeP) {
            return Ok((ObjectType::Type, self.parse_type_object_until_is()?));
        }
        if self.consume(TokenKind::DomainP) {
            return Ok((ObjectType::Domain, self.parse_type_object_until_is()?));
        }
        if self.consume(TokenKind::Aggregate) {
            return Ok((
                ObjectType::Aggregate,
                Node::ObjectWithArgs(self.parse_aggregate_with_args_until(&[TokenKind::Is])?),
            ));
        }
        if self.consume(TokenKind::Function) {
            return Ok((
                ObjectType::Function,
                Node::ObjectWithArgs(self.parse_object_with_args_until(&[TokenKind::Is])?),
            ));
        }
        if self.consume(TokenKind::Procedure) {
            return Ok((
                ObjectType::Procedure,
                Node::ObjectWithArgs(self.parse_object_with_args_until(&[TokenKind::Is])?),
            ));
        }
        if self.consume(TokenKind::Routine) {
            return Ok((
                ObjectType::Routine,
                Node::ObjectWithArgs(self.parse_object_with_args_until(&[TokenKind::Is])?),
            ));
        }
        if self.consume(TokenKind::LargeP) {
            self.expect(TokenKind::ObjectP)?;
            let value = self.parse_numeric_only()?;
            if !self.at(TokenKind::Is) {
                return Err(self.error_here("unexpected token after large object identifier"));
            }
            return Ok((ObjectType::Largeobject, value));
        }

        if !security_label {
            if self.consume(TokenKind::Operator) {
                if self.consume(TokenKind::Class) || self.consume(TokenKind::Family) {
                    let is_family = self.tokens[self.pos - 1].kind == TokenKind::Family;
                    let name_tokens = self.take_until_top_level(&[TokenKind::Using]);
                    let names = parse_any_name_tokens(&name_tokens)?;
                    self.expect(TokenKind::Using)?;
                    let method = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("USING requires an access method name"))?;
                    if !self.at(TokenKind::Is) {
                        return Err(self
                            .error_here("unexpected token after operator class/family identity"));
                    }
                    let mut elements = vec![make_string_node(method)];
                    elements.extend(names);
                    return Ok((
                        if is_family {
                            ObjectType::Opfamily
                        } else {
                            ObjectType::Opclass
                        },
                        name_list_node(elements),
                    ));
                }
                return Ok((
                    ObjectType::Operator,
                    Node::ObjectWithArgs(self.parse_operator_with_args_until(&[TokenKind::Is])?),
                ));
            }
            if self.consume(TokenKind::Cast) {
                self.expect(TokenKind::Char('('))?;
                let source_tokens = self.take_until_top_level(&[TokenKind::As]);
                self.expect(TokenKind::As)?;
                let target_tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
                self.expect(TokenKind::Char(')'))?;
                if !self.at(TokenKind::Is) {
                    return Err(self.error_here("unexpected token after CAST identity"));
                }
                return Ok((
                    ObjectType::Cast,
                    name_list_node(vec![
                        Node::TypeName(parse_type_name_tokens(source_tokens)?),
                        Node::TypeName(parse_type_name_tokens(target_tokens)?),
                    ]),
                ));
            }
            if self.consume(TokenKind::Transform) {
                self.expect(TokenKind::For)?;
                let type_tokens = self.take_until_top_level(&[TokenKind::Language]);
                self.expect(TokenKind::Language)?;
                let language = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("LANGUAGE requires a name"))?;
                if !self.at(TokenKind::Is) {
                    return Err(self.error_here("unexpected token after TRANSFORM identity"));
                }
                return Ok((
                    ObjectType::Transform,
                    name_list_node(vec![
                        Node::TypeName(parse_type_name_tokens(type_tokens)?),
                        make_string_node(language),
                    ]),
                ));
            }
            if self.consume(TokenKind::Constraint) {
                let conname = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?;
                self.expect(TokenKind::On)?;
                if self.consume(TokenKind::DomainP) {
                    let domain = self.parse_type_object_until_is()?;
                    return Ok((
                        ObjectType::Domconstraint,
                        name_list_node(vec![domain, make_string_node(conname)]),
                    ));
                }
                let mut names = self.parse_any_name_elements_until_is()?;
                names.push(make_string_node(conname));
                return Ok((ObjectType::Tabconstraint, name_list_node(names)));
            }
            for (kind, objtype) in [
                (TokenKind::Policy, ObjectType::Policy),
                (TokenKind::Rule, ObjectType::Rule),
                (TokenKind::Trigger, ObjectType::Trigger),
            ] {
                if self.consume(kind) {
                    let name = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("object requires a name"))?;
                    self.expect(TokenKind::On)?;
                    let mut elements = self.parse_any_name_elements_until_is()?;
                    elements.push(make_string_node(name));
                    return Ok((objtype, name_list_node(elements)));
                }
            }
        }

        let (objtype, identity_kind) = self.parse_simple_described_object_type(security_label)?;
        let object = match identity_kind {
            DescribedIdentityKind::AnyName => self.parse_any_name_object_until_is()?,
            DescribedIdentityKind::Name => {
                let name = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("object requires a name"))?;
                if !self.at(TokenKind::Is) {
                    return Err(self.error_here("unexpected token after object name"));
                }
                make_string_node(name)
            }
        };
        Ok((objtype, object))
    }

    fn parse_described_column_until_is(&mut self) -> PResult<Node> {
        if self.at_completion_cursor() {
            self.record_completion_at(
                CompletionSlot::ObjectColumnName,
                Expectation::Name(NameExpectation::Relation { schema: None }),
            );
            self.record_completion_at(
                CompletionSlot::ObjectColumnName,
                Expectation::Name(NameExpectation::Schema),
            );
            return Err(self.completion_stop());
        }

        let tokens = self.take_until_top_level(&[TokenKind::Is]);
        if self.at_completion_cursor()
            && tokens
                .last()
                .is_some_and(|token| token.kind == TokenKind::Char('.'))
        {
            if let Some(qualifier) = tokens
                .get(tokens.len().saturating_sub(2))
                .and_then(token_name)
            {
                self.record_completion_at(
                    CompletionSlot::ObjectColumnName,
                    Expectation::Name(NameExpectation::Column(ColumnContext::Qualified(qualifier))),
                );
            }
            return Err(self.completion_stop());
        }
        Ok(name_list_node(parse_any_name_tokens(&tokens)?))
    }

    fn parse_simple_described_object_type(
        &mut self,
        security_label: bool,
    ) -> PResult<(ObjectType, DescribedIdentityKind)> {
        let any_name = match self.peek_kind() {
            TokenKind::Table => Some(ObjectType::Table),
            TokenKind::Sequence => Some(ObjectType::Sequence),
            TokenKind::View => Some(ObjectType::View),
            TokenKind::Index => Some(ObjectType::Index),
            TokenKind::Collation => Some(ObjectType::Collation),
            TokenKind::ConversionP => Some(ObjectType::Conversion),
            TokenKind::Statistics => Some(ObjectType::StatisticExt),
            _ => None,
        };
        if let Some(objtype) = any_name {
            self.advance();
            return Ok((objtype, DescribedIdentityKind::AnyName));
        }
        if self.consume(TokenKind::Materialized) {
            self.expect(TokenKind::View)?;
            return Ok((ObjectType::Matview, DescribedIdentityKind::AnyName));
        }
        if self.consume(TokenKind::Foreign) {
            if self.consume(TokenKind::Table) {
                return Ok((ObjectType::ForeignTable, DescribedIdentityKind::AnyName));
            }
            self.expect(TokenKind::DataP)?;
            self.expect(TokenKind::Wrapper)?;
            return Ok((ObjectType::Fdw, DescribedIdentityKind::Name));
        }
        if self.consume(TokenKind::Property) {
            self.expect(TokenKind::Graph)?;
            return Ok((ObjectType::Propgraph, DescribedIdentityKind::AnyName));
        }
        if self.consume(TokenKind::TextP) {
            self.expect(TokenKind::Search)?;
            let objtype = match self.peek_kind() {
                TokenKind::Parser => ObjectType::Tsparser,
                TokenKind::Dictionary => ObjectType::Tsdictionary,
                TokenKind::Template => ObjectType::Tstemplate,
                TokenKind::Configuration => ObjectType::Tsconfiguration,
                _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
            };
            self.advance();
            return Ok((objtype, DescribedIdentityKind::AnyName));
        }
        let objtype = match self.peek_kind() {
            TokenKind::Access => {
                self.advance();
                self.expect(TokenKind::Method)?;
                ObjectType::AccessMethod
            }
            TokenKind::Event => {
                self.advance();
                self.expect(TokenKind::Trigger)?;
                ObjectType::EventTrigger
            }
            TokenKind::Extension => {
                self.advance();
                ObjectType::Extension
            }
            TokenKind::Procedural => {
                self.advance();
                self.expect(TokenKind::Language)?;
                ObjectType::Language
            }
            TokenKind::Language => {
                self.advance();
                ObjectType::Language
            }
            TokenKind::Publication => {
                self.advance();
                ObjectType::Publication
            }
            TokenKind::Schema => {
                self.advance();
                ObjectType::Schema
            }
            TokenKind::Server => {
                self.advance();
                ObjectType::ForeignServer
            }
            TokenKind::Database => {
                self.advance();
                ObjectType::Database
            }
            TokenKind::Role => {
                self.advance();
                ObjectType::Role
            }
            TokenKind::Subscription => {
                self.advance();
                ObjectType::Subscription
            }
            TokenKind::Tablespace => {
                self.advance();
                ObjectType::Tablespace
            }
            _ => {
                return Err(self.error_here(if security_label {
                    "unsupported SECURITY LABEL object type"
                } else {
                    "unsupported COMMENT object type"
                }));
            }
        };
        Ok((objtype, DescribedIdentityKind::Name))
    }

    fn parse_any_name_elements_until_is(&mut self) -> PResult<NodeList> {
        let tokens = self.take_until_top_level(&[TokenKind::Is]);
        parse_any_name_tokens(&tokens)
    }

    fn parse_any_name_object_until_is(&mut self) -> PResult<Node> {
        Ok(name_list_node(self.parse_any_name_elements_until_is()?))
    }

    fn parse_type_object_until_is(&mut self) -> PResult<Node> {
        let tokens = self.take_until_top_level(&[TokenKind::Is]);
        Ok(Node::TypeName(parse_type_name_tokens(tokens)?))
    }
}
