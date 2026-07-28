use super::*;

pub(super) fn tokens_end_at_top_level(tokens: &[Token]) -> bool {
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
    }
    depth == 0
}

pub(super) fn parse_statement_node_tokens(tokens: Vec<Token>) -> PResult<Node> {
    parse_statement_node_tokens_with_completion(tokens, None)
}

pub(super) fn parse_statement_node_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<Node> {
    let location = tokens.last().map_or(0, Token::end_location);
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "expected a statement"));
    }
    tokens.push(Token::synthetic(TokenKind::Eof, location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    let node = parser.parse_statement(None)?;
    if !parser.at(TokenKind::Eof) {
        return Err(parser.error_here("unexpected token after nested statement"));
    }
    Ok(node)
}

pub(super) fn parse_select_statement_tokens(tokens: Vec<Token>) -> PResult<Node> {
    parse_select_statement_tokens_with_completion(tokens, None)
}

pub(super) fn parse_select_statement_tokens_with_completion(
    tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location());
    let node = parse_statement_node_tokens_with_completion(tokens, completion)?;
    if matches!(node, Node::SelectStmt(_)) {
        Ok(node)
    } else {
        Err(ParseError::syntax_exit(
            location,
            "expected a SELECT statement",
        ))
    }
}

pub(super) fn parse_preparable_statement_tokens(tokens: Vec<Token>) -> PResult<Node> {
    parse_preparable_statement_tokens_with_completion(tokens, None)
}

pub(super) fn parse_preparable_statement_tokens_with_completion(
    tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location());
    let node = parse_statement_node_tokens_with_completion(tokens, completion)?;
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
        Err(ParseError::syntax_exit(
            location,
            "expected a preparable statement",
        ))
    }
}

pub(super) fn parse_type_node_list(tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.last().map(|token| token.kind) == Some(TokenKind::Char(',')) {
        return Err(ParseError::syntax_exit(
            location,
            "type list cannot end with ','",
        ));
    }
    let chunks = split_top_level_commas(tokens);
    if chunks.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "type list cannot be empty",
        ));
    }
    chunks
        .into_iter()
        .map(|tokens| parse_type_name_tokens(tokens).map(Node::TypeName))
        .collect()
}

impl Parser {
    pub(super) fn parse_expression_fragment_tokens(
        &mut self,
        mut tokens: Vec<Token>,
    ) -> PResult<Node> {
        self.append_completion_marker(&mut tokens);
        parse_expression_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_b_expression_fragment_tokens(
        &mut self,
        mut tokens: Vec<Token>,
    ) -> PResult<Node> {
        self.append_completion_marker(&mut tokens);
        parse_b_expression_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_c_expression_fragment_tokens(
        &mut self,
        mut tokens: Vec<Token>,
    ) -> PResult<Node> {
        self.append_completion_marker(&mut tokens);
        parse_c_expression_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_json_value_fragment_tokens(
        &mut self,
        mut tokens: Vec<Token>,
    ) -> PResult<JsonValueExpr> {
        self.append_completion_marker(&mut tokens);
        parse_json_value_expr_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_select_fragment_tokens(&mut self, mut tokens: Vec<Token>) -> PResult<Node> {
        if !self.at_completion() {
            return parse_select_statement_tokens(tokens);
        }
        if tokens.is_empty() {
            self.record_completion_tokens(&[
                TokenKind::With,
                TokenKind::Select,
                TokenKind::Values,
                TokenKind::Table,
                TokenKind::Char('('),
            ]);
            return Err(self.error_here("completion point in SELECT fragment"));
        }
        self.append_completion_marker(&mut tokens);
        parse_select_statement_tokens_with_completion(tokens, self.completion.clone())
    }

    pub(super) fn parse_preparable_fragment_tokens(
        &mut self,
        mut tokens: Vec<Token>,
    ) -> PResult<Node> {
        if !self.at_completion() {
            return parse_preparable_statement_tokens(tokens);
        }
        if tokens.is_empty() {
            self.record_completion_tokens(&[
                TokenKind::With,
                TokenKind::Select,
                TokenKind::Values,
                TokenKind::Table,
                TokenKind::Char('('),
                TokenKind::Insert,
                TokenKind::Update,
                TokenKind::DeleteP,
                TokenKind::Merge,
            ]);
            return Err(self.error_here("completion point in preparable statement fragment"));
        }
        self.append_completion_marker(&mut tokens);
        parse_preparable_statement_tokens_with_completion(tokens, self.completion.clone())
    }
}
