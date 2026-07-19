use super::*;

impl Parser {
    pub(super) fn parse_statement_range(&self, range: std::ops::Range<usize>) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            return Err(ParseError::new(location, "expected a statement"));
        }
        let mut nested = self.bounded_view(range);
        let node = nested.parse_statement(None)?;
        if !nested.at(TokenKind::Eof) {
            return Err(nested.error_here("unexpected token after nested statement"));
        }
        Ok(node)
    }

    pub(super) fn parse_select_statement_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        let node = self.parse_statement_range(range)?;
        if matches!(node, Node::SelectStmt(_)) {
            Ok(node)
        } else {
            Err(ParseError::new(location, "expected a SELECT statement"))
        }
    }

    pub(super) fn parse_preparable_statement_range(
        &self,
        range: std::ops::Range<usize>,
    ) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        let node = self.parse_statement_range(range)?;
        if matches!(
            node,
            Node::SelectStmt(_)
                | Node::InsertStmt(_)
                | Node::UpdateStmt(_)
                | Node::DeleteStmt(_)
                | Node::MergeStmt(_)
        ) {
            Ok(node)
        } else {
            Err(ParseError::new(location, "expected a preparable statement"))
        }
    }
}

pub(super) fn parse_transformed_statement_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.last().map_or(0, Token::end_location);
    if tokens.is_empty() {
        return Err(ParseError::new(location, "expected a statement"));
    }
    let mut parser = Parser::from_transformed_tokens(tokens);
    let node = parser.parse_statement(None)?;
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after nested statement"));
    }
    Ok(node)
}

pub(super) fn parse_type_node_list(tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.last().map(|token| token.kind) == Some(TokenKind::Char(',')) {
        return Err(ParseError::new(location, "type list cannot end with ','"));
    }
    let chunks = split_top_level_commas(tokens);
    if chunks.is_empty() {
        return Err(ParseError::new(location, "type list cannot be empty"));
    }
    chunks
        .into_iter()
        .map(|tokens| parse_type_name_tokens(tokens).map(Node::TypeName))
        .collect()
}
