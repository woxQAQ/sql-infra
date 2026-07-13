use super::*;

fn function_parameter_mode(kind: TokenKind) -> Option<FunctionParameterMode> {
    match kind {
        TokenKind::InP => Some(FunctionParameterMode::In),
        TokenKind::OutP => Some(FunctionParameterMode::Out),
        TokenKind::Inout => Some(FunctionParameterMode::Inout),
        TokenKind::Variadic => Some(FunctionParameterMode::Variadic),
        _ => None,
    }
}

pub(super) fn token_starts_builtin_type(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Bigint
            | TokenKind::Bit
            | TokenKind::BooleanP
            | TokenKind::CharP
            | TokenKind::Character
            | TokenKind::Dec
            | TokenKind::DecimalP
            | TokenKind::DoubleP
            | TokenKind::FloatP
            | TokenKind::IntP
            | TokenKind::Integer
            | TokenKind::Interval
            | TokenKind::National
            | TokenKind::Nchar
            | TokenKind::Numeric
            | TokenKind::Real
            | TokenKind::Setof
            | TokenKind::Smallint
            | TokenKind::Time
            | TokenKind::Timestamp
            | TokenKind::Varchar
    )
}

pub(super) fn function_parameter_from_tokens(mut tokens: Vec<Token>) -> PResult<FunctionParameter> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::new(location, "expected a function parameter"));
    }

    let default_index = tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Default | TokenKind::Char('=')));
    let default_tokens = default_index.map(|index| tokens.split_off(index + 1));
    if default_index.is_some() {
        tokens.pop();
    }
    let defexpr = default_tokens
        .map(parse_expression_tokens)
        .transpose()?
        .map(Box::new);

    let mut mode = FunctionParameterMode::Default;
    let mut name = None;
    if let Some(parameter_mode) = tokens
        .first()
        .and_then(|token| function_parameter_mode(token.kind))
    {
        mode = parameter_mode;
        tokens.remove(0);
        if mode == FunctionParameterMode::In
            && tokens.first().map(|token| token.kind) == Some(TokenKind::OutP)
        {
            mode = FunctionParameterMode::Inout;
            tokens.remove(0);
        }
    } else if tokens.len() > 1
        && let Some(parameter_mode) = function_parameter_mode(tokens[1].kind)
        && let Some(parameter_name) = token_name_in_categories(
            &tokens[0],
            &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName],
        )
    {
        name = Some(parameter_name);
        mode = parameter_mode;
        tokens.drain(0..2);
        if mode == FunctionParameterMode::In
            && tokens.first().map(|token| token.kind) == Some(TokenKind::OutP)
        {
            mode = FunctionParameterMode::Inout;
            tokens.remove(0);
        }
    }

    if name.is_none()
        && tokens.len() > 1
        && let Some(parameter_name) = token_name_in_categories(
            &tokens[0],
            &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName],
        )
        && !token_starts_builtin_type(tokens[0].kind)
        && tokens[1].kind != TokenKind::Char('.')
        && tokens[1].kind != TokenKind::Char('[')
    {
        name = Some(parameter_name);
        tokens.remove(0);
    }

    let arg_type = parse_func_type_tokens(tokens)
        .map(Box::new)
        .map_err(|_| ParseError::new(location, "expected a function parameter type"))?;
    Ok(FunctionParameter {
        node_tag: NodeTag::FunctionParameter,
        name,
        arg_type: Some(arg_type),
        mode,
        defexpr,
        location: location as ParseLoc,
    })
}
