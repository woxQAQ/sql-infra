//! PostgreSQL's generic `OPTIONS` grammar for foreign-data objects.
//!
//! Create and alter forms differ in permitted actions but share option-name and
//! value parsing here.

use super::*;

impl Parser {
    pub(super) fn parse_create_generic_options(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Options)?;
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("OPTIONS list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected an option name"))?;
            let value = self.consume_required_string("option value must be a string literal")?;
            options.push(make_def_elem(
                &name,
                Some(make_string_node(value)),
                location,
            ));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }

    pub(super) fn parse_alter_generic_options(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Options)?;
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("OPTIONS list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            self.record_completion_tokens(&[TokenKind::Set, TokenKind::AddP, TokenKind::Drop]);
            let action = match self.peek_kind() {
                TokenKind::Set => {
                    self.advance();
                    DefElemAction::Set
                }
                TokenKind::AddP => {
                    self.advance();
                    DefElemAction::Add
                }
                TokenKind::Drop => {
                    self.advance();
                    DefElemAction::Drop
                }
                _ => DefElemAction::Unspec,
            };
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected an option name"))?;
            let arg = if action == DefElemAction::Drop {
                None
            } else {
                Some(Box::new(make_string_node(self.consume_required_string(
                    "option value must be a string literal",
                )?)))
            };
            options.push(Node::DefElem(DefElem {
                node_tag: NodeTag::DefElem,
                defname: Some(name),
                arg,
                defaction: action,
                location: location as ParseLoc,
                ..DefElem::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }
}
