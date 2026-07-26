use super::*;

pub(super) fn tokens_to_type_name(tokens: Vec<Token>) -> Option<TypeName> {
    parse_type_name_tokens(tokens).ok()
}

pub(super) fn parse_const_type_name_tokens(tokens: Vec<Token>) -> PResult<TypeName> {
    let kinds = tokens.iter().map(|token| token.kind).collect::<Vec<_>>();
    let has_explicit_modifiers = find_top_level_token(&tokens, TokenKind::Char('(')).is_some();
    let mut type_name = parse_type_name_tokens(tokens)?;
    if !has_explicit_modifiers
        && matches!(
            kinds.as_slice(),
            [TokenKind::Bit]
                | [TokenKind::Character]
                | [TokenKind::CharP]
                | [TokenKind::National, TokenKind::Character]
                | [TokenKind::National, TokenKind::CharP]
                | [TokenKind::Nchar]
        )
    {
        type_name.typmods.clear();
    }
    Ok(type_name)
}

pub(super) fn parse_simple_type_name_tokens(tokens: Vec<Token>) -> PResult<TypeName> {
    let location = tokens.first().map_or(0, |token| token.location());
    let type_name = parse_type_name_tokens(tokens)?;
    if type_name.setof || !type_name.array_bounds.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "simple type name cannot use SETOF or array bounds",
        ));
    }
    Ok(type_name)
}

pub(super) fn parse_func_type_tokens(mut tokens: Vec<Token>) -> PResult<TypeName> {
    let location = tokens.first().map_or(0, |token| token.location());
    let setof = if tokens.first().map(|token| token.kind) == Some(TokenKind::Setof) {
        tokens.remove(0);
        true
    } else {
        false
    };
    let type_location = tokens.first().map_or(location, |token| token.location());
    if tokens.len() >= 3
        && tokens[tokens.len() - 2].kind == TokenKind::Char('%')
        && tokens[tokens.len() - 1].kind == TokenKind::TypeP
    {
        tokens.truncate(tokens.len() - 2);
        if tokens.is_empty() {
            return Err(ParseError::syntax_exit(
                location,
                "%TYPE requires a referenced name",
            ));
        }
        return Ok(TypeName {
            node_tag: NodeTag::TypeName,
            names: parse_qualified_type_names(&tokens)?,
            pct_type: true,
            setof,
            location: type_location as ParseLoc,
            ..TypeName::default()
        });
    }
    let mut type_name = parse_type_name_tokens(tokens)?;
    type_name.setof = setof;
    Ok(type_name)
}

pub(super) fn parse_type_name_tokens(mut tokens: Vec<Token>) -> PResult<TypeName> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "expected a type name"));
    }
    let setof = if tokens.first().map(|token| token.kind) == Some(TokenKind::Setof) {
        tokens.remove(0);
        true
    } else {
        false
    };
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "SETOF requires a type name",
        ));
    }
    let type_location = tokens[0].location();

    let mut array_bounds = Vec::new();
    while tokens.last().map(|token| token.kind) == Some(TokenKind::Char(']')) {
        let close = tokens.len() - 1;
        let open = (0..close)
            .rev()
            .find(|index| tokens[*index].kind == TokenKind::Char('['))
            .ok_or_else(|| ParseError::ranged(tokens[close].range, "unmatched ']' in type"))?;
        let bound = match &tokens[open + 1..close] {
            [] => -1,
            [
                Token {
                    kind: TokenKind::IConst,
                    value: Some(TokenValue::Integer(value)),
                    ..
                },
            ] if *value >= 0 => *value,
            _ => {
                return Err(ParseError::ranged(
                    tokens[open].range,
                    "array bound must be a non-negative integer",
                ));
            }
        };
        array_bounds.push(Node::Integer(Integer::new(bound)));
        tokens.truncate(open);
    }
    array_bounds.reverse();
    if tokens.last().map(|token| token.kind) == Some(TokenKind::Array) {
        tokens.pop();
        if array_bounds.is_empty() {
            array_bounds.push(Node::Integer(Integer::new(-1)));
        } else if array_bounds.len() != 1 {
            return Err(ParseError::syntax_exit(
                location,
                "SQL ARRAY syntax supports one dimension",
            ));
        }
    }
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "expected a type before array bounds",
        ));
    }

    let timezone = if tokens.len() >= 3
        && tokens[tokens.len() - 2].kind == TokenKind::Time
        && tokens[tokens.len() - 1].kind == TokenKind::Zone
        && matches!(
            tokens[tokens.len() - 3].kind,
            TokenKind::With | TokenKind::Without
        ) {
        let with_timezone = tokens[tokens.len() - 3].kind == TokenKind::With;
        tokens.truncate(tokens.len() - 3);
        Some(with_timezone)
    } else {
        None
    };

    let modifier_open = find_top_level_token(&tokens, TokenKind::Char('('));
    let (base_tokens, typmods) = if let Some(open) = modifier_open {
        let close = find_matching_close(&tokens, open)
            .ok_or_else(|| ParseError::ranged(tokens[open].range, "unterminated type modifier"))?;
        if close + 1 != tokens.len() {
            return Err(ParseError::ranged(
                tokens[close + 1].range,
                "unexpected token after type modifier",
            ));
        }
        let modifier_tokens = tokens[open + 1..close].to_vec();
        if modifier_tokens.is_empty() {
            return Err(ParseError::ranged(
                tokens[open].range,
                "type modifier list cannot be empty",
            ));
        }
        if modifier_tokens.last().map(|token| token.kind) == Some(TokenKind::Char(',')) {
            return Err(ParseError::syntax_exit(
                modifier_tokens
                    .last()
                    .map_or(location, |token| token.location()),
                "type modifier list cannot end with ','",
            ));
        }
        let mut typmods = Vec::new();
        for chunk in split_top_level_commas(modifier_tokens) {
            if chunk.is_empty() {
                return Err(ParseError::syntax_exit(
                    location,
                    "invalid type modifier list",
                ));
            }
            typmods.push(parse_expression_tokens(chunk)?);
        }
        (tokens[..open].to_vec(), typmods)
    } else {
        (tokens, Vec::new())
    };
    if base_tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "expected a type name"));
    }

    let kinds = base_tokens
        .iter()
        .map(|token| token.kind)
        .collect::<Vec<_>>();
    let mut default_typmods = Vec::new();
    let (names, typmods_allowed) = match kinds.as_slice() {
        [TokenKind::IntP] | [TokenKind::Integer] => (system_type_names("int4"), false),
        [TokenKind::Smallint] => (system_type_names("int2"), false),
        [TokenKind::Bigint] => (system_type_names("int8"), false),
        [TokenKind::Real] => (system_type_names("float4"), false),
        [TokenKind::DoubleP, TokenKind::Precision] => (system_type_names("float8"), false),
        [TokenKind::FloatP] => {
            let name = if typmods.is_empty() {
                "float8"
            } else if let [
                Node::AConst(AConst {
                    val: ValUnion::Integer(value),
                    ..
                }),
            ] = typmods.as_slice()
            {
                let precision = value.ival;
                if !(1..=53).contains(&precision) {
                    return Err(ParseError::syntax_exit(
                        location,
                        "FLOAT precision must be between 1 and 53",
                    ));
                }
                if precision <= 24 { "float4" } else { "float8" }
            } else {
                return Err(ParseError::syntax_exit(
                    location,
                    "FLOAT accepts one integer precision",
                ));
            };
            (system_type_names(name), true)
        }
        [TokenKind::DecimalP] | [TokenKind::Dec] | [TokenKind::Numeric] => {
            (system_type_names("numeric"), true)
        }
        [TokenKind::BooleanP] => (system_type_names("bool"), false),
        [TokenKind::Bit] | [TokenKind::Bit, TokenKind::Varying] => {
            let varying = kinds.len() == 2;
            if typmods.is_empty() && !varying {
                default_typmods.push(Node::AConst(AConst::integer(1, -1)));
            }
            (
                system_type_names(if varying { "varbit" } else { "bit" }),
                true,
            )
        }
        [TokenKind::Varchar]
        | [TokenKind::Character, TokenKind::Varying]
        | [TokenKind::CharP, TokenKind::Varying]
        | [
            TokenKind::National,
            TokenKind::Character,
            TokenKind::Varying,
        ]
        | [TokenKind::National, TokenKind::CharP, TokenKind::Varying]
        | [TokenKind::Nchar, TokenKind::Varying] => (system_type_names("varchar"), true),
        [TokenKind::Character]
        | [TokenKind::CharP]
        | [TokenKind::National, TokenKind::Character]
        | [TokenKind::National, TokenKind::CharP]
        | [TokenKind::Nchar] => {
            if typmods.is_empty() {
                default_typmods.push(Node::AConst(AConst::integer(1, -1)));
            }
            (system_type_names("bpchar"), true)
        }
        [TokenKind::Timestamp] => (
            system_type_names(if timezone == Some(true) {
                "timestamptz"
            } else {
                "timestamp"
            }),
            true,
        ),
        [TokenKind::Time] => (
            system_type_names(if timezone == Some(true) {
                "timetz"
            } else {
                "time"
            }),
            true,
        ),
        kinds if kinds.first() == Some(&TokenKind::Interval) => {
            if kinds.len() == 1 {
                if !typmods.is_empty() {
                    default_typmods.push(Node::AConst(AConst::integer(0x7fff, -1)));
                }
            } else {
                default_typmods.push(Node::AConst(AConst::integer(
                    parse_interval_mask(&kinds[1..], location)?,
                    base_tokens[1].location() as ParseLoc,
                )));
            }
            (system_type_names("interval"), true)
        }
        [TokenKind::Json] => (system_type_names("json"), false),
        _ => {
            if timezone.is_some() {
                return Err(ParseError::syntax_exit(
                    location,
                    "WITH/WITHOUT TIME ZONE requires TIME or TIMESTAMP",
                ));
            }
            (parse_qualified_type_names(&base_tokens)?, true)
        }
    };
    if !typmods_allowed && !typmods.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "this type does not accept type modifiers",
        ));
    }
    let typmods = if typmods.is_empty() {
        default_typmods
    } else if kinds.as_slice() == [TokenKind::FloatP] {
        Vec::new()
    } else if kinds.first() == Some(&TokenKind::Interval) {
        if typmods.len() != 1 {
            return Err(ParseError::syntax_exit(
                location,
                "INTERVAL accepts one precision modifier",
            ));
        }
        default_typmods.into_iter().chain(typmods).collect()
    } else {
        typmods
    };
    Ok(TypeName {
        node_tag: NodeTag::TypeName,
        names,
        setof,
        typmods,
        array_bounds,
        location: type_location as ParseLoc,
        ..TypeName::default()
    })
}

pub(super) fn system_type_names(name: &str) -> NodeList {
    vec![make_string_node("pg_catalog"), make_string_node(name)]
}

fn parse_interval_mask(kinds: &[TokenKind], location: usize) -> PResult<i32> {
    const MONTH: i32 = 1 << 1;
    const YEAR: i32 = 1 << 2;
    const DAY: i32 = 1 << 3;
    const HOUR: i32 = 1 << 10;
    const MINUTE: i32 = 1 << 11;
    const SECOND: i32 = 1 << 12;
    let mask = match kinds {
        [TokenKind::YearP] => YEAR,
        [TokenKind::MonthP] => MONTH,
        [TokenKind::DayP] => DAY,
        [TokenKind::HourP] => HOUR,
        [TokenKind::MinuteP] => MINUTE,
        [TokenKind::SecondP] => SECOND,
        [TokenKind::YearP, TokenKind::To, TokenKind::MonthP] => YEAR | MONTH,
        [TokenKind::DayP, TokenKind::To, TokenKind::HourP] => DAY | HOUR,
        [TokenKind::DayP, TokenKind::To, TokenKind::MinuteP] => DAY | HOUR | MINUTE,
        [TokenKind::DayP, TokenKind::To, TokenKind::SecondP] => DAY | HOUR | MINUTE | SECOND,
        [TokenKind::HourP, TokenKind::To, TokenKind::MinuteP] => HOUR | MINUTE,
        [TokenKind::HourP, TokenKind::To, TokenKind::SecondP] => HOUR | MINUTE | SECOND,
        [TokenKind::MinuteP, TokenKind::To, TokenKind::SecondP] => MINUTE | SECOND,
        _ => {
            return Err(ParseError::syntax_exit(
                location,
                "invalid INTERVAL field specification",
            ));
        }
    };
    Ok(mask)
}

pub(super) fn parse_qualified_type_names(tokens: &[Token]) -> PResult<NodeList> {
    let location = tokens.first().map_or(0, |token| token.location());
    let mut names = Vec::new();
    let mut expect_name = true;
    for token in tokens {
        if token.kind == TokenKind::Char('.') {
            if expect_name {
                return Err(ParseError::ranged(
                    token.range,
                    "invalid qualified type name",
                ));
            }
            expect_name = true;
            continue;
        }
        if !expect_name {
            return Err(ParseError::ranged(
                token.range,
                "type name components must be separated by '.'",
            ));
        }
        let name = token_name(token)
            .ok_or_else(|| ParseError::ranged(token.range, "invalid token in type name"))?;
        names.push(make_string_node(name));
        expect_name = false;
    }
    if names.is_empty() || expect_name {
        return Err(ParseError::syntax_exit(
            location,
            "invalid qualified type name",
        ));
    }
    Ok(names)
}

pub(super) fn parse_any_name_tokens(tokens: &[Token]) -> PResult<NodeList> {
    let location = tokens.first().map_or(0, |token| token.location());
    let mut names = Vec::new();
    let mut expect_name = true;
    for token in tokens {
        if token.kind == TokenKind::Char('.') {
            if expect_name {
                return Err(ParseError::ranged(
                    token.range,
                    "invalid qualified object name",
                ));
            }
            expect_name = true;
            continue;
        }
        if !expect_name {
            return Err(ParseError::ranged(
                token.range,
                "object name components must be separated by '.'",
            ));
        }
        let categories: &[KeywordCategory] = if names.is_empty() {
            &[KeywordCategory::Unreserved, KeywordCategory::ColName]
        } else {
            &[
                KeywordCategory::Unreserved,
                KeywordCategory::ColName,
                KeywordCategory::TypeFuncName,
                KeywordCategory::Reserved,
            ]
        };
        let name = token_name_in_categories(token, categories)
            .ok_or_else(|| ParseError::ranged(token.range, "invalid token in object name"))?;
        names.push(make_string_node(name));
        expect_name = false;
    }
    if names.is_empty() || expect_name {
        return Err(ParseError::syntax_exit(
            location,
            "invalid qualified object name",
        ));
    }
    Ok(names)
}

pub(super) fn tokens_to_name_nodes(tokens: &[Token]) -> NodeList {
    tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Char('.'))
        .filter_map(|token| token_name(token).map(make_string_node))
        .collect()
}
