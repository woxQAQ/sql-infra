use pg_parser::{TextRange, TextSize, Token, TokenKind, TokenValue};

#[cfg(test)]
use pg_parser::{LexError, lex};

use crate::{
    CteDefinition, NamePart, QueryScope, RelationKind, ScopeSnapshot, UnsupportedRelation,
    VisibleRelation, prefix::name_part_from_token,
};

struct ScopeInput<'a> {
    source: &'a str,
    base: TextSize,
    tokens: &'a [Token],
    depths: &'a [usize],
}

#[cfg(test)]
pub(super) fn collect(
    source: &str,
    base: TextSize,
    point: TextSize,
) -> Result<ScopeSnapshot, LexError> {
    let tokens = lex(source)?;
    Ok(collect_tokens(source, base, point, &tokens))
}

pub(super) fn collect_tokens(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> ScopeSnapshot {
    let tokens = if tokens.last().map(|token| token.kind) == Some(TokenKind::Eof) {
        &tokens[..tokens.len() - 1]
    } else {
        tokens
    };
    let depths = token_depths(tokens);
    let point_depth = depth_at_point(tokens, point);
    let selects = enclosing_selects(tokens, &depths, point, point_depth);
    let cte_groups = collect_cte_groups(source, base, tokens, &depths);
    let ctes = visible_ctes_at_point(base, point, &cte_groups);
    let input = ScopeInput {
        source,
        base,
        tokens,
        depths: &depths,
    };

    let mut snapshot = ScopeSnapshot {
        ctes,
        ..ScopeSnapshot::default()
    };
    if let Some((local_index, local_depth)) = selects.last().copied() {
        snapshot.local = query_scope(&input, local_index, local_depth, &snapshot.ctes);
        apply_table_function_visibility(
            base,
            point,
            tokens,
            &depths,
            local_index,
            local_depth,
            &mut snapshot.local,
        );
        apply_join_condition_visibility(
            base,
            point,
            tokens,
            &depths,
            local_index,
            local_depth,
            &mut snapshot.local,
        );
        let mut child = (local_index, local_depth);
        for (index, depth) in selects[..selects.len() - 1].iter().rev().copied() {
            let mut outer = query_scope(&input, index, depth, &snapshot.ctes);
            apply_join_condition_visibility(base, point, tokens, &depths, index, depth, &mut outer);
            if let Some((open, lateral)) =
                derived_table_container(tokens, &depths, child.0, index, depth, point)
            {
                if lateral {
                    let boundary = add(base, tokens[open].range.start());
                    outer
                        .relations
                        .retain(|relation| relation.syntax_range.end() <= boundary);
                } else {
                    outer.relations.clear();
                }
            }
            if !outer.relations.is_empty() {
                snapshot.outer.push(outer);
            }
            child = (index, depth);
        }
    }

    let ctes = snapshot.ctes.clone();
    collect_dml_scope(&input, point, &ctes, !selects.is_empty(), &mut snapshot);
    snapshot
}

pub(super) fn incomplete_range(base: TextSize, tokens: &[Token]) -> Option<TextRange> {
    let mut opens = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::Char('(') => opens.push(token.range),
            TokenKind::Char(')') if opens.pop().is_none() => {
                return Some(absolute_range(base, token.range.start(), token.range.end()));
            }
            _ => {}
        }
    }
    opens
        .last()
        .map(|range| absolute_range(base, range.start(), range.end()))
}

fn derived_table_container(
    tokens: &[Token],
    depths: &[usize],
    child_select: usize,
    outer_select: usize,
    outer_depth: usize,
    point: TextSize,
) -> Option<(usize, bool)> {
    let open = (outer_select + 1..child_select).rev().find(|index| {
        tokens[*index].kind == TokenKind::Char('(')
            && depths[*index] == outer_depth
            && matching_close(tokens, depths, *index, tokens.len())
                .is_none_or(|close| tokens[close].range.start() >= point)
    })?;
    let branch_end = (outer_select + 1..tokens.len())
        .find(|index| {
            depths[*index] < outer_depth
                || (depths[*index] == outer_depth
                    && matches!(
                        tokens[*index].kind,
                        TokenKind::Union | TokenKind::Intersect | TokenKind::Except
                    ))
        })
        .unwrap_or(tokens.len());
    let from = (outer_select + 1..branch_end)
        .find(|index| depths[*index] == outer_depth && tokens[*index].kind == TokenKind::From)?;
    let from_end = (from + 1..branch_end)
        .find(|index| {
            depths[*index] == outer_depth
                && matches!(
                    tokens[*index].kind,
                    TokenKind::Where
                        | TokenKind::GroupP
                        | TokenKind::Having
                        | TokenKind::Window
                        | TokenKind::Order
                        | TokenKind::Limit
                        | TokenKind::Offset
                        | TokenKind::Fetch
                        | TokenKind::For
                )
        })
        .unwrap_or(branch_end);
    if !(from < open && open < from_end) {
        return None;
    }
    let preceding_clause = (from + 1..open).rev().find(|index| {
        depths[*index] == outer_depth
            && matches!(
                tokens[*index].kind,
                TokenKind::Char(',') | TokenKind::Join | TokenKind::On | TokenKind::Using
            )
    });
    if preceding_clause
        .is_some_and(|index| matches!(tokens[index].kind, TokenKind::On | TokenKind::Using))
    {
        return None;
    }
    let explicit_lateral = open > from && tokens[open - 1].kind == TokenKind::LateralP;
    let function_call = open > from
        && token_name(&tokens[open - 1]).is_some()
        && tokens[open - 1].kind != TokenKind::From;
    let rows_from = open >= from + 2
        && tokens[open - 2].kind == TokenKind::Rows
        && tokens[open - 1].kind == TokenKind::From;
    let lateral = explicit_lateral || function_call || rows_from;
    Some((open, lateral))
}

fn token_depths(tokens: &[Token]) -> Vec<usize> {
    let mut depth = 0usize;
    let mut depths = Vec::with_capacity(tokens.len());
    for token in tokens {
        if token.kind == TokenKind::Char(')') {
            depth = depth.saturating_sub(1);
        }
        depths.push(depth);
        if token.kind == TokenKind::Char('(') {
            depth += 1;
        }
    }
    depths
}

fn depth_at_point(tokens: &[Token], point: TextSize) -> usize {
    let mut depth = 0usize;
    for token in tokens {
        if token.range.start() >= point {
            break;
        }
        match token.kind {
            TokenKind::Char('(') => depth += 1,
            TokenKind::Char(')') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn enclosing_selects(
    tokens: &[Token],
    depths: &[usize],
    point: TextSize,
    point_depth: usize,
) -> Vec<(usize, usize)> {
    let mut by_depth = Vec::<(usize, usize)>::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.range.start() > point {
            break;
        }
        let depth = depths[index];
        match token.kind {
            // Depth alone cannot distinguish sibling parenthesized groups: a
            // close paren strictly before the point ends every group nested
            // inside it, so the SELECTs recorded there do not enclose the
            // point even when the point later returns to the same depth.
            TokenKind::Char(')') if token.range.start() < point => {
                by_depth.retain(|(_, candidate)| *candidate <= depth);
            }
            TokenKind::Select if depth <= point_depth => {
                if let Some(existing) = by_depth
                    .iter_mut()
                    .find(|(_, candidate)| *candidate == depth)
                {
                    *existing = (index, depth);
                } else {
                    by_depth.push((index, depth));
                }
            }
            _ => {}
        }
    }
    by_depth.sort_by_key(|(_, depth)| *depth);
    by_depth
}

fn query_scope(
    input: &ScopeInput<'_>,
    select_index: usize,
    depth: usize,
    ctes: &[CteDefinition],
) -> QueryScope {
    let tokens = input.tokens;
    let depths = input.depths;
    let branch_end = (select_index + 1..tokens.len())
        .find(|index| {
            depths[*index] < depth
                || (depths[*index] == depth
                    && matches!(
                        tokens[*index].kind,
                        TokenKind::Union | TokenKind::Intersect | TokenKind::Except
                    ))
        })
        .unwrap_or(tokens.len());
    let Some(from_index) = (select_index + 1..branch_end)
        .find(|index| depths[*index] == depth && tokens[*index].kind == TokenKind::From)
    else {
        return QueryScope::default();
    };
    let end = (from_index + 1..branch_end)
        .find(|index| {
            depths[*index] == depth
                && matches!(
                    tokens[*index].kind,
                    TokenKind::Where
                        | TokenKind::GroupP
                        | TokenKind::Having
                        | TokenKind::Window
                        | TokenKind::Order
                        | TokenKind::Limit
                        | TokenKind::Offset
                        | TokenKind::Fetch
                        | TokenKind::For
                        | TokenKind::Returning
                )
        })
        .unwrap_or(branch_end);
    QueryScope {
        relations: parse_from_relations(input, from_index + 1, end, depth, ctes),
    }
}

fn apply_table_function_visibility(
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
    depths: &[usize],
    select_index: usize,
    depth: usize,
    scope: &mut QueryScope,
) {
    let branch_end = (select_index + 1..tokens.len())
        .find(|index| {
            depths[*index] < depth
                || (depths[*index] == depth
                    && matches!(
                        tokens[*index].kind,
                        TokenKind::Union | TokenKind::Intersect | TokenKind::Except
                    ))
        })
        .unwrap_or(tokens.len());
    let Some(from) = (select_index + 1..branch_end)
        .find(|index| depths[*index] == depth && tokens[*index].kind == TokenKind::From)
    else {
        return;
    };
    let Some(open) = (from + 1..branch_end).find(|index| {
        tokens[*index].kind == TokenKind::Char('(')
            && depths[*index] == depth
            && tokens[*index].range.start() < point
            && matching_close(tokens, depths, *index, branch_end)
                .is_some_and(|close| tokens[close].range.end() >= point)
            && index.checked_sub(1).is_some_and(|previous| {
                depths[previous] == depth && token_name(&tokens[previous]).is_some()
            })
    }) else {
        return;
    };
    let item_start = (from + 1..open)
        .rev()
        .find(|index| {
            depths[*index] == depth
                && matches!(tokens[*index].kind, TokenKind::Char(',') | TokenKind::Join)
        })
        .map_or(from + 1, |delimiter| delimiter + 1);
    if (item_start..open).any(|index| {
        depths[index] == depth && matches!(tokens[index].kind, TokenKind::On | TokenKind::Using)
    }) {
        return;
    }
    let boundary = add(base, tokens[item_start].range.start());
    scope
        .relations
        .retain(|relation| relation.syntax_range.end() <= boundary);
}

fn apply_join_condition_visibility(
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
    depths: &[usize],
    select_index: usize,
    depth: usize,
    scope: &mut QueryScope,
) {
    let branch_end = (select_index + 1..tokens.len())
        .find(|index| {
            depths[*index] < depth
                || (depths[*index] == depth
                    && matches!(
                        tokens[*index].kind,
                        TokenKind::Union | TokenKind::Intersect | TokenKind::Except
                    ))
        })
        .unwrap_or(tokens.len());
    let Some(from) = (select_index + 1..branch_end)
        .find(|index| depths[*index] == depth && tokens[*index].kind == TokenKind::From)
    else {
        return;
    };
    let Some(on) = join_condition_boundary(tokens, depths, from + 1, branch_end, depth, point)
    else {
        return;
    };
    let boundary = add(base, on);
    scope
        .relations
        .retain(|relation| relation.syntax_range.end() <= boundary);
}

fn parse_from_relations(
    input: &ScopeInput<'_>,
    start: usize,
    end: usize,
    depth: usize,
    ctes: &[CteDefinition],
) -> Vec<VisibleRelation> {
    let ScopeInput {
        source,
        base,
        tokens,
        depths,
    } = input;
    let mut relations = Vec::new();
    let mut index = start;
    let mut expects_item = true;
    while index < end {
        if depths[index] != depth {
            index += 1;
            continue;
        }
        let kind = tokens[index].kind;
        if matches!(kind, TokenKind::Char(',') | TokenKind::Join) {
            expects_item = true;
            index += 1;
            continue;
        }
        if matches!(
            kind,
            TokenKind::Left
                | TokenKind::Right
                | TokenKind::Full
                | TokenKind::InnerP
                | TokenKind::Cross
                | TokenKind::Natural
                | TokenKind::OuterP
        ) {
            index += 1;
            continue;
        }
        if matches!(kind, TokenKind::On | TokenKind::Using) {
            expects_item = false;
            index += 1;
            continue;
        }
        if !expects_item {
            index += 1;
            continue;
        }
        if kind == TokenKind::Char('(')
            && let Some(close) = matching_close(tokens, depths, index, end)
            && !parenthesized_body_is_query(tokens, depths, index, close)
        {
            let (alias, _, next) = parse_alias(source, *base, tokens, close + 1, end);
            if alias.is_none() {
                relations.extend(parse_from_relations(
                    input,
                    index + 1,
                    close,
                    depth + 1,
                    ctes,
                ));
                index = next;
                expects_item = false;
                continue;
            }
        }
        if let Some((relation, next)) = parse_from_item(input, index, end, depth, ctes) {
            relations.push(relation);
            index = next;
            expects_item = false;
        } else {
            index += 1;
        }
    }
    relations
}

fn parse_from_item(
    input: &ScopeInput<'_>,
    mut index: usize,
    end: usize,
    depth: usize,
    ctes: &[CteDefinition],
) -> Option<(VisibleRelation, usize)> {
    let ScopeInput {
        source,
        base,
        tokens,
        depths,
    } = input;
    let base = *base;
    let start = index;
    let lateral = consume_kind(tokens, &mut index, end, TokenKind::LateralP);
    consume_kind(tokens, &mut index, end, TokenKind::Only);
    if index >= end {
        return None;
    }

    if tokens[index].kind == TokenKind::Char('(') {
        let open = index;
        let close = matching_close(tokens, depths, index, end)?;
        let query_body = parenthesized_body_is_query(tokens, depths, open, close);
        let values_body =
            first_parenthesized_body_kind(tokens, depths, open, close) == Some(TokenKind::Values);
        index = close + 1;
        let (alias, explicit_columns, next) = parse_alias(source, base, tokens, index, end);
        let syntax_end = next
            .checked_sub(1)
            .and_then(|last| tokens.get(last))
            .map_or(tokens[close].range.end(), |token| token.range.end());
        return Some((
            VisibleRelation {
                kind: if values_body {
                    RelationKind::Values
                } else if query_body {
                    RelationKind::Subquery
                } else {
                    RelationKind::JoinAlias
                },
                name: Vec::new(),
                alias,
                explicit_columns,
                syntax_range: absolute_range(base, tokens[start].range.start(), syntax_end),
                body_range: Some(absolute_range(
                    base,
                    tokens[open].range.end(),
                    tokens[close].range.start(),
                )),
                lateral,
                unsupported: (!query_body).then(|| UnsupportedRelation {
                    range: absolute_range(
                        base,
                        tokens[open].range.start(),
                        tokens[close].range.end(),
                    ),
                    reason: "parenthesized table expression is not classified".to_owned(),
                }),
            },
            next,
        ));
    }

    if tokens[index].kind == TokenKind::Rows
        && tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::From)
        && tokens.get(index + 2).map(|token| token.kind) == Some(TokenKind::Char('('))
    {
        let open = index + 2;
        let close = matching_close(tokens, depths, open, end)?;
        index = close + 1;
        consume_with_ordinality(tokens, &mut index, end);
        let (alias, mut explicit_columns, next) =
            parse_function_alias(source, base, tokens, index, end);
        if explicit_columns.is_empty() {
            explicit_columns = parse_rows_from_columns(source, base, tokens, depths, open, close);
        }
        let syntax_end = next
            .checked_sub(1)
            .and_then(|last| tokens.get(last))
            .map_or(tokens[close].range.end(), |token| token.range.end());
        return Some((
            VisibleRelation {
                kind: RelationKind::TableFunction,
                name: Vec::new(),
                alias,
                explicit_columns,
                syntax_range: absolute_range(base, tokens[start].range.start(), syntax_end),
                body_range: Some(absolute_range(
                    base,
                    tokens[open].range.end(),
                    tokens[close].range.start(),
                )),
                lateral,
                unsupported: None,
            },
            next,
        ));
    }

    let (name, after_name) = parse_qualified_name(source, base, tokens, index, end)?;
    index = after_name;
    let function =
        index < end && depths[index] == depth && tokens[index].kind == TokenKind::Char('(');
    let body_range = if function {
        let open = index;
        let close = matching_close(tokens, depths, open, end)?;
        index = close + 1;
        consume_with_ordinality(tokens, &mut index, end);
        Some(absolute_range(
            base,
            tokens[open].range.end(),
            tokens[close].range.start(),
        ))
    } else {
        None
    };
    let (alias, mut explicit_columns, next) = if function {
        parse_function_alias(source, base, tokens, index, end)
    } else {
        parse_alias(source, base, tokens, index, end)
    };
    let cte = name.last().and_then(|part| {
        ctes.iter()
            .find(|cte| cte.name.normalized == part.normalized)
    });
    if explicit_columns.is_empty()
        && let Some(cte) = cte
    {
        explicit_columns = cte.explicit_columns.clone();
    }
    let syntax_end = next
        .checked_sub(1)
        .and_then(|last| tokens.get(last))
        .map_or(tokens[after_name - 1].range.end(), |token| {
            token.range.end()
        });
    Some((
        VisibleRelation {
            kind: if function {
                RelationKind::TableFunction
            } else if cte.is_some() {
                RelationKind::Cte
            } else {
                RelationKind::Relation
            },
            name,
            alias,
            explicit_columns,
            syntax_range: absolute_range(base, tokens[start].range.start(), syntax_end),
            body_range,
            lateral,
            unsupported: None,
        },
        next,
    ))
}

fn parse_rows_from_columns(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    depths: &[usize],
    open: usize,
    close: usize,
) -> Vec<NamePart> {
    let item_depth = depths[open] + 1;
    let mut columns = Vec::new();
    let mut index = open + 1;
    while index + 1 < close {
        if depths[index] == item_depth
            && tokens[index].kind == TokenKind::As
            && depths[index + 1] == item_depth
            && tokens[index + 1].kind == TokenKind::Char('(')
            && let Some(definitions_close) = matching_close(tokens, depths, index + 1, close)
        {
            let (definitions, _) = parse_parenthesized_column_names(
                source,
                base,
                tokens,
                index + 1,
                definitions_close + 1,
            );
            columns.extend(definitions);
            index = definitions_close + 1;
            continue;
        }
        index += 1;
    }
    columns
}

fn first_parenthesized_body_kind(
    tokens: &[Token],
    depths: &[usize],
    mut open: usize,
    outer_close: usize,
) -> Option<TokenKind> {
    loop {
        let first = open + 1;
        if first >= outer_close {
            return None;
        }
        if tokens[first].kind != TokenKind::Char('(') {
            return Some(tokens[first].kind);
        }
        let close = matching_close(tokens, depths, first, outer_close)?;
        if close + 1 != outer_close {
            return Some(tokens[first].kind);
        }
        open = first;
    }
}

fn parenthesized_body_is_query(
    tokens: &[Token],
    depths: &[usize],
    open: usize,
    close: usize,
) -> bool {
    matches!(
        first_parenthesized_body_kind(tokens, depths, open, close),
        Some(TokenKind::Select | TokenKind::With | TokenKind::Table | TokenKind::Values)
    )
}

fn parse_alias(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    mut index: usize,
    end: usize,
) -> (Option<NamePart>, Vec<NamePart>, usize) {
    consume_kind(tokens, &mut index, end, TokenKind::As);
    let alias = if index < end && token_is_alias(&tokens[index]) {
        let alias = name_part(source, base, &tokens[index]);
        index += 1;
        alias
    } else {
        None
    };
    let mut columns = Vec::new();
    if alias.is_some() && index < end && tokens[index].kind == TokenKind::Char('(') {
        index += 1;
        while index < end && tokens[index].kind != TokenKind::Char(')') {
            if let Some(column) = name_part(source, base, &tokens[index]) {
                columns.push(column);
            }
            index += 1;
        }
        if index < end {
            index += 1;
        }
    }
    (alias, columns, index)
}

fn parse_function_alias(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    mut index: usize,
    end: usize,
) -> (Option<NamePart>, Vec<NamePart>, usize) {
    let has_as = consume_kind(tokens, &mut index, end, TokenKind::As);
    if has_as && index < end && tokens[index].kind == TokenKind::Char('(') {
        let (columns, next) = parse_parenthesized_column_names(source, base, tokens, index, end);
        return (None, columns, next);
    }
    let alias = if index < end && token_is_alias(&tokens[index]) {
        let alias = name_part(source, base, &tokens[index]);
        index += 1;
        alias
    } else {
        None
    };
    if alias.is_some() && index < end && tokens[index].kind == TokenKind::Char('(') {
        let (columns, next) = parse_parenthesized_column_names(source, base, tokens, index, end);
        (alias, columns, next)
    } else {
        (alias, Vec::new(), index)
    }
}

fn parse_parenthesized_column_names(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    open: usize,
    end: usize,
) -> (Vec<NamePart>, usize) {
    let mut columns = Vec::new();
    let mut index = open + 1;
    let mut depth = 0usize;
    let mut at_column_start = true;
    while index < end {
        match tokens[index].kind {
            TokenKind::Char(')') if depth == 0 => return (columns, index + 1),
            TokenKind::Char('(') => depth += 1,
            TokenKind::Char(')') => depth = depth.saturating_sub(1),
            TokenKind::Char(',') if depth == 0 => at_column_start = true,
            _ if depth == 0 && at_column_start => {
                if let Some(column) = name_part(source, base, &tokens[index]) {
                    columns.push(column);
                    at_column_start = false;
                }
            }
            _ => {}
        }
        index += 1;
    }
    (columns, end)
}

fn token_is_alias(token: &Token) -> bool {
    token_name(token).is_some()
        && !matches!(
            token.kind,
            TokenKind::On
                | TokenKind::Using
                | TokenKind::Join
                | TokenKind::Left
                | TokenKind::Right
                | TokenKind::Full
                | TokenKind::InnerP
                | TokenKind::Cross
                | TokenKind::Natural
                | TokenKind::Where
                | TokenKind::Set
                | TokenKind::From
                | TokenKind::Returning
                | TokenKind::When
                | TokenKind::GroupP
                | TokenKind::Having
                | TokenKind::Order
                | TokenKind::Limit
                | TokenKind::Offset
                | TokenKind::Fetch
                | TokenKind::For
                | TokenKind::Union
                | TokenKind::Intersect
                | TokenKind::Except
                | TokenKind::Tablesample
                | TokenKind::Repeatable
                | TokenKind::Ordinality
        )
}

fn consume_with_ordinality(tokens: &[Token], index: &mut usize, end: usize) {
    if *index + 1 < end
        && tokens[*index].kind == TokenKind::With
        && tokens[*index + 1].kind == TokenKind::Ordinality
    {
        *index += 2;
    }
}

fn parse_qualified_name(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    mut index: usize,
    end: usize,
) -> Option<(Vec<NamePart>, usize)> {
    let mut parts = Vec::new();
    parts.push(name_part(source, base, tokens.get(index)?)?);
    index += 1;
    while index + 1 < end && tokens[index].kind == TokenKind::Char('.') && parts.len() < 3 {
        let Some(part) = name_part(source, base, &tokens[index + 1]) else {
            break;
        };
        parts.push(part);
        index += 2;
    }
    Some((parts, index))
}

#[derive(Debug)]
struct CteGroup {
    depth: usize,
    start: TextSize,
    main_query_start: TextSize,
    end: TextSize,
    recursive: bool,
    ctes: Vec<CteDefinition>,
}

fn collect_cte_groups(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    depths: &[usize],
) -> Vec<CteGroup> {
    let mut groups = Vec::new();
    for with_index in 0..tokens.len() {
        if tokens[with_index].kind != TokenKind::With {
            continue;
        }
        if let Some(group) = parse_cte_group(source, base, tokens, depths, with_index) {
            groups.push(group);
        }
    }
    groups
}

fn parse_cte_group(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    depths: &[usize],
    with_index: usize,
) -> Option<CteGroup> {
    let depth = depths[with_index];
    let mut ctes = Vec::new();
    let mut index = with_index + 1;
    let recursive = consume_kind(tokens, &mut index, tokens.len(), TokenKind::Recursive);
    loop {
        if depths.get(index).copied() != Some(depth) {
            return None;
        }
        let name = tokens
            .get(index)
            .and_then(|token| name_part(source, base, token))?;
        let syntax_start = tokens[index].range.start();
        index += 1;
        let mut columns = Vec::new();
        if tokens.get(index).map(|token| token.kind) == Some(TokenKind::Char('('))
            && depths[index] == depth
        {
            let close = matching_close(tokens, depths, index, tokens.len())?;
            index += 1;
            while index < close {
                if depths[index] == depth + 1
                    && let Some(column) = name_part(source, base, &tokens[index])
                {
                    columns.push(column);
                }
                index += 1;
            }
            index = close + 1;
        }
        if tokens.get(index).map(|token| token.kind) != Some(TokenKind::As)
            || depths[index] != depth
        {
            return None;
        }
        index += 1;
        if matches!(
            tokens.get(index).map(|token| token.kind),
            Some(TokenKind::Not)
        ) {
            index += 1;
        }
        if matches!(
            tokens.get(index).map(|token| token.kind),
            Some(TokenKind::Materialized)
        ) {
            index += 1;
        }
        if tokens.get(index).map(|token| token.kind) != Some(TokenKind::Char('('))
            || depths[index] != depth
        {
            return None;
        }
        let open = index;
        let close = matching_close(tokens, depths, open, tokens.len());
        let body_end = close.map_or_else(
            || {
                tokens
                    .last()
                    .map_or(tokens[open].range.end(), |token| token.range.end())
            },
            |close| tokens[close].range.start(),
        );
        let syntax_end = close.map_or(body_end, |close| tokens[close].range.end());
        ctes.push(CteDefinition {
            name,
            explicit_columns: columns,
            syntax_range: absolute_range(base, syntax_start, syntax_end),
            body_range: absolute_range(base, tokens[open].range.end(), body_end),
        });
        let Some(close) = close else {
            index = tokens.len();
            break;
        };
        index = close + 1;
        if tokens.get(index).map(|token| token.kind) != Some(TokenKind::Char(','))
            || depths[index] != depth
        {
            break;
        }
        index += 1;
    }
    let main_query_start = tokens.get(index).map_or_else(
        || tokens[with_index].range.end(),
        |token| token.range.start(),
    );
    let end = (index..tokens.len())
        .find(|candidate| depths[*candidate] < depth)
        .map_or_else(
            || {
                tokens
                    .last()
                    .map_or(TextSize::ZERO, |token| token.range.end())
            },
            |candidate| tokens[candidate].range.start(),
        );
    Some(CteGroup {
        depth,
        start: add(base, tokens[with_index].range.start()),
        main_query_start: add(base, main_query_start),
        end: add(base, end),
        recursive,
        ctes,
    })
}

fn visible_ctes_at_point(
    base: TextSize,
    point: TextSize,
    groups: &[CteGroup],
) -> Vec<CteDefinition> {
    let point = add(base, point);
    let mut enclosing = groups
        .iter()
        .filter(|group| group.start <= point && point <= group.end)
        .collect::<Vec<_>>();
    enclosing.sort_by_key(|group| std::cmp::Reverse(group.depth));

    let mut visible = Vec::<CteDefinition>::new();
    for group in enclosing {
        let in_body = group
            .ctes
            .iter()
            .position(|cte| cte.body_range.start() <= point && point <= cte.body_range.end());
        let count = match in_body {
            Some(index) if !group.recursive => index,
            Some(_) => group.ctes.len(),
            None if point >= group.main_query_start || group.recursive => group.ctes.len(),
            None => group
                .ctes
                .iter()
                .take_while(|cte| cte.syntax_range.end() <= point)
                .count(),
        };
        for cte in group.ctes.iter().take(count) {
            if visible
                .iter()
                .any(|existing| existing.name.normalized == cte.name.normalized)
            {
                continue;
            }
            visible.push(cte.clone());
        }
    }
    visible
}

fn collect_dml_scope(
    input: &ScopeInput<'_>,
    point: TextSize,
    ctes: &[CteDefinition],
    inside_select: bool,
    snapshot: &mut ScopeSnapshot,
) {
    let ScopeInput {
        source,
        base,
        tokens,
        depths,
    } = input;
    let base = *base;
    // DML keywords also appear at depth 0 as trigger/rule events, privilege
    // names, and row-lock clauses; only statement heads that can wrap
    // top-level DML enter this path, and a `FOR [NO KEY] UPDATE` lock clause
    // never introduces the DML target.
    if !matches!(
        tokens.first().map(|token| token.kind),
        Some(
            TokenKind::With
                | TokenKind::Insert
                | TokenKind::Update
                | TokenKind::DeleteP
                | TokenKind::Merge
                | TokenKind::Explain
                | TokenKind::Prepare
        )
    ) {
        return;
    }
    let Some(first) = tokens
        .iter()
        .enumerate()
        .find(|(index, token)| {
            depths[*index] == 0
                && matches!(
                    token.kind,
                    TokenKind::Insert | TokenKind::Update | TokenKind::DeleteP | TokenKind::Merge
                )
                && !(token.kind == TokenKind::Update
                    && index.checked_sub(1).is_some_and(|previous| {
                        matches!(tokens[previous].kind, TokenKind::For | TokenKind::Key)
                    }))
        })
        .map(|(index, _)| index)
    else {
        return;
    };
    let target_start = match tokens[first].kind {
        TokenKind::Insert | TokenKind::Merge => first + 2,
        TokenKind::DeleteP => first + 2,
        TokenKind::Update => first + 1,
        _ => return,
    };
    if let Some((target, _)) = parse_dml_target(
        source,
        base,
        tokens,
        target_start,
        tokens.len(),
        tokens[first].kind,
    ) {
        snapshot.dml_target = Some(target);
    }
    if tokens[first].kind == TokenKind::Insert
        && point_is_in_insert_source(point, tokens, depths, first)
    {
        snapshot.dml_target = None;
    }
    if tokens[first].kind == TokenKind::Merge
        && let Some(using) = (first + 1..tokens.len()).find(|index| {
            depths[*index] == depths[first] && tokens[*index].kind == TokenKind::Using
        })
    {
        if let Some((source_relation, _)) =
            parse_from_item(input, using + 1, tokens.len(), depths[first], ctes)
        {
            snapshot.merge_source = Some(source_relation);
        }
        let on = (using + 1..tokens.len())
            .find(|index| depths[*index] == depths[first] && tokens[*index].kind == TokenKind::On);
        let source_end = on.map_or_else(
            || {
                tokens
                    .last()
                    .map_or(TextSize::ZERO, |token| token.range.end())
            },
            |index| tokens[index].range.start(),
        );
        if tokens[using].range.end() <= point && point <= source_end {
            snapshot.dml_target = None;
            snapshot.merge_source = None;
        }
    }

    let source_keyword = match tokens[first].kind {
        TokenKind::Update => Some(TokenKind::From),
        TokenKind::DeleteP => Some(TokenKind::Using),
        _ => None,
    };
    if let Some(source_keyword) = source_keyword
        && let Some(source_start) = (first + 1..tokens.len())
            .find(|index| depths[*index] == depths[first] && tokens[*index].kind == source_keyword)
    {
        let source_end = (source_start + 1..tokens.len())
            .find(|index| {
                depths[*index] == depths[first]
                    && matches!(tokens[*index].kind, TokenKind::Where | TokenKind::Returning)
            })
            .unwrap_or(tokens.len());
        let mut relations =
            parse_from_relations(input, source_start + 1, source_end, depths[first], ctes);
        if let Some(active) = relations.iter().position(|relation| {
            relation.body_range.is_some_and(|range| {
                range.start() <= add(base, point) && add(base, point) <= range.end()
            })
        }) {
            let active_start = relations[active].syntax_range.start();
            let implicitly_lateral = relations[active].kind == RelationKind::TableFunction;
            if relations[active].lateral || implicitly_lateral {
                relations.retain(|relation| relation.syntax_range.end() <= active_start);
                if !relations.is_empty() {
                    if inside_select {
                        snapshot.outer.push(QueryScope { relations });
                    } else {
                        snapshot.local.relations.extend(relations);
                    }
                }
            }
            snapshot.dml_target = None;
        } else {
            if let Some(on) = join_condition_boundary(
                tokens,
                depths,
                source_start + 1,
                source_end,
                depths[first],
                point,
            ) {
                let boundary = add(base, on);
                relations.retain(|relation| relation.syntax_range.end() <= boundary);
            }
            if inside_select {
                if !relations.is_empty() {
                    snapshot.outer.push(QueryScope { relations });
                }
            } else {
                snapshot.local.relations.extend(relations);
            }
        }
    }
}

fn point_is_in_insert_source(
    point: TextSize,
    tokens: &[Token],
    depths: &[usize],
    insert: usize,
) -> bool {
    let depth = depths[insert];
    let source = (insert + 1..tokens.len()).find(|index| {
        depths[*index] >= depth
            && matches!(
                tokens[*index].kind,
                TokenKind::Values
                    | TokenKind::Select
                    | TokenKind::With
                    | TokenKind::Table
                    | TokenKind::Execute
                    | TokenKind::Default
            )
    });
    let Some(source) = source else {
        return false;
    };
    let end = (source + 1..tokens.len())
        .find(|index| {
            depths[*index] == depth
                && (tokens[*index].kind == TokenKind::Returning
                    || (tokens[*index].kind == TokenKind::On
                        && tokens.get(*index + 1).map(|token| token.kind)
                            == Some(TokenKind::Conflict)))
        })
        .map_or_else(
            || {
                tokens
                    .last()
                    .map_or(TextSize::ZERO, |token| token.range.end())
            },
            |index| tokens[index].range.start(),
        );
    tokens[source].range.start() <= point && point <= end
}

fn join_condition_boundary(
    tokens: &[Token],
    depths: &[usize],
    start: usize,
    end: usize,
    depth: usize,
    point: TextSize,
) -> Option<TextSize> {
    (start..end)
        .rev()
        .find(|index| {
            if depths[*index] != depth
                || tokens[*index].kind != TokenKind::On
                || tokens[*index].range.start() > point
            {
                return false;
            }
            let condition_end = (*index + 1..end)
                .find(|candidate| {
                    depths[*candidate] == depth
                        && matches!(
                            tokens[*candidate].kind,
                            TokenKind::Char(',')
                                | TokenKind::Join
                                | TokenKind::Where
                                | TokenKind::GroupP
                                | TokenKind::Having
                                | TokenKind::Window
                                | TokenKind::Order
                                | TokenKind::Limit
                                | TokenKind::Offset
                                | TokenKind::Fetch
                                | TokenKind::For
                                | TokenKind::Returning
                        )
                })
                .map_or_else(
                    || {
                        tokens
                            .last()
                            .map_or(TextSize::ZERO, |token| token.range.end())
                    },
                    |candidate| tokens[candidate].range.start(),
                );
            point <= condition_end
        })
        .map(|index| tokens[index].range.start())
}

fn parse_dml_target(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    mut index: usize,
    end: usize,
    statement_kind: TokenKind,
) -> Option<(VisibleRelation, usize)> {
    let start = index;
    consume_kind(tokens, &mut index, end, TokenKind::Only);
    let parenthesized = consume_kind(tokens, &mut index, end, TokenKind::Char('('));
    let (name, after_name) = parse_qualified_name(source, base, tokens, index, end)?;
    index = after_name;
    if parenthesized {
        consume_kind(tokens, &mut index, end, TokenKind::Char(')'));
    }
    consume_kind(tokens, &mut index, end, TokenKind::Char('*'));
    let (alias, explicit_columns, next) = if statement_kind == TokenKind::Insert {
        if consume_kind(tokens, &mut index, end, TokenKind::As) {
            let alias = name_part(source, base, tokens.get(index)?)?;
            index += 1;
            if index < end && tokens[index].kind == TokenKind::Char('(') {
                let (columns, next) =
                    parse_parenthesized_column_names(source, base, tokens, index, end);
                (Some(alias), columns, next)
            } else {
                (Some(alias), Vec::new(), index)
            }
        } else {
            (None, Vec::new(), index)
        }
    } else {
        let (alias, columns, next) = parse_alias(source, base, tokens, index, end);
        (alias, columns, next)
    };
    let syntax_end = next
        .checked_sub(1)
        .and_then(|last| tokens.get(last))
        .map_or(tokens[after_name - 1].range.end(), |token| {
            token.range.end()
        });
    Some((
        VisibleRelation {
            kind: RelationKind::Relation,
            name,
            alias,
            explicit_columns,
            syntax_range: absolute_range(base, tokens[start].range.start(), syntax_end),
            body_range: None,
            lateral: false,
            unsupported: None,
        },
        next,
    ))
}

fn matching_close(tokens: &[Token], depths: &[usize], open: usize, end: usize) -> Option<usize> {
    let depth = depths[open];
    (open + 1..end)
        .find(|index| tokens[*index].kind == TokenKind::Char(')') && depths[*index] == depth)
}

fn consume_kind(tokens: &[Token], index: &mut usize, end: usize, kind: TokenKind) -> bool {
    if *index < end && tokens[*index].kind == kind {
        *index += 1;
        true
    } else {
        false
    }
}

fn name_part(source: &str, base: TextSize, token: &Token) -> Option<NamePart> {
    name_part_from_token(source, base, token)
}

fn token_name(token: &Token) -> Option<String> {
    match &token.value {
        Some(TokenValue::String(value)) => Some(value.clone()),
        Some(TokenValue::Keyword(value)) => Some((*value).to_owned()),
        _ => None,
    }
}

fn absolute_range(base: TextSize, start: TextSize, end: TextSize) -> TextRange {
    TextRange::new(add(base, start), add(base, end))
}

fn add(left: TextSize, right: TextSize) -> TextSize {
    TextSize::new(
        left.get()
            .checked_add(right.get())
            .expect("source range overflow"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_local_and_outer_query_relations() {
        let sql =
            "SELECT * FROM accounts a WHERE EXISTS (SELECT 1 FROM orders o WHERE o.id = a.id)";
        let point = TextSize::try_from(sql.find("o.id").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations[0].alias.as_ref().unwrap().text, "o");
        assert_eq!(
            scope.outer[0].relations[0].alias.as_ref().unwrap().text,
            "a"
        );
    }

    #[test]
    fn does_not_leak_cte_body_relations() {
        let sql = "WITH x(a) AS (SELECT secret FROM hidden) SELECT x.a FROM x";
        let point = TextSize::try_from(sql.find("x.a").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.ctes[0].name.normalized, "x");
        assert_eq!(scope.local.relations.len(), 1);
        assert_eq!(scope.local.relations[0].kind, RelationKind::Cte);
        assert!(
            scope.local.relations[0]
                .name
                .iter()
                .all(|part| part.normalized != "hidden")
        );
    }

    #[test]
    fn applies_lateral_visibility_to_derived_tables() {
        let non_lateral = "SELECT * FROM accounts a, (SELECT a.id) s";
        let point = TextSize::try_from(non_lateral.find("a.id").unwrap()).unwrap();
        let scope = collect(non_lateral, TextSize::ZERO, point).unwrap();
        assert!(scope.outer.is_empty());

        let lateral = "SELECT * FROM accounts a, LATERAL (SELECT a.id) s";
        let point = TextSize::try_from(lateral.find("a.id").unwrap()).unwrap();
        let scope = collect(lateral, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.outer[0].relations.len(), 1);
        assert_eq!(
            scope.outer[0].relations[0]
                .alias
                .as_ref()
                .unwrap()
                .normalized,
            "a"
        );
    }

    #[test]
    fn keeps_set_operation_branches_isolated() {
        let sql = "SELECT * FROM left_table UNION SELECT marker FROM right_table r";
        let point = TextSize::try_from(sql.find("marker").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations.len(), 1);
        assert_eq!(
            scope.local.relations[0].alias.as_ref().unwrap().normalized,
            "r"
        );
    }

    #[test]
    fn applies_cte_visibility_order_and_explicit_columns() {
        let sql =
            "WITH first(a) AS (SELECT 1), second AS (SELECT a FROM first) SELECT * FROM first";
        let first_body = TextSize::try_from(sql.find("SELECT 1").unwrap() + 7).unwrap();
        let scope = collect(sql, TextSize::ZERO, first_body).unwrap();
        assert!(scope.ctes.is_empty());

        let second_body = TextSize::try_from(sql.find("SELECT a").unwrap() + 7).unwrap();
        let scope = collect(sql, TextSize::ZERO, second_body).unwrap();
        assert_eq!(
            scope
                .ctes
                .iter()
                .map(|cte| cte.name.normalized.as_str())
                .collect::<Vec<_>>(),
            ["first"]
        );

        let outer = TextSize::try_from(sql.rfind("SELECT").unwrap() + 7).unwrap();
        let scope = collect(sql, TextSize::ZERO, outer).unwrap();
        assert_eq!(scope.local.relations[0].kind, RelationKind::Cte);
        assert_eq!(
            scope.local.relations[0]
                .explicit_columns
                .iter()
                .map(|column| column.normalized.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn table_functions_see_only_preceding_from_items() {
        let sql = "SELECT * FROM accounts a, generate_series(a.id, 10) g, later_relation later";
        let point = TextSize::try_from(sql.find("a.id").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations.len(), 1);
        assert_eq!(
            scope.local.relations[0].alias.as_ref().unwrap().normalized,
            "a"
        );

        let sql = "SELECT * FROM accounts a JOIN users u ON coalesce(a.id, u.id) > 0";
        let point = TextSize::try_from(sql.find("a.id").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations.len(), 2);
    }

    #[test]
    fn closed_sibling_groups_do_not_hijack_the_local_scope() {
        let sql = "SELECT * FROM (SELECT 1) AS s, generate_series(x, 10)";
        let point = TextSize::try_from(sql.find("x, 10").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert!(scope.outer.is_empty());
        assert_eq!(scope.local.relations.len(), 1);
        assert_eq!(scope.local.relations[0].kind, RelationKind::Subquery);
        assert_eq!(scope.local.relations[0].alias.as_ref().unwrap().text, "s");

        let sql = "UPDATE t SET a = (SELECT 1 FROM hidden) + lower(x)";
        let point = TextSize::try_from(sql.rfind("x)").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert!(scope.local.relations.is_empty());
        assert_eq!(scope.dml_target.unwrap().name[0].normalized, "t");
    }

    #[test]
    fn classifies_rows_from_as_a_table_function() {
        let sql = "SELECT r.a FROM ROWS FROM (f(), g()) AS r(a)";
        let point = TextSize::try_from(sql.find("r.a").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations.len(), 1);
        assert_eq!(scope.local.relations[0].kind, RelationKind::TableFunction);
        assert_eq!(
            scope.local.relations[0].alias.as_ref().unwrap().normalized,
            "r"
        );
        assert_eq!(scope.local.relations[0].explicit_columns[0].normalized, "a");
    }

    #[test]
    fn classifies_values_and_marks_unclassified_table_expressions() {
        let sql = "SELECT v.a FROM (VALUES (1)) AS v(a)";
        let point = TextSize::try_from(sql.find("v.a").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations[0].kind, RelationKind::Values);
        assert_eq!(scope.local.relations[0].explicit_columns[0].normalized, "a");

        let sql = "SELECT j.a FROM (left_table l JOIN right_table r ON true) j";
        let point = TextSize::try_from(sql.find("j.a").unwrap() + 2).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations[0].kind, RelationKind::JoinAlias);
        assert!(scope.local.relations[0].unsupported.is_some());
        assert!(scope.local.relations[0].explicit_columns.is_empty());
    }

    #[test]
    fn dml_targets_are_relations_and_ignore_nested_cte_dml() {
        let sql = "INSERT INTO target_table (id) VALUES (1)";
        let point = TextSize::try_from(sql.find(" VALUES").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        let target = scope.dml_target.unwrap();
        assert_eq!(target.kind, RelationKind::Relation);
        assert_eq!(target.name[0].normalized, "target_table");

        let sql =
            "WITH changed AS (UPDATE hidden SET id = 1 RETURNING *) UPDATE visible SET id = 2";
        let scope = collect(sql, TextSize::ZERO, TextSize::try_from(sql.len()).unwrap()).unwrap();
        assert_eq!(
            scope.dml_target.unwrap().name[0].normalized,
            "visible",
            "the top-level DML target must win over a CTE body target"
        );

        let sql = "INSERT INTO target OVERRIDING SYSTEM VALUE VALUES (1)";
        let point = TextSize::try_from(sql.find(" OVERRIDING").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert!(scope.dml_target.as_ref().unwrap().alias.is_none());

        let sql = "INSERT INTO target AS dst VALUES (1)";
        let point = TextSize::try_from(sql.find(" VALUES").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(
            scope
                .dml_target
                .as_ref()
                .unwrap()
                .alias
                .as_ref()
                .unwrap()
                .normalized,
            "dst"
        );
    }

    #[test]
    fn dml_event_and_lock_keywords_do_not_fabricate_targets() {
        for sql in [
            "CREATE RULE r AS ON UPDATE TO t WHERE value > 0 DO INSTEAD NOTHING",
            "CREATE TRIGGER trg BEFORE UPDATE ON t FOR EACH ROW WHEN (true) EXECUTE FUNCTION f()",
            "GRANT UPDATE ON t TO role_name",
            "REVOKE INSERT ON t FROM role_name",
            "CREATE POLICY p ON t FOR UPDATE USING (true)",
            "SELECT * FROM t FOR UPDATE OF t",
            "EXPLAIN SELECT * FROM t FOR UPDATE OF t",
            "PREPARE q AS SELECT * FROM t FOR NO KEY UPDATE",
            "WITH x AS (SELECT 1) SELECT * FROM x FOR UPDATE OF x",
        ] {
            let point = TextSize::try_from(sql.len()).unwrap();
            let scope = collect(sql, TextSize::ZERO, point).unwrap();
            assert!(
                scope.dml_target.is_none(),
                "{sql:?}: {:?}",
                scope.dml_target
            );
            assert!(scope.merge_source.is_none(), "{sql:?}");
        }
    }

    #[test]
    fn wrapped_dml_statements_keep_their_target() {
        for sql in [
            "EXPLAIN UPDATE t SET a = 1",
            "EXPLAIN (ANALYZE, VERBOSE) UPDATE t SET a = 1",
            "PREPARE q AS UPDATE t SET a = 1",
            "WITH x AS (SELECT 1) UPDATE t SET a = 1",
        ] {
            let point = TextSize::try_from(sql.len()).unwrap();
            let scope = collect(sql, TextSize::ZERO, point).unwrap();
            assert_eq!(
                scope
                    .dml_target
                    .as_ref()
                    .map(|target| target.name[0].normalized.as_str()),
                Some("t"),
                "{sql:?}"
            );
        }

        // The INSERT target stays visible in its column list; the point must
        // sit before the source, which never sees the target.
        let sql = "PREPARE q (int) AS INSERT INTO t (a) VALUES ($1)";
        let point = TextSize::try_from(sql.find("a)").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(
            scope
                .dml_target
                .as_ref()
                .map(|target| target.name[0].normalized.as_str()),
            Some("t")
        );
    }
}
