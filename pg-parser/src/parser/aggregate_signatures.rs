//! Aggregate argument-signature parsing shared by create, alter, and drop forms.
//!
//! Both current and legacy PostgreSQL aggregate syntaxes normalize into raw
//! function-parameter nodes here.

use super::*;

pub(super) fn parse_old_aggregate_definition(tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.first().location_or(0);
    if tokens.last().has_kind(TokenKind::Char(',')) {
        return Err(ParseError::syntax_exit(
            location,
            "aggregate definition cannot end with ','",
        ));
    }
    let chunks = split_top_level_commas(tokens);
    if chunks.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "aggregate definition cannot be empty",
        ));
    }
    chunks
        .into_iter()
        .map(|tokens| {
            let item_location = tokens.first().location_or(location);
            let Some(name_token) = tokens.first() else {
                return Err(ParseError::syntax_exit(
                    item_location,
                    "old-style aggregate definition requires name = value",
                ));
            };
            if !matches!(name_token.kind, TokenKind::Ident | TokenKind::UIdent) {
                return Err(ParseError::ranged(
                    name_token.range,
                    "old-style aggregate option name must be an identifier",
                ));
            }
            if !tokens.get(1).has_kind(TokenKind::Char('=')) {
                return Err(ParseError::syntax_exit(
                    item_location,
                    "old-style aggregate definition requires name = value",
                ));
            }
            let name = token_name(name_token).ok_or_else(|| {
                ParseError::ranged(name_token.range, "invalid old-style aggregate option name")
            })?;
            let arg = parse_operator_def_arg(&name, tokens[2..].to_vec(), item_location)?;
            Ok(node!(DefElem {
                defname: Some(name),
                arg: Some(Box::new(arg)),
                location: item_location as ParseLoc,
                ..DefElem::default()
            }))
        })
        .collect()
}

pub(super) fn parse_aggregate_args(tokens: Vec<Token>) -> PResult<NodeList> {
    let location = tokens.first().location_or(0);
    if tokens.len() == 1 && tokens[0].kind == TokenKind::Char('*') {
        return Ok(vec![name_list_node(Vec::new()), node!(Integer::new(-1))]);
    }
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "aggregate argument list cannot be empty",
        ));
    }

    let mut depth = 0usize;
    let mut order_index = None;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            TokenKind::Order if depth == 0 && tokens.get(index + 1).has_kind(TokenKind::By) => {
                order_index = Some(index);
                break;
            }
            _ => {}
        }
    }
    let (direct_tokens, ordered_tokens) = if let Some(index) = order_index {
        (tokens[..index].to_vec(), tokens[index + 2..].to_vec())
    } else {
        (tokens, Vec::new())
    };
    let direct_chunks = if direct_tokens.is_empty() {
        Vec::new()
    } else {
        if direct_tokens.last().has_kind(TokenKind::Char(',')) {
            return Err(ParseError::syntax_exit(
                direct_tokens.last().location_or(location),
                "aggregate argument list cannot end with ','",
            ));
        }
        split_top_level_commas(direct_tokens)
    };
    if direct_chunks.iter().any(Vec::is_empty) {
        return Err(ParseError::syntax_exit(
            location,
            "invalid aggregate argument list",
        ));
    }
    let direct_count = if order_index.is_some() {
        direct_chunks.len() as i32
    } else {
        -1
    };
    let mut parameters = Vec::new();
    for chunk in direct_chunks {
        parameters.push(Node::FunctionParameter(parse_aggregate_parameter(chunk)?));
    }
    if order_index.is_some() {
        if ordered_tokens.last().has_kind(TokenKind::Char(',')) {
            return Err(ParseError::syntax_exit(
                ordered_tokens.last().location_or(location),
                "ordered aggregate argument list cannot end with ','",
            ));
        }
        let ordered_chunks = split_top_level_commas(ordered_tokens);
        if ordered_chunks.is_empty() || ordered_chunks.iter().any(Vec::is_empty) {
            return Err(ParseError::syntax_exit(
                location,
                "ORDER BY requires aggregate arguments",
            ));
        }
        let mut ordered = Vec::new();
        for chunk in ordered_chunks {
            ordered.push(parse_aggregate_parameter(chunk)?);
        }
        let direct_variadic = parameters.last().and_then(|parameter| match parameter {
            Node::FunctionParameter(parameter)
                if parameter.mode == FunctionParameterMode::Variadic =>
            {
                Some(parameter)
            }
            _ => None,
        });
        if let Some(direct_variadic) = direct_variadic {
            let compatible = matches!(ordered.as_slice(), [ordered_variadic]
                if ordered_variadic.mode == FunctionParameterMode::Variadic
                    && ordered_variadic
                        .arg_type
                        .as_deref()
                        .zip(direct_variadic.arg_type.as_deref())
                        .is_some_and(|(ordered, direct)| type_names_equal_ignoring_locations(ordered, direct)));
            if !compatible {
                return Err(ParseError::syntax_exit(
                    ordered
                        .first()
                        .map_or(location, |parameter| parameter.location as usize),
                    "an ordered-set aggregate with a VARIADIC direct argument requires one matching VARIADIC ordered argument",
                ));
            }
        } else {
            parameters.extend(ordered.into_iter().map(Node::FunctionParameter));
        }
    }
    Ok(vec![
        name_list_node(parameters),
        node!(Integer::new(direct_count)),
    ])
}

fn type_names_equal_ignoring_locations(left: &TypeName, right: &TypeName) -> bool {
    left.names == right.names
        && left.type_oid == right.type_oid
        && left.setof == right.setof
        && left.pct_type == right.pct_type
        && left.typemod == right.typemod
        && left.array_bounds == right.array_bounds
        && left.typmods.len() == right.typmods.len()
        && left
            .typmods
            .iter()
            .zip(&right.typmods)
            .all(|(left, right)| match (left, right) {
                (Node::AConst(left), Node::AConst(right)) => {
                    left.val == right.val && left.isnull == right.isnull
                }
                _ => left == right,
            })
}

fn parse_aggregate_parameter(tokens: Vec<Token>) -> PResult<FunctionParameter> {
    let location = tokens.first().location_or(0);
    let parameter = function_parameter_from_tokens(tokens)?;
    if !matches!(
        parameter.mode,
        FunctionParameterMode::Default
            | FunctionParameterMode::In
            | FunctionParameterMode::Variadic
    ) {
        return Err(ParseError::syntax_exit(
            location,
            "aggregates cannot have output arguments",
        ));
    }
    if parameter.defexpr.is_some() {
        return Err(ParseError::syntax_exit(
            location,
            "aggregate arguments cannot have default values",
        ));
    }
    Ok(parameter)
}
