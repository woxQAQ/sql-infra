use super::*;

impl Parser {
    pub(super) fn parse_operator_definition_list(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("ALTER TYPE option list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            let location = self.location();
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected an ALTER TYPE option name"))?;
            let arg = if self.consume(TokenKind::Char('=')) {
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

    pub(super) fn parse_alter_operator(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Operator)?;
        let opername = Some(Box::new(self.parse_operator_with_args_until(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?));
        self.expect(TokenKind::Set)?;
        let options = self.parse_operator_definition_list()?;
        self.expect_statement_end()?;
        Ok(Node::AlterOperatorStmt(AlterOperatorStmt {
            node_tag: NodeTag::AlterOperatorStmt,
            opername,
            options,
        }))
    }
}
