//! Parsing for `COMMENT` and `SECURITY LABEL` object descriptions.
//!
//! The module selects the identity grammar required by each describable object
//! kind before consuming the shared `IS` payload.

use super::*;

const DESCRIBED_OBJECT_STARTS: &[TokenKind] = &[
    TokenKind::Access,
    TokenKind::Aggregate,
    TokenKind::Collation,
    TokenKind::Column,
    TokenKind::ConversionP,
    TokenKind::Database,
    TokenKind::DomainP,
    TokenKind::Event,
    TokenKind::Extension,
    TokenKind::Foreign,
    TokenKind::Function,
    TokenKind::Index,
    TokenKind::Language,
    TokenKind::LargeP,
    TokenKind::Materialized,
    TokenKind::Procedure,
    TokenKind::Procedural,
    TokenKind::Property,
    TokenKind::Publication,
    TokenKind::Role,
    TokenKind::Routine,
    TokenKind::Schema,
    TokenKind::Sequence,
    TokenKind::Server,
    TokenKind::Statistics,
    TokenKind::Subscription,
    TokenKind::Table,
    TokenKind::Tablespace,
    TokenKind::TextP,
    TokenKind::TypeP,
    TokenKind::View,
];

const COMMENT_ONLY_OBJECT_STARTS: &[TokenKind] = &[
    TokenKind::Cast,
    TokenKind::Constraint,
    TokenKind::Operator,
    TokenKind::Policy,
    TokenKind::Rule,
    TokenKind::Transform,
    TokenKind::Trigger,
];

#[derive(Clone, Copy, Eq, PartialEq)]
enum DescribedObjectContext {
    Comment,
    SecurityLabel,
}

#[derive(Clone, Copy)]
enum DescribedIdentityKind {
    AnyName,
    Name,
}

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
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ ,
    // ... ]
    pub(super) fn parse_comment(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Comment)?;
        self.expect(TokenKind::On)?;
        let (objtype, object) = self.parse_described_object(DescribedObjectContext::Comment)?;
        self.expect(TokenKind::Is)?;
        let comment = if self.consume(TokenKind::NullP) {
            None
        } else {
            Some(self.consume_required_string("COMMENT text must be a string or NULL")?)
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
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ ,
    // ... ]
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
        let (objtype, object) =
            self.parse_described_object(DescribedObjectContext::SecurityLabel)?;
        self.expect(TokenKind::Is)?;
        let label = if self.consume(TokenKind::NullP) {
            None
        } else {
            Some(self.consume_required_string("security label must be a string or NULL")?)
        };
        Ok(Node::SecLabelStmt(SecLabelStmt {
            node_tag: NodeTag::SecLabelStmt,
            objtype,
            object: Some(Box::new(object)),
            provider,
            label,
        }))
    }

    fn parse_described_object(
        &mut self,
        context: DescribedObjectContext,
    ) -> PResult<(ObjectType, Node)> {
        self.record_completion_tokens(DESCRIBED_OBJECT_STARTS);
        if context == DescribedObjectContext::Comment {
            self.record_completion_tokens(COMMENT_ONLY_OBJECT_STARTS);
        }
        if self.consume(TokenKind::Column) {
            return Ok((
                ObjectType::Column,
                self.parse_any_name_object_until_is(completion::GrammarSlot::Column)?,
            ));
        }
        if self.consume(TokenKind::TypeP) {
            return Ok((
                ObjectType::Type,
                self.parse_type_object_until_is(completion::GrammarSlot::Type)?,
            ));
        }
        if self.consume(TokenKind::DomainP) {
            return Ok((
                ObjectType::Domain,
                self.parse_type_object_until_is(completion::GrammarSlot::Domain)?,
            ));
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
                Node::ObjectWithArgs(self.parse_object_with_args_until_with_slot(
                    &[TokenKind::Is],
                    completion::GrammarSlot::Procedure,
                )?),
            ));
        }
        if self.consume(TokenKind::Routine) {
            return Ok((
                ObjectType::Routine,
                Node::ObjectWithArgs(self.parse_object_with_args_until_with_slot(
                    &[TokenKind::Is],
                    completion::GrammarSlot::Routine,
                )?),
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

        if context == DescribedObjectContext::Comment {
            if self.consume(TokenKind::Operator) {
                self.record_completion_tokens(&[TokenKind::Class, TokenKind::Family]);
                self.record_completion_slot(completion::GrammarSlot::Operator);
                if self.consume(TokenKind::Class) || self.consume(TokenKind::Family) {
                    let is_family = self.tokens[self.pos - 1].kind == TokenKind::Family;
                    let name_slot = if is_family {
                        completion::GrammarSlot::OperatorFamily
                    } else {
                        completion::GrammarSlot::OperatorClass
                    };
                    self.record_completion_slot(name_slot);
                    self.record_completion_qualified_name_slot(name_slot, &[TokenKind::Using]);
                    let name_tokens = self.take_until_top_level(&[TokenKind::Using]);
                    let names = parse_any_name_tokens(&name_tokens)?;
                    self.expect(TokenKind::Using)?;
                    self.record_completion_slot(completion::GrammarSlot::AccessMethod);
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
                if self.at_completion() {
                    let mut completion_tokens = source_tokens.clone();
                    self.append_completion_marker(&mut completion_tokens);
                    record_type_name_completion(&completion_tokens, self.completion.as_ref());
                }
                self.expect(TokenKind::As)?;
                let target_tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
                if self.at_completion() {
                    let mut completion_tokens = target_tokens.clone();
                    self.append_completion_marker(&mut completion_tokens);
                    record_type_name_completion(&completion_tokens, self.completion.as_ref());
                }
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
                self.record_completion_slot(completion::GrammarSlot::Type);
                self.record_completion_qualified_name_slot(
                    completion::GrammarSlot::Type,
                    &[TokenKind::Language],
                );
                let type_tokens = self.take_until_top_level(&[TokenKind::Language]);
                self.expect(TokenKind::Language)?;
                self.record_completion_slot(completion::GrammarSlot::Language);
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
                self.record_completion_slot(completion::GrammarSlot::Constraint);
                let conname = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))?;
                self.expect(TokenKind::On)?;
                self.record_completion_tokens(&[TokenKind::DomainP]);
                if self.consume(TokenKind::DomainP) {
                    let domain =
                        self.parse_type_object_until_is(completion::GrammarSlot::Domain)?;
                    return Ok((
                        ObjectType::Domconstraint,
                        name_list_node(vec![domain, make_string_node(conname)]),
                    ));
                }
                self.record_completion_slot(completion::GrammarSlot::Table);
                let mut names =
                    self.parse_any_name_elements_until_is(completion::GrammarSlot::Table)?;
                names.push(make_string_node(conname));
                return Ok((ObjectType::Tabconstraint, name_list_node(names)));
            }
            for (kind, objtype) in [
                (TokenKind::Policy, ObjectType::Policy),
                (TokenKind::Rule, ObjectType::Rule),
                (TokenKind::Trigger, ObjectType::Trigger),
            ] {
                if self.consume(kind) {
                    self.record_completion_slot(completion::object_type_slot(objtype));
                    let name = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("object requires a name"))?;
                    self.expect(TokenKind::On)?;
                    self.record_completion_slot(completion::GrammarSlot::Table);
                    let mut elements =
                        self.parse_any_name_elements_until_is(completion::GrammarSlot::Table)?;
                    elements.push(make_string_node(name));
                    return Ok((objtype, name_list_node(elements)));
                }
            }
        }

        let (objtype, identity_kind) = self.parse_simple_described_object_type(context)?;
        let slot = completion::object_type_slot(objtype);
        let object = match identity_kind {
            DescribedIdentityKind::AnyName => self.parse_any_name_object_until_is(slot)?,
            DescribedIdentityKind::Name => {
                self.record_completion_slot(slot);
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

    fn parse_simple_described_object_type(
        &mut self,
        context: DescribedObjectContext,
    ) -> PResult<(ObjectType, DescribedIdentityKind)> {
        self.record_completion_tokens(&[
            TokenKind::Table,
            TokenKind::Sequence,
            TokenKind::View,
            TokenKind::Index,
            TokenKind::Collation,
            TokenKind::ConversionP,
            TokenKind::Statistics,
            TokenKind::Materialized,
            TokenKind::Foreign,
            TokenKind::Property,
            TokenKind::TextP,
            TokenKind::Access,
            TokenKind::Event,
            TokenKind::Extension,
            TokenKind::Procedural,
            TokenKind::Language,
            TokenKind::Publication,
            TokenKind::Schema,
            TokenKind::Server,
            TokenKind::Database,
            TokenKind::Role,
            TokenKind::Subscription,
            TokenKind::Tablespace,
        ]);
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
            self.record_completion_tokens(&[TokenKind::DataP, TokenKind::Table]);
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
            self.record_completion_tokens(&[
                TokenKind::Parser,
                TokenKind::Dictionary,
                TokenKind::Template,
                TokenKind::Configuration,
            ]);
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
                return Err(
                    self.error_here(if context == DescribedObjectContext::SecurityLabel {
                        "unsupported SECURITY LABEL object type"
                    } else {
                        "unsupported COMMENT object type"
                    }),
                );
            }
        };
        Ok((objtype, DescribedIdentityKind::Name))
    }

    fn parse_any_name_elements_until_is(
        &mut self,
        slot: completion::GrammarSlot,
    ) -> PResult<NodeList> {
        self.record_completion_slot(slot);
        self.record_completion_qualified_name_slot(slot, &[TokenKind::Is]);
        let tokens = self.take_until_top_level(&[TokenKind::Is]);
        parse_any_name_tokens(&tokens)
    }

    fn parse_any_name_object_until_is(&mut self, slot: completion::GrammarSlot) -> PResult<Node> {
        Ok(name_list_node(self.parse_any_name_elements_until_is(slot)?))
    }

    fn parse_type_object_until_is(&mut self, slot: completion::GrammarSlot) -> PResult<Node> {
        self.record_completion_slot(slot);
        self.record_completion_qualified_name_slot(slot, &[TokenKind::Is]);
        let tokens = self.take_until_top_level(&[TokenKind::Is]);
        Ok(Node::TypeName(parse_type_name_tokens(tokens)?))
    }
}
