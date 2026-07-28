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

pub(super) fn function_parameter_from_tokens(tokens: Vec<Token>) -> PResult<FunctionParameter> {
    function_parameter_from_tokens_with_completion(tokens, None)
}

pub(super) fn function_parameter_from_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<FunctionParameter> {
    record_type_name_completion(&tokens, completion.as_ref());
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "expected a function parameter",
        ));
    }

    let default_index = tokens
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Default | TokenKind::Char('=')));
    if let Some(completion_index) = tokens
        .iter()
        .position(|token| token.kind == TokenKind::Completion)
        && default_index.is_none_or(|default_index| completion_index <= default_index)
        && let Some(collector) = &completion
    {
        let mut collector = collector.borrow_mut();
        collector.slot(completion::GrammarSlot::Type);
        let can_start_parameter_mode = completion_index == 0
            || (completion_index == 1
                && tokens.first().is_some_and(|token| {
                    function_parameter_mode(token.kind).is_none()
                        && !token_starts_builtin_type(token.kind)
                        && token_name_in_categories(
                            token,
                            &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName],
                        )
                        .is_some()
                }));
        if can_start_parameter_mode {
            collector.lookahead_tokens(&[
                TokenKind::InP,
                TokenKind::OutP,
                TokenKind::Inout,
                TokenKind::Variadic,
            ]);
        } else if tokens.first().map(|token| token.kind) == Some(TokenKind::InP)
            && completion_index == 1
        {
            collector.lookahead_tokens(&[TokenKind::OutP]);
        }
        if function_parameter_from_tokens(tokens[..completion_index].to_vec()).is_ok() {
            collector.lookahead_tokens(&[TokenKind::Default]);
        }
        collector.tokens(&[TokenKind::Char(')')]);
        if completion_index > 0 {
            collector.tokens(&[TokenKind::Char(',')]);
        }
    }
    if let (Some(completion_index), Some(default_index)) = (
        tokens
            .iter()
            .position(|token| token.kind == TokenKind::Completion),
        default_index,
    ) && completion_index > default_index + 1
        && parse_expression_tokens(tokens[default_index + 1..completion_index].to_vec()).is_ok()
        && let Some(collector) = &completion
    {
        collector
            .borrow_mut()
            .follow_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
    }
    let default_tokens = default_index.map(|index| tokens.split_off(index + 1));
    if default_index.is_some() {
        tokens.pop();
    }
    let defexpr = default_tokens
        .map(|tokens| parse_expression_tokens_with_completion(tokens, completion.clone()))
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
        .map_err(|_| ParseError::syntax_exit(location, "expected a function parameter type"))?;
    Ok(FunctionParameter {
        node_tag: NodeTag::FunctionParameter,
        name,
        arg_type: Some(arg_type),
        mode,
        defexpr,
        location: location as ParseLoc,
    })
}
