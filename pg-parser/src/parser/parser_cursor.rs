use super::*;

impl Parser {
    pub(super) fn take_until_top_level(&mut self, stops: &[TokenKind]) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            let within_group = kind == TokenKind::GroupP
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Within);
            let collation_for = kind == TokenKind::For
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Collation);
            let distinct_from = kind == TokenKind::From
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Distinct);
            let is_not_predicate = kind == TokenKind::Not
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Is);
            if depth == 0
                && stops.contains(&kind)
                && !within_group
                && !collation_for
                && !distinct_from
                && !is_not_predicate
            {
                break;
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 && stops.contains(&kind) {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            out.push(self.advance().clone());
        }
        out
    }

    pub(super) fn at_statement_end(&self) -> bool {
        self.at(TokenKind::Char(';')) || self.at(TokenKind::Eof)
    }

    pub(super) fn expect_statement_end(&self) -> PResult<()> {
        if self.at_statement_end() {
            Ok(())
        } else {
            Err(self.error_here(format!(
                "unexpected token {:?} after statement",
                self.peek_kind()
            )))
        }
    }

    pub(super) fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    pub(super) fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek_kind())
    }

    pub(super) fn has_top_level_token_before(
        &self,
        needle: TokenKind,
        stops: &[TokenKind],
    ) -> bool {
        let mut depth = 0usize;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    depth = depth.saturating_sub(1);
                }
                kind if depth == 0 && kind == needle => return true,
                kind if depth == 0 && stops.contains(&kind) => return false,
                _ => {}
            }
        }
        false
    }

    pub(super) fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at(kind) {
            Ok(self.advance().clone())
        } else {
            Err(self.error_here(format!("expected {:?}, found {:?}", kind, self.peek_kind())))
        }
    }

    pub(super) fn advance(&mut self) -> &Token {
        if !self.at(TokenKind::Eof) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    pub(super) fn location(&self) -> usize {
        self.peek().location
    }

    pub(super) fn previous_location(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.location)
            .unwrap_or(self.location())
    }

    pub(super) fn error_here(&self, message: impl Into<std::string::String>) -> ParseError {
        ParseError::new(self.location(), message)
    }
}
