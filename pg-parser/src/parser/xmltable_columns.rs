use super::*;

pub(super) fn xmltable_column_from_tokens_with_completion(
    tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<RangeTableFuncCol> {
    let location = tokens.first().map_or(0, |token| token.location());
    let colname = tokens
        .first()
        .and_then(|token| {
            token_name_in_categories(
                token,
                &[KeywordCategory::Unreserved, KeywordCategory::ColName],
            )
        })
        .ok_or_else(|| ParseError::syntax_exit(location, "expected an XMLTABLE column name"))?;
    if tokens.get(1).map(|token| token.kind) == Some(TokenKind::For)
        && tokens.get(2).map(|token| token.kind) == Some(TokenKind::Ordinality)
        && tokens.len() == 3
    {
        return Ok(RangeTableFuncCol {
            node_tag: NodeTag::RangeTableFuncCol,
            colname: Some(colname),
            for_ordinality: true,
            location: location as ParseLoc,
            ..RangeTableFuncCol::default()
        });
    }

    let option_start = (2..tokens.len())
        .find(|&index| {
            xmltable_option_starts_at(&tokens, index)
                && tokens_to_type_name(tokens[1..index].to_vec()).is_some()
        })
        .unwrap_or(tokens.len());
    let type_name = tokens_to_type_name(tokens[1..option_start].to_vec())
        .map(Box::new)
        .ok_or_else(|| ParseError::syntax_exit(location, "expected an XMLTABLE column type"))?;
    let mut column = RangeTableFuncCol {
        node_tag: NodeTag::RangeTableFuncCol,
        colname: Some(colname.clone()),
        type_name: Some(type_name),
        location: location as ParseLoc,
        ..RangeTableFuncCol::default()
    };
    let mut index = option_start;
    let mut nullability_seen = false;
    while index < tokens.len() {
        match tokens[index].kind {
            TokenKind::Not => {
                if tokens.get(index + 1).map(|token| token.kind) != Some(TokenKind::NullP) {
                    return Err(ParseError::ranged(tokens[index].range, "expected NOT NULL"));
                }
                if nullability_seen {
                    return Err(ParseError::ranged(
                        tokens[index].range,
                        format!(
                            "conflicting or redundant NULL / NOT NULL declarations for column {colname:?}"
                        ),
                    ));
                }
                column.is_not_null = true;
                nullability_seen = true;
                index += 2;
            }
            TokenKind::NullP => {
                if nullability_seen {
                    return Err(ParseError::ranged(
                        tokens[index].range,
                        format!(
                            "conflicting or redundant NULL / NOT NULL declarations for column {colname:?}"
                        ),
                    ));
                }
                column.is_not_null = false;
                nullability_seen = true;
                index += 1;
            }
            TokenKind::Path | TokenKind::Default => {
                let is_path = tokens[index].kind == TokenKind::Path;
                let option_location = tokens[index].location();
                index += 1;
                let start = index;
                index = xmltable_option_expression_end(&tokens, start);
                let expression = parse_b_expression_tokens_with_completion(
                    tokens[start..index].to_vec(),
                    completion.clone(),
                )?;
                if is_path {
                    if column.colexpr.is_some() {
                        return Err(ParseError::syntax_exit(
                            option_location,
                            "only one PATH value per column is allowed",
                        ));
                    }
                    column.colexpr = Some(Box::new(expression));
                } else {
                    if column.coldefexpr.is_some() {
                        return Err(ParseError::syntax_exit(
                            option_location,
                            "only one DEFAULT value is allowed",
                        ));
                    }
                    column.coldefexpr = Some(Box::new(expression));
                }
            }
            TokenKind::Ident | TokenKind::UIdent => {
                let option = token_name(&tokens[index]).unwrap_or_default();
                let message = if option == "__pg__is_not_null" {
                    format!("option name {option:?} cannot be used in XMLTABLE")
                } else {
                    format!("unrecognized column option {option:?}")
                };
                return Err(ParseError::ranged(tokens[index].range, message));
            }
            _ => {
                return Err(ParseError::ranged(
                    tokens[index].range,
                    "unsupported XMLTABLE column option",
                ));
            }
        }
    }
    Ok(column)
}

fn xmltable_option_starts_at(tokens: &[Token], index: usize) -> bool {
    match tokens[index].kind {
        TokenKind::Path | TokenKind::Default | TokenKind::NullP => true,
        TokenKind::Not => tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::NullP),
        TokenKind::Ident | TokenKind::UIdent => true,
        _ => false,
    }
}

fn xmltable_option_expression_end(tokens: &[Token], start: usize) -> usize {
    let mut depth = 0usize;
    for index in start..tokens.len() {
        match tokens[index].kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
        if index > start
            && depth == 0
            && xmltable_option_starts_at(tokens, index)
            && parse_b_expression_tokens(tokens[start..index].to_vec()).is_ok()
        {
            return index;
        }
    }
    tokens.len()
}
