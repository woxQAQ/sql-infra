use super::*;

impl Parser {
    pub(super) fn parse_alter_generic_options(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Options)?;
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("OPTIONS list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            let action = if self.consume(TokenKind::Set) {
                DefElemAction::Set
            } else if self.consume(TokenKind::AddP) {
                DefElemAction::Add
            } else if self.consume(TokenKind::Drop) {
                DefElemAction::Drop
            } else {
                DefElemAction::Unspec
            };
            let location = self.location();
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
