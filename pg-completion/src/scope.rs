use pg_parser::{TextRange, TextSize, Token, TokenKind, TokenValue};

#[cfg(test)]
use pg_parser::{LexError, lex};

use crate::{
    CteDefinition, NamePart, QueryScope, RelationKind, ScopeSnapshot, UnsupportedRelation,
    VisibleRelation, prefix::name_part_from_token,
};

#[derive(Clone, Copy)]
struct ScopeInput<'a> {
    source: &'a str,
    base: TextSize,
    point: TextSize,
    tokens: &'a [Token],
    depths: &'a [usize],
}

impl ScopeInput<'_> {
    fn absolute_point(self) -> TextSize {
        add(self.base, self.point)
    }
}

#[derive(Clone, Copy)]
struct SelectLocation {
    index: usize,
    depth: usize,
}

#[derive(Clone, Copy)]
enum DmlRelationPlacement {
    Local,
    Outer,
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
    let input = ScopeInput {
        source,
        base,
        point,
        tokens,
        depths: &depths,
    };
    let point_depth = depth_at_point(input.tokens, input.point);
    let mut selects = enclosing_selects(&input, point_depth);
    remove_completed_insert_source_selects(&input, &mut selects);
    let cte_groups = collect_cte_groups(&input);
    let ctes = visible_ctes_at_point(input.absolute_point(), &cte_groups);

    let mut snapshot = ScopeSnapshot::default();
    if let Some((&local_select, outer_selects)) = selects.split_last() {
        snapshot.local = query_scope(&input, local_select, &ctes);

        // Table functions are implicitly LATERAL: while completing inside the
        // call, only relations introduced before this FROM item are visible.
        apply_table_function_visibility(&input, local_select, &mut snapshot.local);

        // In the local query, only relations available at the cursor position
        // may be referenced from JOIN ON/USING conditions.
        apply_join_condition_visibility(&input, local_select, point_depth, &mut snapshot.local);
        let mut inner_select = local_select;
        for &outer_select in outer_selects.iter().rev() {
            let mut outer = query_scope(&input, outer_select, &ctes);

            // The enclosing SELECT may itself be inside a JOIN condition.
            // Prevent relations introduced by later joins from leaking in.
            apply_join_condition_visibility(&input, outer_select, outer_select.depth, &mut outer);

            // Check whether the inner SELECT is inside a derived table belonging
            // to this outer SELECT's FROM list.
            if let Some((open_paren, is_lateral)) =
                derived_table_container(&input, inner_select, outer_select)
            {
                if is_lateral {
                    let boundary = add(input.base, input.tokens[open_paren].range.start());
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
            inner_select = outer_select;
        }
    }

    let dml_relation_placement = if selects.is_empty() {
        DmlRelationPlacement::Local
    } else {
        DmlRelationPlacement::Outer
    };
    collect_dml_scope(&input, &ctes, dml_relation_placement, &mut snapshot);
    snapshot.ctes = ctes;
    snapshot
}

pub(super) fn incomplete_range(base: TextSize, tokens: &[Token]) -> Option<TextRange> {
    let mut unmatched_opens = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::Char('(') => unmatched_opens.push(token.range),
            TokenKind::Char(')') if unmatched_opens.pop().is_none() => {
                return Some(absolute_range(base, token.range.start(), token.range.end()));
            }
            _ => {}
        }
    }
    unmatched_opens
        .last()
        .map(|range| absolute_range(base, range.start(), range.end()))
}

/// Clause keywords that end a `FROM` list when they appear at branch depth.
const FROM_LIST_END: &[TokenKind] = &[
    TokenKind::Where,
    TokenKind::GroupP,
    TokenKind::Having,
    TokenKind::Window,
    TokenKind::Order,
    TokenKind::Limit,
    TokenKind::Offset,
    TokenKind::Fetch,
    TokenKind::For,
    TokenKind::Returning,
];

/// The `FROM` list of one SELECT set-operation branch.
struct FromSegment {
    /// Index of the `FROM` token.
    from_keyword: usize,
    /// First same-depth [`FROM_LIST_END`] keyword after the list, else the
    /// branch end.
    list_end: usize,
    /// First same-depth set-operation keyword or depth drop after the SELECT.
    branch_end: usize,
}

fn from_segment(tokens: &[Token], depths: &[usize], select: SelectLocation) -> Option<FromSegment> {
    let branch_end = (select.index + 1..tokens.len())
        .find(|index| {
            depths[*index] < select.depth
                || (depths[*index] == select.depth
                    && matches!(
                        tokens[*index].kind,
                        TokenKind::Union | TokenKind::Intersect | TokenKind::Except
                    ))
        })
        .unwrap_or(tokens.len());
    let from_keyword = (select.index + 1..branch_end)
        .find(|index| depths[*index] == select.depth && tokens[*index].kind == TokenKind::From)?;
    let list_end = (from_keyword + 1..branch_end)
        .find(|index| {
            depths[*index] == select.depth && FROM_LIST_END.contains(&tokens[*index].kind)
        })
        .unwrap_or(branch_end);
    Some(FromSegment {
        from_keyword,
        list_end,
        branch_end,
    })
}

fn derived_table_container(
    input: &ScopeInput<'_>,
    child_select: SelectLocation,
    outer_select: SelectLocation,
) -> Option<(usize, bool)> {
    let open = (outer_select.index + 1..child_select.index)
        .rev()
        .find(|index| {
            input.tokens[*index].kind == TokenKind::Char('(')
                && input.depths[*index] == outer_select.depth
                && matching_close(input.tokens, input.depths, *index, input.tokens.len())
                    .is_none_or(|close| input.tokens[close].range.start() >= input.point)
        })?;
    let segment = from_segment(input.tokens, input.depths, outer_select)?;
    let from_keyword = segment.from_keyword;
    if !(from_keyword < open && open < segment.list_end) {
        return None;
    }
    let preceding_clause = (from_keyword + 1..open).rev().find(|index| {
        input.depths[*index] == outer_select.depth
            && matches!(
                input.tokens[*index].kind,
                TokenKind::Char(',') | TokenKind::Join | TokenKind::On | TokenKind::Using
            )
    });
    if preceding_clause
        .is_some_and(|index| matches!(input.tokens[index].kind, TokenKind::On | TokenKind::Using))
    {
        return None;
    }
    let explicit_lateral =
        open > from_keyword && input.tokens[open - 1].kind == TokenKind::LateralP;
    let function_call = open > from_keyword
        && token_can_be_name(&input.tokens[open - 1])
        && input.tokens[open - 1].kind != TokenKind::From;
    let rows_from = open >= from_keyword + 2
        && input.tokens[open - 2].kind == TokenKind::Rows
        && input.tokens[open - 1].kind == TokenKind::From;
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

fn enclosing_selects(input: &ScopeInput<'_>, point_depth: usize) -> Vec<SelectLocation> {
    let tokens = input.tokens;
    let depths = input.depths;
    let point = input.point;
    let mut selects_by_depth = Vec::<SelectLocation>::new();
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
                selects_by_depth.retain(|candidate| candidate.depth <= depth);
            }
            TokenKind::Select if depth <= point_depth => {
                if let Some(existing) = selects_by_depth
                    .iter_mut()
                    .find(|candidate| candidate.depth == depth)
                {
                    *existing = SelectLocation { index, depth };
                } else {
                    selects_by_depth.push(SelectLocation { index, depth });
                }
            }
            _ => {}
        }
    }
    if selects_by_depth.is_empty()
        && let Some(select) = wrapped_query_select_before_suffix(input, point_depth)
    {
        selects_by_depth.push(SelectLocation {
            index: select,
            depth: depths[select],
        });
    }
    selects_by_depth
        .retain(|select| !point_is_in_set_operation_suffix(input, *select, point_depth));
    selects_by_depth.sort_by_key(|select| select.depth);
    selects_by_depth
}

fn remove_completed_insert_source_selects(
    input: &ScopeInput<'_>,
    selects: &mut Vec<SelectLocation>,
) {
    for (insert, token) in input.tokens.iter().enumerate() {
        if token.kind != TokenKind::Insert
            || token.range.start() > input.point
            || input.tokens.get(insert + 1).map(|token| token.kind) != Some(TokenKind::Into)
        {
            continue;
        }
        let statement_depth = input.depths[insert];
        let source_end_keyword = (insert + 2..input.tokens.len()).find(|index| {
            input.depths[*index] == statement_depth
                && (input.tokens[*index].kind == TokenKind::Returning
                    || (input.tokens[*index].kind == TokenKind::On
                        && input.tokens.get(*index + 1).map(|token| token.kind)
                            == Some(TokenKind::Conflict)))
        });
        let Some(source_end_keyword) = source_end_keyword
            .filter(|boundary| input.tokens[*boundary].range.start() < input.point)
        else {
            continue;
        };
        selects.retain(|select| {
            !(select.index > insert
                && select.index < source_end_keyword
                && select.depth >= statement_depth)
        });
    }
}

fn point_is_in_set_operation_suffix(
    input: &ScopeInput<'_>,
    select: SelectLocation,
    point_depth: usize,
) -> bool {
    let has_set_operation = input.tokens.iter().enumerate().any(|(index, token)| {
        token.range.start() < input.point
            && input.depths[index] == select.depth
            && matches!(
                token.kind,
                TokenKind::Union | TokenKind::Intersect | TokenKind::Except
            )
    });
    let suffix_depth = select.depth.min(point_depth);
    has_set_operation
        && (select.index + 1..input.tokens.len()).any(|index| {
            input.depths[index] == suffix_depth
                && input.tokens[index].range.start() <= input.point
                && matches!(
                    input.tokens[index].kind,
                    TokenKind::Order
                        | TokenKind::Limit
                        | TokenKind::Offset
                        | TokenKind::Fetch
                        | TokenKind::For
                )
        })
}

fn wrapped_query_select_before_suffix(input: &ScopeInput<'_>, point_depth: usize) -> Option<usize> {
    let suffix = input
        .tokens
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, token)| {
            let is_suffix_keyword = token.range.start() <= input.point
                && input.depths[index] == point_depth
                && matches!(
                    token.kind,
                    TokenKind::Order
                        | TokenKind::Limit
                        | TokenKind::Offset
                        | TokenKind::Fetch
                        | TokenKind::For
                );
            is_suffix_keyword.then_some(index)
        })?;
    let close = suffix.checked_sub(1)?;
    if input.tokens[close].kind != TokenKind::Char(')') || input.depths[close] != point_depth {
        return None;
    }
    let open = (0..close).rev().find(|index| {
        input.tokens[*index].kind == TokenKind::Char('(')
            && input.depths[*index] == point_depth
            && matching_close(input.tokens, input.depths, *index, close + 1) == Some(close)
    })?;
    (open + 1..close).rev().find(|index| {
        input.tokens[*index].kind == TokenKind::Select && input.depths[*index] > point_depth
    })
}

fn query_scope(
    input: &ScopeInput<'_>,
    select: SelectLocation,
    ctes: &[CteDefinition],
) -> QueryScope {
    let Some(segment) = from_segment(input.tokens, input.depths, select) else {
        return QueryScope::default();
    };
    QueryScope {
        relations: parse_from_relations(
            input,
            segment.from_keyword + 1,
            segment.list_end,
            select.depth,
            ctes,
        ),
    }
}

fn apply_table_function_visibility(
    input: &ScopeInput<'_>,
    select: SelectLocation,
    scope: &mut QueryScope,
) {
    let Some(segment) = from_segment(input.tokens, input.depths, select) else {
        return;
    };
    let from_keyword = segment.from_keyword;
    // A scalar call in WHERE/GROUP BY/… matches the same name-then-paren
    // shape, so the search must stop at the end of the FROM list.
    let Some(open) = (from_keyword + 1..segment.list_end).find(|index| {
        input.tokens[*index].kind == TokenKind::Char('(')
            && input.depths[*index] == select.depth
            && input.tokens[*index].range.start() < input.point
            && matching_close(input.tokens, input.depths, *index, segment.list_end)
                .is_some_and(|close| input.tokens[close].range.end() >= input.point)
            && is_table_function_open(input.tokens, input.depths, *index, select.depth)
    }) else {
        return;
    };
    let item_start = (from_keyword + 1..open)
        .rev()
        .find(|index| {
            input.depths[*index] == select.depth
                && matches!(
                    input.tokens[*index].kind,
                    TokenKind::Char(',') | TokenKind::Join
                )
        })
        .map_or(from_keyword + 1, |delimiter| delimiter + 1);
    if (item_start..open).any(|index| {
        input.depths[index] == select.depth
            && matches!(input.tokens[index].kind, TokenKind::On | TokenKind::Using)
    }) {
        return;
    }
    let boundary = add(input.base, input.tokens[item_start].range.start());
    scope
        .relations
        .retain(|relation| relation.syntax_range.end() <= boundary);
}

fn apply_join_condition_visibility(
    input: &ScopeInput<'_>,
    select: SelectLocation,
    max_depth: usize,
    scope: &mut QueryScope,
) {
    let Some(segment) = from_segment(input.tokens, input.depths, select) else {
        return;
    };
    let Some(condition_start) = deepest_active_join_condition_start(
        input,
        segment.from_keyword + 1..segment.branch_end,
        select.depth..=max_depth,
    ) else {
        return;
    };
    let boundary = add(input.base, condition_start);
    scope
        .relations
        .retain(|relation| relation.syntax_range.end() <= boundary);
}

fn is_table_function_open(tokens: &[Token], depths: &[usize], open: usize, depth: usize) -> bool {
    let Some(previous) = open.checked_sub(1) else {
        return false;
    };
    if depths[previous] != depth {
        return false;
    }
    if tokens[previous].kind == TokenKind::From {
        return previous.checked_sub(1).is_some_and(|rows_keyword| {
            depths[rows_keyword] == depth && tokens[rows_keyword].kind == TokenKind::Rows
        });
    }
    token_can_be_name(&tokens[previous])
        && !matches!(
            tokens[previous].kind,
            TokenKind::Join | TokenKind::LateralP | TokenKind::Only
        )
}

fn parse_from_relations(
    input: &ScopeInput<'_>,
    list_start: usize,
    list_end: usize,
    list_depth: usize,
    ctes: &[CteDefinition],
) -> Vec<VisibleRelation> {
    let ScopeInput {
        source,
        base,
        point,
        tokens,
        depths,
    } = *input;
    let mut relations = Vec::new();
    let mut index = list_start;
    let mut expecting_item = true;
    while index < list_end {
        if depths[index] != list_depth {
            index += 1;
            continue;
        }
        let kind = tokens[index].kind;
        if matches!(kind, TokenKind::Char(',') | TokenKind::Join) {
            expecting_item = true;
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
            expecting_item = false;
            index += 1;
            continue;
        }
        if !expecting_item {
            index += 1;
            continue;
        }
        if kind == TokenKind::Char('(')
            && let Some(group_close) = matching_close(tokens, depths, index, list_end)
            && !parenthesized_body_is_query(tokens, depths, index, group_close)
        {
            let (alias, _, next_index) =
                parse_alias(source, base, tokens, group_close + 1, list_end);
            let point_in_group =
                tokens[index].range.end() <= point && point <= tokens[group_close].range.start();
            if alias.is_none() || point_in_group {
                relations.extend(parse_from_relations(
                    input,
                    index + 1,
                    group_close,
                    list_depth + 1,
                    ctes,
                ));
                index = next_index;
                expecting_item = false;
                continue;
            }
        }
        if let Some((relation, next_index)) =
            parse_from_item(input, index, list_end, list_depth, ctes)
        {
            relations.push(relation);
            index = next_index;
            expecting_item = false;
        } else {
            index += 1;
        }
    }
    relations
}

fn parse_from_item(
    input: &ScopeInput<'_>,
    mut index: usize,
    list_end: usize,
    item_depth: usize,
    ctes: &[CteDefinition],
) -> Option<(VisibleRelation, usize)> {
    let ScopeInput {
        source,
        base,
        point,
        tokens,
        depths,
    } = *input;
    let item_start = index;
    let lateral = consume_kind(tokens, &mut index, list_end, TokenKind::LateralP);
    let has_only = consume_kind(tokens, &mut index, list_end, TokenKind::Only);
    if index >= list_end {
        return None;
    }

    let only_parenthesis_close = if has_only && tokens[index].kind == TokenKind::Char('(') {
        let close = matching_close(tokens, depths, index, list_end)?;
        index += 1;
        Some(close)
    } else {
        None
    };

    if only_parenthesis_close.is_none() && tokens[index].kind == TokenKind::Char('(') {
        // Parenthesized FROM items can be subqueries, VALUES expressions, or
        // aliased join trees. Keep their common range/alias handling aligned.
        let body_open = index;
        let body_close = matching_close(tokens, depths, index, list_end)?;
        let is_query_body = parenthesized_body_is_query(tokens, depths, body_open, body_close);
        let is_values_body = first_parenthesized_body_kind(tokens, depths, body_open, body_close)
            == Some(TokenKind::Values);
        let relation_kind = if is_values_body {
            RelationKind::Values
        } else if is_query_body {
            RelationKind::Subquery
        } else {
            RelationKind::JoinAlias
        };
        let unsupported = (!is_query_body).then(|| UnsupportedRelation {
            range: absolute_range(
                base,
                tokens[body_open].range.start(),
                tokens[body_close].range.end(),
            ),
            reason: "parenthesized table expression is not classified".to_owned(),
        });
        index = body_close + 1;
        let (alias, explicit_columns, next_index) =
            parse_alias(source, base, tokens, index, list_end);
        let syntax_end = next_index
            .checked_sub(1)
            .and_then(|last| tokens.get(last))
            .map_or(tokens[body_close].range.end(), |token| token.range.end());
        return Some((
            VisibleRelation {
                kind: relation_kind,
                name: Vec::new(),
                alias,
                explicit_columns,
                qualified_only: false,
                syntax_range: absolute_range(base, tokens[item_start].range.start(), syntax_end),
                body_range: Some(absolute_range(
                    base,
                    tokens[body_open].range.end(),
                    tokens[body_close].range.start(),
                )),
                lateral,
                unsupported,
            },
            next_index,
        ));
    }

    if tokens[index].kind == TokenKind::Rows
        && tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::From)
        && tokens.get(index + 2).map(|token| token.kind) == Some(TokenKind::Char('('))
    {
        // ROWS FROM is one table function whose output columns may come from
        // either the relation alias or the individual function definitions.
        let body_open = index + 2;
        let body_close = matching_close(tokens, depths, body_open, list_end)?;
        index = body_close + 1;
        consume_with_ordinality(tokens, &mut index, list_end);
        let (alias, mut explicit_columns, next_index) =
            parse_function_alias(source, base, tokens, index, list_end);
        if explicit_columns.is_empty() {
            explicit_columns =
                parse_rows_from_columns(source, base, tokens, depths, body_open, body_close);
        }
        let syntax_end = next_index
            .checked_sub(1)
            .and_then(|last| tokens.get(last))
            .map_or(tokens[body_close].range.end(), |token| token.range.end());
        return Some((
            VisibleRelation {
                kind: RelationKind::TableFunction,
                name: Vec::new(),
                alias,
                explicit_columns,
                qualified_only: false,
                syntax_range: absolute_range(base, tokens[item_start].range.start(), syntax_end),
                body_range: Some(absolute_range(
                    base,
                    tokens[body_open].range.end(),
                    tokens[body_close].range.start(),
                )),
                lateral,
                unsupported: None,
            },
            next_index,
        ));
    }

    // Named relations, CTEs, and ordinary table-function calls share the same
    // qualified-name prefix and differ only after that name has been consumed.
    let (name, after_name) = parse_qualified_name(source, base, tokens, index, list_end, point)?;
    index = if let Some(close) = only_parenthesis_close {
        if after_name != close {
            return None;
        }
        close + 1
    } else {
        after_name
    };
    let is_table_function = only_parenthesis_close.is_none()
        && index < list_end
        && depths[index] == item_depth
        && tokens[index].kind == TokenKind::Char('(');
    let body_range = if is_table_function {
        let body_open = index;
        let body_close = matching_close(tokens, depths, body_open, list_end)?;
        index = body_close + 1;
        consume_with_ordinality(tokens, &mut index, list_end);
        Some(absolute_range(
            base,
            tokens[body_open].range.end(),
            tokens[body_close].range.start(),
        ))
    } else {
        None
    };
    let (alias, mut explicit_columns, next_index) = if is_table_function {
        parse_function_alias(source, base, tokens, index, list_end)
    } else {
        parse_alias(source, base, tokens, index, list_end)
    };
    let cte_definition = match name.as_slice() {
        [part] => ctes
            .iter()
            .find(|cte| cte.name.normalized == part.normalized),
        _ => None,
    };
    if explicit_columns.is_empty()
        && let Some(cte) = cte_definition
    {
        explicit_columns = cte.explicit_columns.clone();
    }
    let relation_kind = if is_table_function {
        RelationKind::TableFunction
    } else if cte_definition.is_some() {
        RelationKind::Cte
    } else {
        RelationKind::Relation
    };
    let syntax_end = next_index
        .checked_sub(1)
        .and_then(|last| tokens.get(last))
        .map_or(tokens[after_name - 1].range.end(), |token| {
            token.range.end()
        });
    Some((
        VisibleRelation {
            kind: relation_kind,
            name,
            alias,
            explicit_columns,
            qualified_only: false,
            syntax_range: absolute_range(base, tokens[item_start].range.start(), syntax_end),
            body_range,
            lateral,
            unsupported: None,
        },
        next_index,
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
        let first_token = open + 1;
        if first_token >= outer_close {
            return None;
        }
        if tokens[first_token].kind != TokenKind::Char('(') {
            return Some(tokens[first_token].kind);
        }
        let close = matching_close(tokens, depths, first_token, outer_close)?;
        if close + 1 != outer_close {
            return Some(tokens[first_token].kind);
        }
        open = first_token;
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
        let alias = name_part_from_token(source, base, &tokens[index]);
        index += 1;
        alias
    } else {
        None
    };
    let mut columns = Vec::new();
    if alias.is_some() && index < end && tokens[index].kind == TokenKind::Char('(') {
        index += 1;
        while index < end && tokens[index].kind != TokenKind::Char(')') {
            if let Some(column) = name_part_from_token(source, base, &tokens[index]) {
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
    // Unlike a regular relation alias, a function may use `AS (column type, …)`
    // without providing an alias name.
    let has_as = consume_kind(tokens, &mut index, end, TokenKind::As);
    if has_as && index < end && tokens[index].kind == TokenKind::Char('(') {
        let (columns, next) = parse_parenthesized_column_names(source, base, tokens, index, end);
        return (None, columns, next);
    }
    let alias = if index < end && token_is_alias(&tokens[index]) {
        let alias = name_part_from_token(source, base, &tokens[index]);
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
                if let Some(column) = name_part_from_token(source, base, &tokens[index]) {
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
    token_can_be_name(token)
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
    point: TextSize,
) -> Option<(Vec<NamePart>, usize)> {
    let mut parts = Vec::new();
    parts.push(name_part_from_token(source, base, tokens.get(index)?)?);
    index += 1;
    while index + 1 < end && tokens[index].kind == TokenKind::Char('.') && parts.len() < 3 {
        let Some(part) = name_part_from_token(source, base, &tokens[index + 1]) else {
            break;
        };
        parts.push(part);
        index += 2;
    }
    if index < end
        && tokens[index].kind == TokenKind::Char('.')
        && tokens[index].range.end() <= point
        && tokens
            .get(index + 1)
            .is_none_or(|token| token.range.start() >= point)
    {
        return None;
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

/// Cursor for the repeated `name [(columns)] AS ... (body)` entries after
/// `WITH`. It keeps token advancement and group-depth checks together so the
/// group loop only has to decide whether another comma-separated CTE follows.
struct CteCursor<'a> {
    source: &'a str,
    base: TextSize,
    tokens: &'a [Token],
    depths: &'a [usize],
    depth: usize,
    index: usize,
}

struct ParsedCteBody {
    start: TextSize,
    end: TextSize,
    syntax_end: TextSize,
}

impl<'a> CteCursor<'a> {
    fn after_with(input: ScopeInput<'a>, with_index: usize) -> Self {
        Self {
            source: input.source,
            base: input.base,
            tokens: input.tokens,
            depths: input.depths,
            depth: input.depths[with_index],
            index: with_index + 1,
        }
    }

    fn parse_definition(&mut self) -> Option<CteDefinition> {
        let syntax_start = self.current_start()?;
        let name = self.parse_name()?;
        let explicit_columns = self.parse_explicit_columns()?;

        self.expect(TokenKind::As)?;
        self.consume_materialization_modifier();
        let body = self.parse_body()?;

        Some(CteDefinition {
            name,
            explicit_columns,
            syntax_range: absolute_range(self.base, syntax_start, body.syntax_end),
            body_range: absolute_range(self.base, body.start, body.end),
        })
    }

    fn parse_name(&mut self) -> Option<NamePart> {
        if self.depths.get(self.index).copied() != Some(self.depth) {
            return None;
        }
        let name = name_part_from_token(self.source, self.base, self.tokens.get(self.index)?)?;
        self.index += 1;
        Some(name)
    }

    fn parse_explicit_columns(&mut self) -> Option<Vec<NamePart>> {
        if !self.current_is(TokenKind::Char('(')) {
            return Some(Vec::new());
        }

        let open = self.index;
        // Without this close, `AS` cannot be distinguished reliably from more
        // column-list input, so only an incomplete CTE body is recoverable.
        let close = matching_close(self.tokens, self.depths, open, self.tokens.len())?;
        let columns = (open + 1..close)
            .filter(|index| self.depths[*index] == self.depth + 1)
            .filter_map(|index| name_part_from_token(self.source, self.base, &self.tokens[index]))
            .collect();
        self.index = close + 1;
        Some(columns)
    }

    fn consume_materialization_modifier(&mut self) {
        // Preserve permissive recovery by consuming these independently;
        // syntax diagnostics belong to the parser, not scope collection.
        self.consume(TokenKind::Not);
        self.consume(TokenKind::Materialized);
    }

    fn parse_body(&mut self) -> Option<ParsedCteBody> {
        if !self.current_is(TokenKind::Char('(')) {
            return None;
        }

        let open = self.index;
        let close = matching_close(self.tokens, self.depths, open, self.tokens.len());
        // An unterminated body is normal while editing. Treat the remaining
        // input as its body so visibility can still be computed at the cursor.
        let end = close.map_or_else(
            || {
                self.tokens
                    .last()
                    .map_or(self.tokens[open].range.end(), |token| token.range.end())
            },
            |close| self.tokens[close].range.start(),
        );
        let syntax_end = close.map_or(end, |close| self.tokens[close].range.end());
        self.index = close.map_or(self.tokens.len(), |close| close + 1);

        Some(ParsedCteBody {
            start: self.tokens[open].range.end(),
            end,
            syntax_end,
        })
    }

    fn expect(&mut self, kind: TokenKind) -> Option<()> {
        self.consume(kind).then_some(())
    }

    fn consume(&mut self, kind: TokenKind) -> bool {
        if !self.current_is(kind) {
            return false;
        }
        self.index += 1;
        true
    }

    fn current_is(&self, kind: TokenKind) -> bool {
        self.depths.get(self.index).copied() == Some(self.depth)
            && self
                .tokens
                .get(self.index)
                .is_some_and(|token| token.kind == kind)
    }

    fn current_start(&self) -> Option<TextSize> {
        self.tokens.get(self.index).map(|token| token.range.start())
    }

    fn container_end(&self) -> TextSize {
        (self.index..self.tokens.len())
            .find(|candidate| self.depths[*candidate] < self.depth)
            .map_or_else(
                || {
                    self.tokens
                        .last()
                        .map_or(TextSize::ZERO, |token| token.range.end())
                },
                |candidate| self.tokens[candidate].range.start(),
            )
    }
}

fn collect_cte_groups(input: &ScopeInput<'_>) -> Vec<CteGroup> {
    let mut groups = Vec::new();
    for with_index in 0..input.tokens.len() {
        if input.tokens[with_index].kind != TokenKind::With {
            continue;
        }
        if let Some(group) = parse_cte_group(input, with_index) {
            groups.push(group);
        }
    }
    groups
}

fn parse_cte_group(input: &ScopeInput<'_>, with_index: usize) -> Option<CteGroup> {
    let mut cursor = CteCursor::after_with(*input, with_index);
    let recursive = cursor.consume(TokenKind::Recursive);
    let mut ctes = vec![cursor.parse_definition()?];
    while cursor.consume(TokenKind::Char(',')) {
        let definition_start = cursor.index;
        let Some(cte) = cursor.parse_definition() else {
            // Keep the valid prefix while the user is typing the next CTE or
            // recovering from a dangling comma. Parsing a definition advances
            // incrementally, so restore the cursor before exposing that prefix.
            cursor.index = definition_start;
            break;
        };
        ctes.push(cte);
    }

    let main_query_start = cursor
        .current_start()
        .unwrap_or_else(|| input.tokens[with_index].range.end());
    Some(CteGroup {
        depth: cursor.depth,
        start: add(input.base, input.tokens[with_index].range.start()),
        main_query_start: add(input.base, main_query_start),
        end: add(input.base, cursor.container_end()),
        recursive,
        ctes,
    })
}

fn visible_ctes_at_point(absolute_point: TextSize, groups: &[CteGroup]) -> Vec<CteDefinition> {
    let mut enclosing_groups = groups
        .iter()
        .filter(|group| group.start <= absolute_point && absolute_point <= group.end)
        .collect::<Vec<_>>();

    // Process nested groups first so an inner CTE shadows an outer CTE
    // with the same normalized name.
    enclosing_groups.sort_by_key(|group| std::cmp::Reverse(group.depth));

    let mut visible_ctes = Vec::<CteDefinition>::new();
    for group in enclosing_groups {
        let visible_count = visible_cte_count(group, absolute_point);

        for cte in group.ctes.iter().take(visible_count) {
            if visible_ctes
                .iter()
                .any(|existing| existing.name.normalized == cte.name.normalized)
            {
                continue;
            }
            visible_ctes.push(cte.clone());
        }
    }
    visible_ctes
}

fn visible_cte_count(group: &CteGroup, absolute_point: TextSize) -> usize {
    // A recursive CTE group exposes every CTE to every CTE body.
    if group.recursive {
        return group.ctes.len();
    }

    // A non-recursive CTE cannot reference itself or later CTEs
    // while its own body is being parsed.
    if let Some(active_body_index) = group.ctes.iter().position(|cte| {
        cte.body_range.start() <= absolute_point && absolute_point <= cte.body_range.end()
    }) {
        return active_body_index;
    }

    // Once the main query starts, all CTE definitions are visible.
    if absolute_point >= group.main_query_start {
        return group.ctes.len();
    }

    // Before the main query, only already-completed CTE definitions are visible.
    group
        .ctes
        .iter()
        .take_while(|cte| cte.syntax_range.end() <= absolute_point)
        .count()
}

fn collect_dml_scope(
    input: &ScopeInput<'_>,
    ctes: &[CteDefinition],
    relation_placement: DmlRelationPlacement,
    snapshot: &mut ScopeSnapshot,
) {
    let ScopeInput {
        base,
        point,
        tokens,
        depths,
        ..
    } = *input;
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
    let Some(statement) = active_dml_statement(input) else {
        return;
    };
    let statement_kind = tokens[statement].kind;
    let statement_depth = depths[statement];
    let statement_end = (statement + 1..tokens.len())
        .find(|index| depths[*index] < statement_depth)
        .unwrap_or(tokens.len());
    let target_start = match statement_kind {
        TokenKind::Insert | TokenKind::Merge | TokenKind::DeleteP => statement + 2,
        TokenKind::Update => statement + 1,
        _ => return,
    };
    if let Some((target, _)) = parse_dml_target(input, target_start, statement_end, statement_kind)
    {
        snapshot.dml_target = Some(target);
    }
    // INSERT source expressions cannot reference their target relation.
    if statement_kind == TokenKind::Insert
        && point_is_in_insert_source(input, statement, statement_end)
    {
        snapshot.dml_target = None;
    }
    // MERGE visibility changes between the source, ON, and WHEN clauses.
    if statement_kind == TokenKind::Merge
        && let Some(using_keyword) = (statement + 1..statement_end).find(|index| {
            depths[*index] == statement_depth && tokens[*index].kind == TokenKind::Using
        })
    {
        if let Some((source_relation, _)) = parse_from_item(
            input,
            using_keyword + 1,
            statement_end,
            statement_depth,
            ctes,
        ) {
            snapshot.merge_source = Some(source_relation);
        }
        let on_keyword = (using_keyword + 1..statement_end).find(|index| {
            depths[*index] == statement_depth && tokens[*index].kind == TokenKind::On
        });
        let source_range_end = on_keyword.map_or_else(
            || token_boundary(tokens, statement_end),
            |index| tokens[index].range.start(),
        );
        if tokens[using_keyword].range.end() <= point && point <= source_range_end {
            snapshot.dml_target = None;
            snapshot.merge_source = None;
        }
        apply_merge_when_clause_visibility(input, statement, statement_end, snapshot);
    }

    // UPDATE FROM and DELETE USING introduce ordinary FROM-list relations.
    let source_clause_kind = match statement_kind {
        TokenKind::Update => Some(TokenKind::From),
        TokenKind::DeleteP => Some(TokenKind::Using),
        _ => None,
    };
    if let Some(source_clause_kind) = source_clause_kind
        && let Some(source_keyword) = (statement + 1..statement_end).find(|index| {
            depths[*index] == statement_depth && tokens[*index].kind == source_clause_kind
        })
    {
        let source_list_end = (source_keyword + 1..statement_end)
            .find(|index| {
                depths[*index] == statement_depth && FROM_LIST_END.contains(&tokens[*index].kind)
            })
            .unwrap_or(statement_end);
        let source_range_end = token_boundary(tokens, source_list_end);
        if tokens[source_keyword].range.end() <= point && point <= source_range_end {
            snapshot.dml_target = None;
        }
        let mut relations = parse_from_relations(
            input,
            source_keyword + 1,
            source_list_end,
            statement_depth,
            ctes,
        );
        let absolute_point = input.absolute_point();
        if let Some(active_relation) = relations.iter().position(|relation| {
            relation.body_range.is_some_and(|range| {
                range.start() <= absolute_point && absolute_point <= range.end()
            })
        }) {
            let active_relation_start = relations[active_relation].syntax_range.start();
            let is_lateral = relations[active_relation].lateral
                || relations[active_relation].kind == RelationKind::TableFunction;
            if is_lateral {
                relations.retain(|relation| relation.syntax_range.end() <= active_relation_start);
                add_visible_relations(snapshot, relation_placement, relations);
            }
            snapshot.dml_target = None;
        } else {
            let max_join_depth = match relation_placement {
                DmlRelationPlacement::Outer => statement_depth,
                DmlRelationPlacement::Local => depth_at_point(tokens, point),
            };
            if let Some(condition_start) = deepest_active_join_condition_start(
                input,
                source_keyword + 1..source_list_end,
                statement_depth..=max_join_depth,
            ) {
                let boundary = add(base, condition_start);
                relations.retain(|relation| relation.syntax_range.end() <= boundary);
            }
            add_visible_relations(snapshot, relation_placement, relations);
        }
    }

    // ON CONFLICT and RETURNING add qualified views of the target.
    let Some(target) = snapshot.dml_target.clone() else {
        return;
    };
    if let Some(excluded) = insert_excluded_relation(input, statement, statement_end, &target) {
        add_visible_relations(snapshot, relation_placement, vec![excluded]);
    }
    let returning = returning_relations(input, statement, statement_end, &target);
    add_visible_relations(snapshot, relation_placement, returning);
}

fn add_visible_relations(
    snapshot: &mut ScopeSnapshot,
    placement: DmlRelationPlacement,
    relations: Vec<VisibleRelation>,
) {
    if relations.is_empty() {
        return;
    }
    match placement {
        DmlRelationPlacement::Local => snapshot.local.relations.extend(relations),
        DmlRelationPlacement::Outer => snapshot.outer.push(QueryScope { relations }),
    }
}

fn insert_excluded_relation(
    input: &ScopeInput<'_>,
    insert: usize,
    statement_end: usize,
    target: &VisibleRelation,
) -> Option<VisibleRelation> {
    if input.tokens[insert].kind != TokenKind::Insert {
        return None;
    }
    let statement_depth = input.depths[insert];
    let conflict_keyword = (insert + 1..statement_end).find(|index| {
        input.depths[*index] == statement_depth
            && input.tokens[*index].kind == TokenKind::On
            && input.tokens.get(*index + 1).map(|token| token.kind) == Some(TokenKind::Conflict)
    })?;
    let update_keyword = (conflict_keyword + 2..statement_end).find(|index| {
        input.depths[*index] == statement_depth
            && input.tokens[*index].kind == TokenKind::Do
            && input.tokens.get(*index + 1).map(|token| token.kind) == Some(TokenKind::Update)
    })? + 1;
    if input.tokens[update_keyword].range.end() > input.point {
        return None;
    }
    let returning_keyword = (update_keyword + 1..statement_end).find(|index| {
        input.depths[*index] == statement_depth && input.tokens[*index].kind == TokenKind::Returning
    });
    if returning_keyword
        .is_some_and(|returning| input.tokens[returning].range.start() <= input.point)
    {
        return None;
    }
    Some(qualified_target_relation(
        target,
        synthetic_name(input.base, &input.tokens[conflict_keyword], "excluded"),
    ))
}

fn returning_relations(
    input: &ScopeInput<'_>,
    statement: usize,
    statement_end: usize,
    target: &VisibleRelation,
) -> Vec<VisibleRelation> {
    let statement_depth = input.depths[statement];
    let Some(returning_keyword) = (statement + 1..statement_end).find(|index| {
        input.depths[*index] == statement_depth && input.tokens[*index].kind == TokenKind::Returning
    }) else {
        return Vec::new();
    };
    if input.tokens[returning_keyword].range.end() > input.point {
        return Vec::new();
    }

    let mut old_name = synthetic_name(input.base, &input.tokens[returning_keyword], "old");
    let mut new_name = synthetic_name(input.base, &input.tokens[returning_keyword], "new");
    let with_keyword = returning_keyword + 1;
    if input.tokens.get(with_keyword).map(|token| token.kind) == Some(TokenKind::With) {
        let open = with_keyword + 1;
        if input.tokens.get(open).map(|token| token.kind) != Some(TokenKind::Char('('))
            || input.depths[open] != statement_depth
        {
            return Vec::new();
        }
        let Some(close) = matching_close(input.tokens, input.depths, open, statement_end) else {
            return Vec::new();
        };
        if input.tokens[close].range.end() > input.point {
            return Vec::new();
        }
        let mut index = open + 1;
        while index < close {
            if input.depths[index] == statement_depth + 1
                && matches!(input.tokens[index].kind, TokenKind::Old | TokenKind::New)
                && input.tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::As)
                && let Some(alias) = input
                    .tokens
                    .get(index + 2)
                    .and_then(|token| name_part_from_token(input.source, input.base, token))
            {
                if input.tokens[index].kind == TokenKind::Old {
                    old_name = alias;
                } else {
                    new_name = alias;
                }
                index += 3;
            } else {
                index += 1;
            }
        }
    }

    vec![
        qualified_target_relation(target, old_name),
        qualified_target_relation(target, new_name),
    ]
}

fn qualified_target_relation(target: &VisibleRelation, alias: NamePart) -> VisibleRelation {
    let mut relation = target.clone();
    relation.syntax_range = alias.range;
    relation.alias = Some(alias);
    relation.qualified_only = true;
    relation
}

fn synthetic_name(base: TextSize, token: &Token, text: &str) -> NamePart {
    NamePart {
        text: text.to_owned(),
        normalized: text.to_owned(),
        quoted: false,
        range: absolute_range(base, token.range.start(), token.range.end()),
    }
}

fn active_dml_statement(input: &ScopeInput<'_>) -> Option<usize> {
    // Keep the latest statement head at each still-open nesting depth. Closing
    // a sibling group removes its candidates before a later group is scanned.
    let mut statement_heads_by_depth = Vec::<usize>::new();
    for (index, token) in input.tokens.iter().enumerate() {
        if token.range.start() > input.point {
            break;
        }
        if token.kind == TokenKind::Char(')') && token.range.start() < input.point {
            statement_heads_by_depth
                .retain(|candidate| input.depths[*candidate] <= input.depths[index]);
        }
        if !is_dml_statement_head(input.tokens, index) {
            continue;
        }
        if let Some(candidate) = statement_heads_by_depth
            .iter_mut()
            .find(|candidate| input.depths[**candidate] == input.depths[index])
        {
            *candidate = index;
        } else {
            statement_heads_by_depth.push(index);
        }
    }
    statement_heads_by_depth
        .into_iter()
        .max_by_key(|index| input.depths[*index])
}

fn is_dml_statement_head(tokens: &[Token], index: usize) -> bool {
    match tokens[index].kind {
        TokenKind::Insert | TokenKind::Merge => {
            tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::Into)
        }
        TokenKind::DeleteP => {
            tokens.get(index + 1).map(|token| token.kind) == Some(TokenKind::From)
        }
        TokenKind::Update => {
            // UPDATE is also a row-lock or MERGE/ON CONFLICT action keyword.
            !index.checked_sub(1).is_some_and(|previous| {
                matches!(
                    tokens[previous].kind,
                    TokenKind::For | TokenKind::Key | TokenKind::Then | TokenKind::Do
                )
            }) && tokens
                .get(index + 1)
                .is_some_and(|next| next.kind == TokenKind::Only || token_can_be_name(next))
        }
        _ => false,
    }
}

fn apply_merge_when_clause_visibility(
    input: &ScopeInput<'_>,
    merge: usize,
    statement_end: usize,
    snapshot: &mut ScopeSnapshot,
) {
    let statement_depth = input.depths[merge];
    let mut case_depth = 0usize;
    let mut active_when_keyword = None;
    let mut active_clause_end = None;
    for index in merge + 1..statement_end {
        if input.depths[index] != statement_depth {
            continue;
        }
        match input.tokens[index].kind {
            TokenKind::Case => case_depth += 1,
            TokenKind::EndP if case_depth > 0 => case_depth -= 1,
            TokenKind::When if case_depth == 0 => {
                if input.tokens[index].range.start() <= input.point {
                    active_when_keyword = Some(index);
                } else if active_when_keyword.is_some() {
                    active_clause_end = Some(input.tokens[index].range.start());
                    break;
                }
            }
            TokenKind::Returning if case_depth == 0 && active_when_keyword.is_some() => {
                active_clause_end = Some(input.tokens[index].range.start());
                break;
            }
            _ => {}
        }
    }
    let Some(when_keyword) = active_when_keyword else {
        return;
    };
    if active_clause_end.is_some_and(|end| input.point >= end)
        || input.tokens.get(when_keyword + 1).map(|token| token.kind) != Some(TokenKind::Not)
        || input.tokens.get(when_keyword + 2).map(|token| token.kind) != Some(TokenKind::Matched)
    {
        return;
    }

    match (
        input.tokens.get(when_keyword + 3).map(|token| token.kind),
        input.tokens.get(when_keyword + 4).map(|token| token.kind),
    ) {
        // NOT MATCHED BY SOURCE has a target row but no source row.
        (Some(TokenKind::By), Some(TokenKind::Source)) => snapshot.merge_source = None,
        // NOT MATCHED [BY TARGET] has a source row but no target row.
        (Some(TokenKind::By), Some(TokenKind::Target))
        | (Some(TokenKind::And | TokenKind::Then), _) => {
            snapshot.dml_target = None;
        }
        _ => {}
    }
}

fn point_is_in_insert_source(input: &ScopeInput<'_>, insert: usize, statement_end: usize) -> bool {
    let statement_depth = input.depths[insert];
    let Some(source_start) = (insert + 1..statement_end).find(|index| {
        input.depths[*index] >= statement_depth
            && matches!(
                input.tokens[*index].kind,
                TokenKind::Values
                    | TokenKind::Select
                    | TokenKind::With
                    | TokenKind::Table
                    | TokenKind::Execute
                    | TokenKind::Default
            )
    }) else {
        return false;
    };
    let source_end = (source_start + 1..statement_end)
        .find(|index| {
            input.depths[*index] == statement_depth
                && (input.tokens[*index].kind == TokenKind::Returning
                    || (input.tokens[*index].kind == TokenKind::On
                        && input.tokens.get(*index + 1).map(|token| token.kind)
                            == Some(TokenKind::Conflict)))
        })
        .map_or_else(
            || token_boundary(input.tokens, statement_end),
            |index| input.tokens[index].range.start(),
        );
    input.tokens[source_start].range.start() <= input.point && input.point <= source_end
}

/// Start of `tokens[index]`, or the end of the token stream when `index == len`.
fn token_boundary(tokens: &[Token], index: usize) -> TextSize {
    tokens.get(index).map_or_else(
        || {
            tokens
                .last()
                .map_or(TextSize::ZERO, |token| token.range.end())
        },
        |token| token.range.start(),
    )
}

/// Start of the ON/USING clause containing the point, if one is active.
fn active_join_condition_start(
    input: &ScopeInput<'_>,
    search_range: std::ops::Range<usize>,
    condition_depth: usize,
) -> Option<TextSize> {
    let search_end = search_range.end;
    search_range.rev().find_map(|index| {
        let at_condition_depth = input.depths[index] == condition_depth;
        let before_point = input.tokens[index].range.start() <= input.point;
        if !at_condition_depth || !before_point {
            return None;
        }
        match input.tokens[index].kind {
            TokenKind::On => {
                let condition_end = (index + 1..search_end)
                    .find(|candidate| {
                        input.depths[*candidate] < condition_depth
                            || (input.depths[*candidate] == condition_depth
                                && (matches!(
                                    input.tokens[*candidate].kind,
                                    TokenKind::Char(',') | TokenKind::Join
                                ) || FROM_LIST_END.contains(&input.tokens[*candidate].kind)))
                    })
                    .map_or_else(
                        || token_boundary(input.tokens, search_end),
                        |candidate| input.tokens[candidate].range.start(),
                    );
                (input.point <= condition_end).then_some(input.tokens[index].range.start())
            }
            TokenKind::Using => {
                let open = index + 1;
                if open >= search_end
                    || input.depths[open] != condition_depth
                    || input.tokens[open].kind != TokenKind::Char('(')
                    || input.point < input.tokens[open].range.end()
                {
                    return None;
                }
                let is_active = matching_close(input.tokens, input.depths, open, search_end)
                    .is_none_or(|close| input.point <= input.tokens[close].range.start());
                is_active.then_some(input.tokens[index].range.start())
            }
            _ => None,
        }
    })
}

fn deepest_active_join_condition_start(
    input: &ScopeInput<'_>,
    search_range: std::ops::Range<usize>,
    depth_range: std::ops::RangeInclusive<usize>,
) -> Option<TextSize> {
    depth_range
        .rev()
        .find_map(|depth| active_join_condition_start(input, search_range.clone(), depth))
}

fn parse_dml_target(
    input: &ScopeInput<'_>,
    mut index: usize,
    statement_end: usize,
    statement_kind: TokenKind,
) -> Option<(VisibleRelation, usize)> {
    let ScopeInput {
        source,
        base,
        point,
        tokens,
        ..
    } = *input;
    let target_start = index;
    consume_kind(tokens, &mut index, statement_end, TokenKind::Only);
    let parenthesized_target =
        consume_kind(tokens, &mut index, statement_end, TokenKind::Char('('));
    let (name, after_name) =
        parse_qualified_name(source, base, tokens, index, statement_end, point)?;
    index = after_name;
    if parenthesized_target {
        consume_kind(tokens, &mut index, statement_end, TokenKind::Char(')'));
    }
    consume_kind(tokens, &mut index, statement_end, TokenKind::Char('*'));
    let (alias, explicit_columns, next) = if statement_kind == TokenKind::Insert {
        if consume_kind(tokens, &mut index, statement_end, TokenKind::As) {
            let alias = name_part_from_token(source, base, tokens.get(index)?)?;
            index += 1;
            if index < statement_end && tokens[index].kind == TokenKind::Char('(') {
                let (columns, next) =
                    parse_parenthesized_column_names(source, base, tokens, index, statement_end);
                (Some(alias), columns, next)
            } else {
                (Some(alias), Vec::new(), index)
            }
        } else {
            (None, Vec::new(), index)
        }
    } else {
        let (alias, columns, next) = parse_alias(source, base, tokens, index, statement_end);
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
            qualified_only: false,
            syntax_range: absolute_range(base, tokens[target_start].range.start(), syntax_end),
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

fn token_can_be_name(token: &Token) -> bool {
    matches!(
        &token.value,
        Some(TokenValue::String(_) | TokenValue::Keyword(_))
    )
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
    fn dangling_schema_qualifier_is_not_a_visible_relation() {
        let sql = "SELECT * FROM public.";
        let point = TextSize::try_from(sql.len()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert!(scope.local.relations.is_empty(), "{scope:?}");
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

        let qualified = "WITH first(a) AS (SELECT 1) SELECT marker FROM public.first";
        let point = TextSize::try_from(qualified.find("marker").unwrap()).unwrap();
        let scope = collect(qualified, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations[0].kind, RelationKind::Relation);
        assert!(scope.local.relations[0].explicit_columns.is_empty());
    }

    #[test]
    fn keeps_scope_for_an_unterminated_cte_body() {
        let sql = "WITH first AS (SELECT 1), second AS (SELECT * FROM first";
        let point = TextSize::try_from(sql.len()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();

        assert_eq!(
            scope
                .ctes
                .iter()
                .map(|cte| cte.name.normalized.as_str())
                .collect::<Vec<_>>(),
            ["first"]
        );
        assert_eq!(scope.local.relations[0].kind, RelationKind::Cte);
        assert_eq!(scope.local.relations[0].name[0].normalized, "first");
    }

    #[test]
    fn keeps_completed_ctes_after_a_trailing_comma() {
        let sql = "WITH first AS (SELECT 1),";
        let point = TextSize::try_from(sql.len()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();

        assert_eq!(
            scope
                .ctes
                .iter()
                .map(|cte| cte.name.normalized.as_str())
                .collect::<Vec<_>>(),
            ["first"]
        );
    }

    #[test]
    fn resolves_completed_ctes_in_a_main_query_after_a_dangling_comma() {
        let sql = "WITH first AS (SELECT 1), SELECT marker FROM first";
        let point = TextSize::try_from(sql.find("marker").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();

        assert_eq!(scope.ctes[0].name.normalized, "first");
        assert_eq!(scope.local.relations[0].kind, RelationKind::Cte);
        assert_eq!(scope.local.relations[0].name[0].normalized, "first");
    }

    #[test]
    fn parses_cte_materialization_modifiers() {
        for modifier in ["MATERIALIZED", "NOT MATERIALIZED"] {
            let sql = format!("WITH first AS {modifier} (SELECT 1) SELECT marker FROM first");
            let point = TextSize::try_from(sql.find("marker").unwrap()).unwrap();
            let scope = collect(&sql, TextSize::ZERO, point).unwrap();

            assert_eq!(scope.local.relations[0].kind, RelationKind::Cte);
            assert_eq!(scope.local.relations[0].name[0].normalized, "first");
        }
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
    fn join_using_lists_do_not_see_later_from_items() {
        let sql =
            "SELECT * FROM accounts a JOIN users u USING (id) JOIN later_relation later ON true";
        let point = TextSize::try_from(sql.find("id)").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert_eq!(scope.local.relations.len(), 2);
        assert_eq!(
            scope
                .local
                .relations
                .iter()
                .map(|relation| relation.alias.as_ref().unwrap().normalized.as_str())
                .collect::<Vec<_>>(),
            ["a", "u"]
        );
    }

    #[test]
    fn function_calls_past_the_from_list_keep_the_from_scope() {
        for sql in [
            "SELECT * FROM t WHERE lower(x)",
            "SELECT * FROM t GROUP BY lower(x)",
            "SELECT * FROM t HAVING count(x) > 0",
            "SELECT * FROM t ORDER BY lower(x)",
        ] {
            let point = TextSize::try_from(sql.rfind('x').unwrap()).unwrap();
            let scope = collect(sql, TextSize::ZERO, point).unwrap();
            assert_eq!(scope.local.relations.len(), 1, "{sql}");
            assert_eq!(scope.local.relations[0].name[0].normalized, "t", "{sql}");
        }

        let sql = "SELECT * FROM generate_series(1, x) g WHERE true";
        let point = TextSize::try_from(sql.rfind("x)").unwrap()).unwrap();
        let scope = collect(sql, TextSize::ZERO, point).unwrap();
        assert!(
            scope.local.relations.is_empty(),
            "a FROM-list table function still sees only preceding items"
        );
    }

    #[test]
    fn parenthesized_query_suffix_keeps_the_query_scope() {
        for sql in [
            "(SELECT * FROM users AS u) ORDER BY u.id",
            "(SELECT * FROM users AS u) LIMIT u.limit_value",
        ] {
            let point = TextSize::try_from(sql.len()).unwrap();
            let scope = collect(sql, TextSize::ZERO, point).unwrap();
            assert_eq!(scope.local.relations.len(), 1, "{sql}");
            assert_eq!(
                scope.local.relations[0].alias.as_ref().unwrap().normalized,
                "u",
                "{sql}"
            );
        }
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
