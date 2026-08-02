//! Operator definition options and `ALTER OPERATOR` parsing.
//!
//! Operator-specific procedure, selectivity, join, commutator, negator, and
//! property clauses become typed raw definition elements here.

use super::*;

impl Parser {
    pub(super) fn parse_operator_definition_list(
        &mut self,
        object_type: ObjectType,
    ) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("ALTER TYPE option list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected an ALTER TYPE option name"))?;
            let arg = if self.consume(TokenKind::Char('=')) {
                if let Some(slot) = completion::definition_value_slot(object_type, &name) {
                    self.record_completion_slot(slot);
                    self.record_completion_slot_within_fragment(
                        slot,
                        &[TokenKind::Char(','), TokenKind::Char(')')],
                    );
                }
                if self.consume(TokenKind::None) {
                    None
                } else {
                    let tokens =
                        self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                    Some(Box::new(parse_operator_def_arg(&name, tokens, location)?))
                }
            } else {
                None
            };
            options.push(Node::DefElem(DefElem {
                node_tag: NodeTag::DefElem,
                defname: Some(name),
                arg,
                location: location as ParseLoc,
                ..DefElem::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an ALTER TYPE option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteroperator.html
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET SCHEMA new_schema
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET ( {  RESTRICT = { res_proc | NONE }
    //            | JOIN = { join_proc | NONE }
    //            | COMMUTATOR = com_op
    //            | NEGATOR = neg_op
    //            | HASHES
    //            | MERGES
    //           } [, ... ] )
    pub(super) fn parse_alter_operator(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Operator)?;
        self.record_completion_tokens(&[TokenKind::Family]);
        let opername = Some(Box::new(self.parse_operator_with_args_until(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?));
        self.expect(TokenKind::Set)?;
        let options = self.parse_operator_definition_list(ObjectType::Operator)?;
        self.expect_statement_end()?;
        Ok(Node::AlterOperatorStmt(AlterOperatorStmt {
            node_tag: NodeTag::AlterOperatorStmt,
            opername,
            options,
        }))
    }
}
