use super::*;

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
    accepted.then(|| token_name(token)).flatten()
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
            Some(TokenValue::Integer(value)) => Some(Node::AConst(AConst::integer(
                value,
                token.location() as ParseLoc,
            ))),
            _ => None,
        },
        TokenKind::FConst => match &token.value {
            Some(TokenValue::String(value)) => Some(Node::AConst(AConst {
                node_tag: NodeTag::AConst,
                val: ValUnion::Float(Float::new(value.clone())),
                location: token.location() as ParseLoc,
                ..AConst::default()
            })),
            _ => None,
        },
        TokenKind::SConst => token_name(token)
            .map(|value| Node::AConst(AConst::string(value, token.location() as ParseLoc))),
        TokenKind::BConst | TokenKind::XConst => match &token.value {
            Some(TokenValue::String(value)) => Some(Node::AConst(AConst {
                node_tag: NodeTag::AConst,
                val: ValUnion::BitString(BitString::new(value.clone())),
                location: token.location() as ParseLoc,
                ..AConst::default()
            })),
            _ => None,
        },
        TokenKind::Param => match token.value {
            Some(TokenValue::Integer(number)) => Some(Node::ParamRef(ParamRef {
                node_tag: NodeTag::ParamRef,
                number,
                location: token.location() as ParseLoc,
            })),
            _ => None,
        },
        TokenKind::NullP => Some(Node::AConst(AConst::null(token.location() as ParseLoc))),
        TokenKind::TrueP => Some(Node::AConst(AConst {
            node_tag: NodeTag::AConst,
            val: ValUnion::Boolean(Boolean::new(true)),
            location: token.location() as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::FalseP => Some(Node::AConst(AConst {
            node_tag: NodeTag::AConst,
            val: ValUnion::Boolean(Boolean::new(false)),
            location: token.location() as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::Char('*') => Some(Node::AStar(AStar {
            node_tag: NodeTag::AStar,
        })),
        _ => token_name(token).map(|name| {
            Node::ColumnRef(ColumnRef {
                node_tag: NodeTag::ColumnRef,
                fields: vec![make_string_node(name)],
                location: token.location() as ParseLoc,
            })
        }),
    }
}

pub(super) fn tokens_to_def_elem(tokens: Vec<Token>, location: usize) -> PResult<DefElem> {
    let mut tokens = tokens
        .into_iter()
        .filter(|token| token.kind != TokenKind::Char(' '))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "expected an option"));
    }
    let first = token_name(&tokens[0])
        .ok_or_else(|| ParseError::syntax_exit(location, "expected an option name"))?;
    if tokens.get(1).map(|token| token.kind) == Some(TokenKind::Char('.')) {
        return Err(ParseError::ranged(
            tokens[1].range,
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
        return Err(ParseError::ranged(
            tokens[0].range,
            "definition option values require '='",
        ));
    }
    let arg = if tokens.is_empty() {
        if has_equals {
            return Err(ParseError::syntax_exit(
                location,
                "option requires a value after '='",
            ));
        }
        None
    } else {
        Some(Box::new(parse_operator_def_arg(
            &defname, tokens, location,
        )?))
    };
    Ok(DefElem {
        node_tag: NodeTag::DefElem,
        defnamespace: None,
        defname: Some(defname),
        arg,
        location: location as ParseLoc,
        ..DefElem::default()
    })
}

pub(super) fn parse_operator_def_arg(
    _option_name: &str,
    tokens: Vec<Token>,
    location: usize,
) -> PResult<Node> {
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "option requires a value"));
    }
    if tokens.len() == 1 {
        let token = &tokens[0];
        return match (&token.kind, &token.value) {
            (TokenKind::IConst, Some(TokenValue::Integer(value))) => {
                Ok(Node::Integer(Integer::new(*value)))
            }
            (TokenKind::FConst, Some(TokenValue::String(value))) => {
                Ok(Node::Float(Float::new(value.clone())))
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
                .ok_or_else(|| ParseError::ranged(token.range, "invalid option value")),
            _ => Err(ParseError::ranged(token.range, "invalid option value")),
        };
    }
    if tokens.first().map(|token| token.kind) == Some(TokenKind::Operator) {
        return parse_qualified_all_operator_tokens(tokens, location).map(name_list_node);
    }
    if tokens.iter().any(|token| is_operator_name_kind(token.kind)) {
        return Err(ParseError::syntax_exit(
            location,
            "qualified operator values require OPERATOR(schema.operator)",
        ));
    }
    parse_type_name_tokens(tokens).map(Node::TypeName)
}

pub(super) fn parse_operator_name_tokens(tokens: Vec<Token>, location: usize) -> PResult<NodeList> {
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "expected an operator name",
        ));
    }
    let mut elements = Vec::new();
    let mut expect_component = true;
    for token in tokens {
        if token.kind == TokenKind::Char('.') {
            if expect_component {
                return Err(ParseError::ranged(token.range, "invalid operator name"));
            }
            expect_component = true;
            continue;
        }
        if !expect_component {
            return Err(ParseError::ranged(
                token.range,
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
            ParseError::ranged(token.range, "invalid operator name")
        })?));
        expect_component = false;
    }
    if expect_component {
        return Err(ParseError::syntax_exit(
            location,
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

pub(super) fn tokens_to_text(tokens: &[Token]) -> std::string::String {
    tokens.iter().map(token_text).collect::<Vec<_>>().join(" ")
}
pub(super) fn extend_stops(stops: &[TokenKind], extra: TokenKind) -> Vec<TokenKind> {
    let mut out = stops.to_vec();
    if !out.contains(&extra) {
        out.push(extra);
    }
    out
}
