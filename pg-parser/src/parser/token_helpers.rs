//! Token interpretation, statement lookahead, and balanced token-list utilities.
//!
//! This module centralizes token-to-value projections, operator names, definition
//! elements, top-level splitting/search, and source-like token rendering.

use super::*;

pub(super) trait TokenOptionExt {
    fn has_kind(self, kind: TokenKind) -> bool;
    fn kind_or_eof(self) -> TokenKind;
    fn offset_or(self, default: usize) -> usize;
    fn offset_or_else(self, default: impl FnOnce() -> usize) -> usize;
    fn end_offset_or(self, default: usize) -> usize;
}

impl TokenOptionExt for Option<&Token> {
    fn has_kind(self, kind: TokenKind) -> bool {
        matches!(self, Some(token) if token.kind == kind)
    }

    fn kind_or_eof(self) -> TokenKind {
        match self {
            Some(token) => token.kind,
            None => TokenKind::Eof,
        }
    }

    fn offset_or(self, default: usize) -> usize {
        match self {
            Some(token) => token.offset(),
            None => default,
        }
    }

    fn offset_or_else(self, default: impl FnOnce() -> usize) -> usize {
        match self {
            Some(token) => token.offset(),
            None => default(),
        }
    }

    fn end_offset_or(self, default: usize) -> usize {
        match self {
            Some(token) => token.end_offset(),
            None => default,
        }
    }
}

pub(super) fn token_name(token: &Token) -> Option<std::string::String> {
    match &token.value {
        Some(TokenValue::String(value)) => Some(value.clone()),
        Some(TokenValue::Keyword(value)) => Some((*value).to_owned()),
        Some(TokenValue::Integer(value)) => Some(value.to_string()),
        None => match token.kind {
            TokenKind::Char('*') => Some("*".to_owned()),
            _ => None,
        },
    }
}

pub(super) fn token_name_in_categories(
    token: &Token,
    categories: &[KeywordCategory],
) -> Option<std::string::String> {
    let accepted = matches!(token.kind, TokenKind::Ident | TokenKind::UIdent)
        || match &token.value {
            Some(TokenValue::Keyword(word)) => {
                lookup_keyword(word).is_some_and(|keyword| categories.contains(&keyword.category))
            }
            _ => false,
        };
    if accepted { token_name(token) } else { None }
}

pub(super) fn token_text(token: &Token) -> std::string::String {
    token_name(token).unwrap_or_else(|| match token.kind {
        TokenKind::Char(ch) => ch.to_string(),
        TokenKind::LessEquals => "<=".to_owned(),
        TokenKind::GreaterEquals => ">=".to_owned(),
        TokenKind::NotEquals => "<>".to_owned(),
        TokenKind::RightArrow => "->".to_owned(),
        other => format!("{:?}", other).to_ascii_lowercase(),
    })
}

pub(super) fn is_operator_name_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Op
            | TokenKind::Char('+')
            | TokenKind::Char('-')
            | TokenKind::Char('*')
            | TokenKind::Char('/')
            | TokenKind::Char('%')
            | TokenKind::Char('^')
            | TokenKind::Char('<')
            | TokenKind::Char('>')
            | TokenKind::Char('=')
            | TokenKind::Char('|')
            | TokenKind::LessEquals
            | TokenKind::GreaterEquals
            | TokenKind::NotEquals
            | TokenKind::RightArrow
    )
}

pub(super) fn token_to_leaf(token: &Token) -> Option<Node> {
    match token.kind {
        TokenKind::IConst => match token.value {
            Some(TokenValue::Integer(value)) => {
                Some(node!(AConst::integer(value, token.offset() as ParseLoc,)))
            }
            _ => None,
        },
        TokenKind::FConst => match &token.value {
            Some(TokenValue::String(value)) => Some(node!(AConst {
                val: ValUnion::Float(Float::new(value.clone())),
                parse_loc: token.offset() as ParseLoc,
                ..AConst::default()
            })),
            _ => None,
        },
        TokenKind::SConst => {
            token_name(token).map(|value| node!(AConst::string(value, token.offset() as ParseLoc)))
        }
        TokenKind::BConst | TokenKind::XConst => match &token.value {
            Some(TokenValue::String(value)) => Some(node!(AConst {
                val: ValUnion::BitString(BitString::new(value.clone())),
                parse_loc: token.offset() as ParseLoc,
                ..AConst::default()
            })),
            _ => None,
        },
        TokenKind::Param => match token.value {
            Some(TokenValue::Integer(number)) => Some(node!(ParamRef {
                number,
                parse_loc: token.offset() as ParseLoc,
            })),
            _ => None,
        },
        TokenKind::NullP => Some(node!(AConst::null(token.offset() as ParseLoc))),
        TokenKind::TrueP => Some(node!(AConst {
            val: ValUnion::Boolean(Boolean::new(true)),
            parse_loc: token.offset() as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::FalseP => Some(node!(AConst {
            val: ValUnion::Boolean(Boolean::new(false)),
            parse_loc: token.offset() as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::Char('*') => Some(Node::AStar),
        _ => token_name(token).map(|name| {
            node!(ColumnRef {
                fields: vec![make_string_node(name)],
                parse_loc: token.offset() as ParseLoc,
            })
        }),
    }
}

pub(super) fn tokens_to_def_elem(tokens: Vec<Token>, offset: usize) -> PResult<DefElem> {
    let mut tokens = tokens
        .into_iter()
        .filter(|token| token.kind != TokenKind::Char(' '))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(offset, "expected an option"));
    }
    let first = token_name(&tokens[0])
        .ok_or_else(|| ParseError::syntax_exit(offset, "expected an option name"))?;
    if tokens.get(1).has_kind(TokenKind::Char('.')) {
        return Err(ParseError::at_loc(
            tokens[1].loc,
            "definition option names cannot be qualified",
        ));
    }
    let defname = first;
    tokens.remove(0);
    let has_equals = matches!(
        tokens.first().map(|token| token.kind),
        Some(TokenKind::Char('='))
    );
    if has_equals {
        tokens.remove(0);
    }
    if !tokens.is_empty() && !has_equals {
        return Err(ParseError::at_loc(
            tokens[0].loc,
            "definition option values require '='",
        ));
    }
    let arg = if tokens.is_empty() {
        if has_equals {
            return Err(ParseError::syntax_exit(
                offset,
                "option requires a value after '='",
            ));
        }
        None
    } else {
        Some(Box::new(parse_operator_def_arg(&defname, tokens, offset)?))
    };
    Ok(DefElem {
        defnamespace: None,
        defname: Some(defname),
        arg,
        parse_loc: offset as ParseLoc,
        ..DefElem::default()
    })
}

pub(super) fn parse_operator_def_arg(
    _option_name: &str,
    tokens: Vec<Token>,
    offset: usize,
) -> PResult<Node> {
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(offset, "option requires a value"));
    }
    if tokens.len() == 1 {
        let token = &tokens[0];
        return match (&token.kind, &token.value) {
            (TokenKind::IConst, Some(TokenValue::Integer(value))) => {
                Ok(node!(Integer::new(*value)))
            }
            (TokenKind::FConst, Some(TokenValue::String(value))) => {
                Ok(node!(Float::new(value.clone())))
            }
            (TokenKind::SConst, Some(TokenValue::String(value))) => {
                Ok(make_string_node(value.clone()))
            }
            (kind, _) if token_starts_builtin_type(*kind) => {
                parse_type_name_tokens(tokens).map(Node::TypeName)
            }
            (TokenKind::Ident | TokenKind::UIdent, _) => {
                parse_type_name_tokens(tokens).map(Node::TypeName)
            }
            (TokenKind::Op | TokenKind::Char(_), _) => {
                Ok(name_list_node(vec![make_string_node(token_text(token))]))
            }
            (_, Some(TokenValue::Keyword(_))) => token_name(token)
                .map(make_string_node)
                .ok_or_else(|| ParseError::at_loc(token.loc, "invalid option value")),
            _ => Err(ParseError::at_loc(token.loc, "invalid option value")),
        };
    }
    if tokens.first().has_kind(TokenKind::Operator) {
        return parse_qualified_all_operator_tokens(tokens, offset).map(name_list_node);
    }
    if tokens.iter().any(|token| is_operator_name_kind(token.kind)) {
        return Err(ParseError::syntax_exit(
            offset,
            "qualified operator values require OPERATOR(schema.operator)",
        ));
    }
    parse_type_name_tokens(tokens).map(Node::TypeName)
}

pub(super) fn parse_operator_name_tokens(tokens: Vec<Token>, offset: usize) -> PResult<NodeList> {
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(offset, "expected an operator name"));
    }
    let mut elements = Vec::new();
    let mut expect_component = true;
    for token in tokens {
        if token.kind == TokenKind::Char('.') {
            if expect_component {
                return Err(ParseError::at_loc(token.loc, "invalid operator name"));
            }
            expect_component = true;
            continue;
        }
        if !expect_component {
            return Err(ParseError::at_loc(
                token.loc,
                "operator name components must be separated by '.'",
            ));
        }
        let value = token_name(&token)
            .or_else(|| comparison_operator(token.kind).map(str::to_owned))
            .or_else(|| match token.kind {
                TokenKind::Op
                | TokenKind::Char('+')
                | TokenKind::Char('-')
                | TokenKind::Char('*')
                | TokenKind::Char('/')
                | TokenKind::Char('%')
                | TokenKind::Char('^')
                | TokenKind::Char('|') => Some(token_text(&token)),
                TokenKind::RightArrow => Some("->".to_owned()),
                _ => None,
            });
        elements.push(make_string_node(value.ok_or_else(|| {
            ParseError::at_loc(token.loc, "invalid operator name")
        })?));
        expect_component = false;
    }
    if expect_component {
        return Err(ParseError::syntax_exit(
            offset,
            "operator name cannot end with '.'",
        ));
    }
    Ok(elements)
}

pub(super) fn split_top_level_commas(tokens: Vec<Token>) -> Vec<Vec<Token>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            TokenKind::Char(',') if depth == 0 => {
                chunks.push(current);
                current = Vec::new();
                continue;
            }
            _ => {}
        }
        current.push(token);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub(super) fn find_top_level_token(tokens: &[Token], needle: TokenKind) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if depth == 0 && token.kind == needle {
            return Some(index);
        }
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

pub(super) fn find_matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::Char('(') => depth += 1,
            TokenKind::Char(')') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn extend_stops(stops: &[TokenKind], extra: TokenKind) -> Vec<TokenKind> {
    let mut extended_stops = stops.to_vec();
    if !extended_stops.contains(&extra) {
        extended_stops.push(extra);
    }
    extended_stops
}

impl Parser {
    pub(super) fn top_level_contains(&self, needle: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .any(|kind| kind == needle)
    }

    pub(super) fn top_level_kinds(&self) -> Vec<TokenKind> {
        let mut kinds = Vec::new();
        let mut depth = 0usize;
        let mut i = self.pos;
        while let Some(token) = self.tokens.get(i) {
            let kind = token.kind;
            if kind == TokenKind::Eof || (depth == 0 && kind == TokenKind::Char(';')) {
                break;
            }
            if depth == 0 {
                kinds.push(kind);
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        kinds
    }

    pub(super) fn consume_string_like(&mut self) -> Option<std::string::String> {
        match self.peek().value.clone() {
            Some(TokenValue::String(value)) => {
                self.advance();
                Some(value)
            }
            Some(TokenValue::Keyword(value)) => {
                self.advance();
                Some(value.to_owned())
            }
            Some(TokenValue::Integer(value)) => {
                self.advance();
                Some(value.to_string())
            }
            None => None,
        }
    }

    pub(super) fn consume_opt_boolean_or_string(&mut self) -> Option<std::string::String> {
        self.record_completion_tokens(&[
            TokenKind::SConst,
            TokenKind::TrueP,
            TokenKind::FalseP,
            TokenKind::On,
        ]);
        let token = self.peek().clone();
        let accepted = matches!(
            token.kind,
            TokenKind::SConst | TokenKind::TrueP | TokenKind::FalseP | TokenKind::On
        ) || token_name_in_categories(
            &token,
            &[
                KeywordCategory::Unreserved,
                KeywordCategory::ColName,
                KeywordCategory::TypeFuncName,
            ],
        )
        .is_some();
        if !accepted {
            return None;
        }
        let value = token_name(&token)?;
        self.advance();
        Some(value)
    }

    pub(super) fn consume_required_string(
        &mut self,
        message: &str,
    ) -> PResult<std::string::String> {
        self.record_completion_tokens(&[TokenKind::SConst]);
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here(message));
        }
        self.consume_string_like()
            .ok_or_else(|| self.error_here(message))
    }
}
