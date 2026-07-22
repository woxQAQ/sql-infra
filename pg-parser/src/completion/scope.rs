use std::collections::{HashMap, HashSet};

use super::{
    BindingGraph, CteBinding, CteBindingId, QualifiedName, RangeBinding, RangeBindingId,
    RangeSource, RowColumnOrigin, RowShape, RowShapeItem, ScopeFrame, ScopeSnapshot,
    TargetRelation, TargetRelationId, text_size,
};
use crate::{KeywordCategory, TextRange, Token, TokenKind, TokenValue, lookup_keyword};

#[derive(Clone)]
struct DepthToken {
    token: Token,
    depth: usize,
}

#[derive(Clone)]
struct CteSpec {
    id: CteBindingId,
    name: String,
    column_aliases: Vec<String>,
    start: usize,
    open: usize,
    close: usize,
}

#[derive(Clone)]
struct WithClause {
    with_index: usize,
    depth: usize,
    recursive: bool,
    ctes: Vec<CteSpec>,
    main_start: usize,
    container_end: usize,
}

#[derive(Clone, Default)]
struct QueryAnalysis {
    ranges: Vec<RangeBindingId>,
    row_shape: RowShape,
}

struct ScopeAnalyzer<'a> {
    tokens: &'a [DepthToken],
    graph: BindingGraph,
    with_clauses: Vec<WithClause>,
    query_cache: HashMap<usize, QueryAnalysis>,
    analyzing_queries: HashSet<usize>,
}

pub(super) fn collect_scope(tokens: &[Token], cursor: usize) -> ScopeSnapshot {
    let tokens = with_depth(tokens);
    let cursor_depth = depth_at(&tokens, cursor);
    let mut analyzer = ScopeAnalyzer::new(&tokens);
    analyzer.resolve_cte_shapes();

    let mut select_by_depth = HashMap::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.token.kind == TokenKind::Select
            && token.token.location() <= cursor
            && token.depth <= cursor_depth
        {
            select_by_depth.insert(token.depth, index);
        }
    }
    let mut selects: Vec<(usize, usize)> = select_by_depth.into_iter().collect();
    selects.sort_by_key(|(depth, _)| *depth);

    let cursor_index = token_index_at(&tokens, cursor);
    let visible_ctes = analyzer.visible_ctes_at(cursor_index);
    let mut query_frames = Vec::new();
    for (_, select) in &selects {
        let analysis = analyzer.analyze_query(*select);
        query_frames.push((*select, analysis.ranges));
    }

    let mut local_ranges = query_frames
        .pop()
        .map(|(_, ranges)| ranges)
        .unwrap_or_default();
    if let Some((_, innermost_select)) = selects.last()
        && cursor_is_in_from_clause(&tokens, *innermost_select, cursor)
    {
        let cutoff = local_ranges
            .iter()
            .find_map(|id| {
                let binding = &analyzer.graph.ranges[id.0];
                (matches!(&binding.source, RangeSource::Function(_))
                    && usize::from(binding.range.start()) < cursor
                    && cursor <= usize::from(binding.range.end()))
                .then_some(usize::from(binding.range.start()))
            })
            .unwrap_or(cursor);
        local_ranges.retain(|id| usize::from(analyzer.graph.ranges[id.0].range.start()) < cutoff);
    }
    if let Some((statement, statement_kind)) = top_level_dml_statement(&tokens) {
        if statement_kind == TokenKind::Insert
            && [TokenKind::Conflict, TokenKind::Returning]
                .into_iter()
                .any(|kind| {
                    find_top_level_token_after(&tokens, statement, kind)
                        .is_some_and(|index| tokens[index].token.location() < cursor)
                })
        {
            local_ranges.clear();
        }
        if statement_kind == TokenKind::Merge {
            local_ranges.extend(analyzer.collect_merge_ranges());
        } else if matches!(statement_kind, TokenKind::Update | TokenKind::DeleteP) {
            local_ranges.extend(analyzer.collect_update_delete_ranges());
        }
    }

    let mut frames = vec![ScopeFrame {
        ranges: local_ranges,
        ctes: visible_ctes,
    }];
    for (select, mut ranges) in query_frames.into_iter().rev() {
        if let Some((lateral, open_location)) = cursor_from_subquery(&tokens, select, cursor) {
            if lateral {
                ranges.retain(|id| {
                    usize::from(analyzer.graph.ranges[id.0].range.start()) < open_location
                });
            } else {
                ranges.clear();
            }
        }
        if !ranges.is_empty() {
            frames.push(ScopeFrame {
                ranges,
                ctes: Vec::new(),
            });
        }
    }

    let target_relation = collect_target_relation(&tokens).map(|target| {
        let id = TargetRelationId(analyzer.graph.target_relations.len());
        analyzer.graph.target_relations.push(target);
        id
    });
    ScopeSnapshot {
        frames,
        graph: analyzer.graph,
        target_relation,
    }
}

impl<'a> ScopeAnalyzer<'a> {
    fn new(tokens: &'a [DepthToken]) -> Self {
        let mut graph = BindingGraph::default();
        let mut with_clauses = Vec::new();
        for with_index in tokens
            .iter()
            .enumerate()
            .filter_map(|(index, token)| (token.token.kind == TokenKind::With).then_some(index))
        {
            let Some(mut clause) = parse_with_clause(tokens, with_index) else {
                continue;
            };
            for spec in &mut clause.ctes {
                let id = CteBindingId(graph.ctes.len());
                spec.id = id;
                graph.ctes.push(CteBinding {
                    name: spec.name.clone(),
                    column_aliases: spec.column_aliases.clone(),
                    row_shape: RowShape::default(),
                    range: TextRange::new(
                        tokens[spec.start].token.range.start(),
                        text_size(tokens[spec.close].token.end_location()),
                    ),
                });
            }
            with_clauses.push(clause);
        }
        Self {
            tokens,
            graph,
            with_clauses,
            query_cache: HashMap::new(),
            analyzing_queries: HashSet::new(),
        }
    }

    fn resolve_cte_shapes(&mut self) {
        let specs = self
            .with_clauses
            .iter()
            .flat_map(|clause| clause.ctes.iter())
            .map(|spec| (spec.id, spec.open, spec.close))
            .collect::<Vec<_>>();
        for (id, open, close) in specs {
            let row_shape = self.row_shape_in_parentheses(open, close);
            self.graph.ctes[id.0].row_shape = row_shape;
        }
    }

    fn row_shape_in_parentheses(&mut self, open: usize, close: usize) -> RowShape {
        if let Some(select) = select_in_parentheses(self.tokens, open, close) {
            self.analyze_query(select).row_shape
        } else if let Some(values) = values_in_parentheses(self.tokens, open, close) {
            collect_values_row_shape(self.tokens, values, close)
        } else {
            RowShape::default()
        }
    }

    fn visible_ctes_at(&self, index: usize) -> Vec<CteBindingId> {
        let mut clauses = self
            .with_clauses
            .iter()
            .filter(|clause| {
                clause.with_index <= index
                    && index < clause.container_end.max(clause.main_start + 1)
            })
            .collect::<Vec<_>>();
        clauses.sort_by(|left, right| {
            right
                .depth
                .cmp(&left.depth)
                .then_with(|| right.with_index.cmp(&left.with_index))
        });

        let mut result = Vec::new();
        let mut names = HashSet::new();
        for clause in clauses {
            let ids: Vec<CteBindingId> = if let Some(position) = clause
                .ctes
                .iter()
                .position(|cte| cte.open < index && index <= cte.close)
            {
                if clause.recursive {
                    clause.ctes.iter().map(|cte| cte.id).collect()
                } else {
                    clause.ctes[..position].iter().map(|cte| cte.id).collect()
                }
            } else if index >= clause.main_start {
                clause.ctes.iter().map(|cte| cte.id).collect()
            } else {
                Vec::new()
            };
            for id in ids {
                let key = self.graph.ctes[id.0].name.to_ascii_lowercase();
                if names.insert(key) {
                    result.push(id);
                }
            }
        }
        result
    }

    fn cte_at(&self, index: usize, name: &QualifiedName) -> Option<CteBindingId> {
        if name.catalog.is_some() || name.schema.is_some() {
            return None;
        }
        self.visible_ctes_at(index)
            .into_iter()
            .find(|id| self.graph.ctes[id.0].name.eq_ignore_ascii_case(&name.name))
    }

    fn analyze_query(&mut self, select: usize) -> QueryAnalysis {
        if let Some(analysis) = self.query_cache.get(&select) {
            return analysis.clone();
        }
        if !self.analyzing_queries.insert(select) {
            return QueryAnalysis::default();
        }
        let ranges = find_from(self.tokens, select)
            .map(|from| self.parse_from_bindings(from + 1))
            .unwrap_or_default();
        let row_shape = collect_select_row_shape(self.tokens, select, &ranges, &self.graph);
        let analysis = QueryAnalysis { ranges, row_shape };
        self.analyzing_queries.remove(&select);
        self.query_cache.insert(select, analysis.clone());
        analysis
    }

    fn parse_from_bindings(&mut self, mut index: usize) -> Vec<RangeBindingId> {
        let depth = self.tokens.get(index).map_or(0, |token| token.depth);
        let mut result = Vec::new();
        while index < self.tokens.len() {
            let token = &self.tokens[index];
            if token.depth < depth || (token.depth == depth && is_from_terminator(token.token.kind))
            {
                break;
            }
            if token.depth != depth || is_join_noise(token.token.kind) {
                index += 1;
                continue;
            }
            if matches!(token.token.kind, TokenKind::On | TokenKind::Using) {
                index = skip_join_condition(self.tokens, index + 1, depth);
                continue;
            }
            let lateral = token.token.kind == TokenKind::LateralP;
            if lateral {
                index += 1;
            }
            let Some(current) = self.tokens.get(index) else {
                break;
            };
            if current.token.kind == TokenKind::Char('(') {
                let Some(close) = matching_paren(self.tokens, index) else {
                    break;
                };
                let row_shape = self.row_shape_in_parentheses(index, close);
                let (alias, column_aliases, next) =
                    parse_alias(self.tokens, close + 1, depth, false);
                let name = alias.clone().unwrap_or_default();
                let end = binding_end(self.tokens, current.token.end_location(), next);
                result.push(self.push_range(RangeBinding {
                    source: RangeSource::Derived(row_shape),
                    name,
                    alias,
                    column_aliases,
                    range: TextRange::new(current.token.range.start(), text_size(end)),
                    lateral,
                }));
                index = next;
                continue;
            }

            let start = current.token.location();
            let (parts, next) = parse_qualified_name(self.tokens, index, depth);
            if parts.is_empty() {
                index += 1;
                continue;
            }
            index = next;
            let is_function = self.tokens.get(index).is_some_and(|token| {
                token.depth == depth && token.token.kind == TokenKind::Char('(')
            });
            if is_function && let Some(close) = matching_paren(self.tokens, index) {
                index = close + 1;
            }
            if self
                .tokens
                .get(index)
                .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::With)
                && self.tokens.get(index + 1).is_some_and(|token| {
                    token.depth == depth && token.token.kind == TokenKind::Ordinality
                })
            {
                index += 2;
            }
            let (alias, column_aliases, next) = parse_alias(self.tokens, index, depth, is_function);
            index = next;
            let qualified = qualified_name(parts);
            let source = if is_function {
                RangeSource::Function(qualified.clone())
            } else if let Some(cte) = self.cte_at(next.saturating_sub(1), &qualified) {
                RangeSource::Cte(cte)
            } else {
                RangeSource::Relation(qualified.clone())
            };
            let default_name = match &source {
                RangeSource::Cte(id) => self.graph.ctes[id.0].name.clone(),
                _ => qualified.name.clone(),
            };
            let name = alias.clone().unwrap_or(default_name);
            let end = binding_end(self.tokens, current.token.end_location(), index);
            result.push(self.push_range(RangeBinding {
                source,
                name,
                alias,
                column_aliases,
                range: TextRange::new(text_size(start), text_size(end)),
                lateral: lateral || is_function,
            }));
        }
        result
    }

    fn push_range(&mut self, binding: RangeBinding) -> RangeBindingId {
        let id = RangeBindingId(self.graph.ranges.len());
        self.graph.ranges.push(binding);
        id
    }

    fn collect_merge_ranges(&mut self) -> Vec<RangeBindingId> {
        [TokenKind::Into, TokenKind::Using]
            .into_iter()
            .filter_map(|marker| {
                let start = self
                    .tokens
                    .iter()
                    .position(|token| token.depth == 0 && token.token.kind == marker)?
                    + 1;
                self.relation_range_at(start, marker != TokenKind::Into)
            })
            .collect()
    }

    fn collect_update_delete_ranges(&mut self) -> Vec<RangeBindingId> {
        let Some((statement, first)) = top_level_dml_statement(self.tokens) else {
            return Vec::new();
        };
        let (target_start, source_marker) = match first {
            TokenKind::Update => (statement + 1, TokenKind::From),
            TokenKind::DeleteP => {
                let Some(from) =
                    find_top_level_token_after(self.tokens, statement, TokenKind::From)
                else {
                    return Vec::new();
                };
                (from + 1, TokenKind::Using)
            }
            _ => return Vec::new(),
        };
        let mut result = self
            .relation_range_at(target_start, false)
            .into_iter()
            .collect::<Vec<_>>();
        if let Some(source) = self
            .tokens
            .iter()
            .position(|token| token.depth == 0 && token.token.kind == source_marker)
        {
            result.extend(self.parse_from_bindings(source + 1));
        }
        result
    }

    fn relation_range_at(&mut self, start: usize, allow_cte: bool) -> Option<RangeBindingId> {
        let first = self.tokens.get(start)?;
        let (parts, next) = parse_qualified_name(self.tokens, start, 0);
        if parts.is_empty() {
            return None;
        }
        let qualified = qualified_name(parts);
        let (alias, column_aliases, end) = parse_alias(self.tokens, next, 0, false);
        let source = if allow_cte {
            self.cte_at(start, &qualified)
                .map(RangeSource::Cte)
                .unwrap_or_else(|| RangeSource::Relation(qualified.clone()))
        } else {
            RangeSource::Relation(qualified.clone())
        };
        let default_name = match &source {
            RangeSource::Cte(id) => self.graph.ctes[id.0].name.clone(),
            _ => qualified.name,
        };
        let name = alias.clone().unwrap_or(default_name);
        let end_location = binding_end(self.tokens, first.token.end_location(), end);
        Some(self.push_range(RangeBinding {
            source,
            name,
            alias,
            column_aliases,
            range: TextRange::new(first.token.range.start(), text_size(end_location)),
            lateral: false,
        }))
    }
}

fn with_depth(tokens: &[Token]) -> Vec<DepthToken> {
    let mut depth = 0usize;
    let mut result = Vec::with_capacity(tokens.len());
    for token in tokens {
        result.push(DepthToken {
            token: token.clone(),
            depth,
        });
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    result
}

fn depth_at(tokens: &[DepthToken], cursor: usize) -> usize {
    let mut depth = 0usize;
    for token in tokens {
        if token.token.location() >= cursor {
            break;
        }
        match token.token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn token_index_at(tokens: &[DepthToken], cursor: usize) -> usize {
    tokens
        .iter()
        .position(|token| token.token.location() >= cursor)
        .unwrap_or(tokens.len())
}

fn parse_with_clause(tokens: &[DepthToken], with_index: usize) -> Option<WithClause> {
    let depth = tokens.get(with_index)?.depth;
    let mut index = with_index + 1;
    let recursive = tokens
        .get(index)
        .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Recursive);
    if recursive {
        index += 1;
    }
    let mut ctes = Vec::new();
    loop {
        let start = index;
        let name = token_name(tokens.get(index).map(|token| &token.token))?;
        if tokens[index].depth != depth {
            return None;
        }
        index += 1;
        let mut column_aliases = Vec::new();
        if tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char('('))
        {
            let close = matching_paren(tokens, index)?;
            column_aliases = tokens[index + 1..close]
                .iter()
                .filter_map(|token| token_name(Some(&token.token)))
                .collect();
            index = close + 1;
        }
        if !tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::As)
        {
            return None;
        }
        index += 1;
        if tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Not)
        {
            index += 1;
        }
        if tokens.get(index).is_some_and(|token| {
            token.depth == depth && token.token.kind == TokenKind::Materialized
        }) {
            index += 1;
        }
        let open = index;
        if !tokens
            .get(open)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char('('))
        {
            return None;
        }
        let close = matching_paren(tokens, open)?;
        ctes.push(CteSpec {
            id: CteBindingId(usize::MAX),
            name,
            column_aliases,
            start,
            open,
            close,
        });
        index = close + 1;
        if tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char(','))
        {
            index += 1;
            continue;
        }
        break;
    }
    Some(WithClause {
        with_index,
        depth,
        recursive,
        ctes,
        main_start: index,
        container_end: query_container_end(tokens, with_index, depth),
    })
}

fn query_container_end(tokens: &[DepthToken], index: usize, depth: usize) -> usize {
    if depth == 0 {
        return tokens.len() + 1;
    }
    tokens[..index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(open, token)| {
            (token.depth + 1 == depth && token.token.kind == TokenKind::Char('('))
                .then(|| matching_paren(tokens, open))
                .flatten()
                .filter(|close| *close >= index)
                .map(|close| close + 1)
        })
        .unwrap_or(tokens.len() + 1)
}

fn select_in_parentheses(tokens: &[DepthToken], open: usize, close: usize) -> Option<usize> {
    let depth = tokens.get(open)?.depth + 1;
    tokens[open + 1..close]
        .iter()
        .enumerate()
        .find_map(|(offset, token)| {
            (token.depth == depth && token.token.kind == TokenKind::Select)
                .then_some(open + 1 + offset)
        })
}

fn values_in_parentheses(tokens: &[DepthToken], open: usize, close: usize) -> Option<usize> {
    let depth = tokens.get(open)?.depth + 1;
    tokens[open + 1..close]
        .iter()
        .enumerate()
        .find_map(|(offset, token)| {
            (token.depth == depth && token.token.kind == TokenKind::Values)
                .then_some(open + 1 + offset)
        })
}

fn collect_values_row_shape(tokens: &[DepthToken], values: usize, limit: usize) -> RowShape {
    let depth = tokens[values].depth;
    let Some(open) = tokens
        .iter()
        .enumerate()
        .skip(values + 1)
        .take(limit.saturating_sub(values + 1))
        .find_map(|(index, token)| {
            (token.depth == depth && token.token.kind == TokenKind::Char('(')).then_some(index)
        })
    else {
        return RowShape::default();
    };
    let Some(close) = matching_paren(tokens, open) else {
        return RowShape::default();
    };
    let item_depth = depth + 1;
    let mut count = 0usize;
    let mut has_tokens = false;
    for token in &tokens[open + 1..close] {
        if token.depth == item_depth && token.token.kind == TokenKind::Char(',') {
            count += usize::from(has_tokens);
            has_tokens = false;
        } else {
            has_tokens = true;
        }
    }
    count += usize::from(has_tokens);
    RowShape {
        sources: Vec::new(),
        items: (1..=count)
            .map(|index| RowShapeItem::Column {
                name: format!("column{index}"),
                origin: RowColumnOrigin::Expression,
            })
            .collect(),
    }
}

fn find_from(tokens: &[DepthToken], select: usize) -> Option<usize> {
    let depth = tokens[select].depth;
    for (index, token) in tokens.iter().enumerate().skip(select + 1) {
        if token.depth < depth {
            return None;
        }
        if token.depth != depth {
            continue;
        }
        if token.token.kind == TokenKind::From {
            if index > select + 1
                && tokens[index - 1].depth == depth
                && tokens[index - 1].token.kind == TokenKind::Distinct
            {
                continue;
            }
            return Some(index);
        }
        if is_query_boundary(token.token.kind) {
            return None;
        }
    }
    None
}

fn cursor_from_subquery(
    tokens: &[DepthToken],
    select: usize,
    cursor: usize,
) -> Option<(bool, usize)> {
    let from = find_from(tokens, select)?;
    let depth = tokens[select].depth;
    for (index, token) in tokens.iter().enumerate().skip(from + 1) {
        if token.depth < depth || (token.depth == depth && is_from_terminator(token.token.kind)) {
            break;
        }
        if token.depth != depth || token.token.kind != TokenKind::Char('(') {
            continue;
        }
        let close = matching_paren(tokens, index)?;
        if token.token.location() < cursor && cursor <= tokens[close].token.end_location() {
            let lateral = index > 0 && tokens[index - 1].token.kind == TokenKind::LateralP;
            return Some((lateral, token.token.location()));
        }
    }
    None
}

fn cursor_is_in_from_clause(tokens: &[DepthToken], select: usize, cursor: usize) -> bool {
    let Some(from) = find_from(tokens, select) else {
        return false;
    };
    if tokens[from].token.end_location() > cursor {
        return false;
    }
    let depth = tokens[select].depth;
    let end = tokens
        .iter()
        .skip(from + 1)
        .find(|token| {
            token.depth < depth || (token.depth == depth && is_from_terminator(token.token.kind))
        })
        .map_or(usize::MAX, |token| token.token.location());
    cursor <= end
}

fn collect_select_row_shape(
    tokens: &[DepthToken],
    select: usize,
    ranges: &[RangeBindingId],
    graph: &BindingGraph,
) -> RowShape {
    let depth = tokens[select].depth;
    let mut start = select + 1;
    if tokens.get(start).is_some_and(|token| {
        token.depth == depth && matches!(token.token.kind, TokenKind::All | TokenKind::Distinct)
    }) {
        start += 1;
        if tokens
            .get(start)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::On)
            && tokens.get(start + 1).is_some_and(|token| {
                token.depth == depth && token.token.kind == TokenKind::Char('(')
            })
        {
            start = matching_paren(tokens, start + 1).map_or(start + 1, |close| close + 1);
        }
    }
    let end = tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, token)| {
            (token.depth < depth
                || (token.depth == depth
                    && matches!(
                        token.token.kind,
                        TokenKind::From
                            | TokenKind::Into
                            | TokenKind::Where
                            | TokenKind::GroupP
                            | TokenKind::Having
                            | TokenKind::Window
                            | TokenKind::Order
                            | TokenKind::Limit
                            | TokenKind::Offset
                            | TokenKind::Fetch
                            | TokenKind::For
                            | TokenKind::Union
                            | TokenKind::Intersect
                            | TokenKind::Except
                            | TokenKind::Returning
                    )))
            .then_some(index)
        })
        .unwrap_or(tokens.len());

    let mut items = Vec::new();
    let mut item_start = start;
    for index in start..=end {
        let at_end = index == end;
        let at_comma = !at_end
            && tokens[index].depth == depth
            && tokens[index].token.kind == TokenKind::Char(',');
        if at_end || at_comma {
            if item_start < index
                && let Some(item) = row_shape_item(tokens, item_start, index, depth, ranges, graph)
            {
                items.push(item);
            }
            item_start = index + 1;
        }
    }
    RowShape {
        sources: ranges.to_vec(),
        items,
    }
}

fn row_shape_item(
    tokens: &[DepthToken],
    start: usize,
    end: usize,
    depth: usize,
    ranges: &[RangeBindingId],
    graph: &BindingGraph,
) -> Option<RowShapeItem> {
    let (expression_end, alias) = select_item_alias(tokens, start, end, depth);
    let expression = &tokens[start..expression_end];
    if expression.is_empty() {
        return None;
    }

    if expression.len() == 1 && expression[0].token.kind == TokenKind::Char('*') {
        return Some(RowShapeItem::Wildcard { binding: None });
    }
    if let Some(qualifier) = qualified_star(expression, depth) {
        let binding = ranges.iter().copied().find(|id| {
            graph.ranges[id.0]
                .exposed_name()
                .eq_ignore_ascii_case(&qualifier)
        });
        return Some(RowShapeItem::Wildcard { binding });
    }
    if let Some(parts) = simple_name_parts(expression, depth).or_else(|| {
        expression
            .iter()
            .position(|token| {
                token.depth == depth
                    && matches!(token.token.kind, TokenKind::TypeCast | TokenKind::Collate)
            })
            .and_then(|end| simple_name_parts(&expression[..end], depth))
    }) {
        let column = parts.last()?.clone();
        let binding = (parts.len() > 1)
            .then(|| {
                let qualifier = &parts[parts.len() - 2];
                ranges.iter().copied().find(|id| {
                    graph.ranges[id.0]
                        .exposed_name()
                        .eq_ignore_ascii_case(qualifier)
                })
            })
            .flatten();
        return Some(RowShapeItem::Column {
            name: alias.unwrap_or_else(|| column.clone()),
            origin: RowColumnOrigin::Column {
                binding,
                name: column,
            },
        });
    }

    let inferred = alias.or_else(|| function_output_name(expression, depth));
    inferred.map(|name| RowShapeItem::Column {
        name,
        origin: RowColumnOrigin::Expression,
    })
}

fn select_item_alias(
    tokens: &[DepthToken],
    start: usize,
    end: usize,
    depth: usize,
) -> (usize, Option<String>) {
    if let Some(as_index) = (start..end)
        .rev()
        .find(|index| tokens[*index].depth == depth && tokens[*index].token.kind == TokenKind::As)
    {
        return (
            as_index,
            token_name(tokens.get(as_index + 1).map(|token| &token.token)),
        );
    }
    if end > start + 1
        && tokens[end - 1].depth == depth
        && is_alias_token(&tokens[end - 1].token)
        && tokens[end - 2].token.kind != TokenKind::Char('.')
        && can_end_select_expression(tokens[end - 2].token.kind)
    {
        return (
            end - 1,
            token_name(tokens.get(end - 1).map(|token| &token.token)),
        );
    }
    (end, None)
}

fn can_end_select_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::UIdent
            | TokenKind::FConst
            | TokenKind::SConst
            | TokenKind::USConst
            | TokenKind::BConst
            | TokenKind::XConst
            | TokenKind::IConst
            | TokenKind::Param
            | TokenKind::Char(')')
            | TokenKind::Char(']')
            | TokenKind::NullP
            | TokenKind::TrueP
            | TokenKind::FalseP
            | TokenKind::EndP
            | TokenKind::SystemUser
            | TokenKind::CurrentDate
            | TokenKind::CurrentTime
            | TokenKind::CurrentTimestamp
            | TokenKind::Localtime
            | TokenKind::Localtimestamp
            | TokenKind::CurrentRole
            | TokenKind::CurrentUser
            | TokenKind::User
            | TokenKind::SessionUser
            | TokenKind::CurrentCatalog
            | TokenKind::CurrentSchema
    )
}

fn simple_name_parts(tokens: &[DepthToken], depth: usize) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut expect_name = true;
    for token in tokens {
        if token.depth != depth {
            return None;
        }
        if expect_name {
            parts.push(token_name(Some(&token.token))?);
        } else if token.token.kind != TokenKind::Char('.') {
            return None;
        }
        expect_name = !expect_name;
    }
    (!parts.is_empty() && !expect_name).then_some(parts)
}

fn qualified_star(tokens: &[DepthToken], depth: usize) -> Option<String> {
    if tokens.len() < 3
        || tokens.last()?.depth != depth
        || tokens.last()?.token.kind != TokenKind::Char('*')
        || tokens[tokens.len() - 2].token.kind != TokenKind::Char('.')
    {
        return None;
    }
    token_name(Some(&tokens[tokens.len() - 3].token))
}

fn function_output_name(tokens: &[DepthToken], depth: usize) -> Option<String> {
    tokens.windows(2).find_map(|pair| {
        (pair[0].depth == depth && pair[1].token.kind == TokenKind::Char('('))
            .then(|| token_name(Some(&pair[0].token)))
            .flatten()
    })
}

fn parse_qualified_name(
    tokens: &[DepthToken],
    mut index: usize,
    depth: usize,
) -> (Vec<String>, usize) {
    let mut parts = Vec::new();
    while let Some(name) = token_name(tokens.get(index).map(|token| &token.token)) {
        if tokens[index].depth != depth {
            break;
        }
        parts.push(name);
        index += 1;
        if !tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char('.'))
        {
            break;
        }
        index += 1;
    }
    (parts, index)
}

fn parse_alias(
    tokens: &[DepthToken],
    mut index: usize,
    depth: usize,
    typed_columns: bool,
) -> (Option<String>, Vec<String>, usize) {
    if tokens
        .get(index)
        .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::As)
    {
        index += 1;
    }
    let alias = tokens
        .get(index)
        .filter(|token| token.depth == depth && is_alias_token(&token.token))
        .and_then(|token| token_name(Some(&token.token)));
    if alias.is_none() {
        return (None, Vec::new(), index);
    }
    index += 1;
    let mut columns = Vec::new();
    if tokens
        .get(index)
        .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char('('))
        && let Some(close) = matching_paren(tokens, index)
    {
        if typed_columns {
            let column_depth = depth + 1;
            let mut expect_column = true;
            for token in &tokens[index + 1..close] {
                if token.depth == column_depth && expect_column {
                    if let Some(name) = token_name(Some(&token.token)) {
                        columns.push(name);
                        expect_column = false;
                    }
                } else if token.depth == column_depth && token.token.kind == TokenKind::Char(',') {
                    expect_column = true;
                }
            }
        } else {
            for token in &tokens[index + 1..close] {
                if let Some(name) = token_name(Some(&token.token)) {
                    columns.push(name);
                }
            }
        }
        index = close + 1;
    }
    (alias, columns, index)
}

fn binding_end(tokens: &[DepthToken], fallback: usize, index: usize) -> usize {
    tokens
        .get(index.saturating_sub(1))
        .map_or(fallback, |token| token.token.end_location())
}

fn collect_target_relation(tokens: &[DepthToken]) -> Option<TargetRelation> {
    let first_token = tokens.first()?;
    let (statement, first) = if first_token.token.kind == TokenKind::With {
        top_level_dml_statement(tokens)?
    } else {
        (0, first_token.token.kind)
    };
    let start = match first {
        TokenKind::Insert => find_top_level_token_after(tokens, statement, TokenKind::Into)? + 1,
        TokenKind::Update => statement + 1,
        TokenKind::DeleteP => find_top_level_token_after(tokens, statement, TokenKind::From)? + 1,
        TokenKind::Merge => find_top_level_token_after(tokens, statement, TokenKind::Into)? + 1,
        TokenKind::Alter
            if tokens
                .get(1)
                .is_some_and(|token| token.token.kind == TokenKind::Table) =>
        {
            2
        }
        TokenKind::Create
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Index) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::On)
                .map(|index| index + 1)?
        }
        TokenKind::Create
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Policy) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::On)
                .map(|index| index + 1)?
        }
        TokenKind::Alter
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Policy) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::On)
                .map(|index| index + 1)?
        }
        TokenKind::Create
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Rule) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::To)
                .map(|index| index + 1)?
        }
        TokenKind::Create
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Trigger) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::On)
                .map(|index| index + 1)?
        }
        TokenKind::Create
            if tokens
                .iter()
                .any(|token| token.depth == 0 && token.token.kind == TokenKind::Publication) =>
        {
            tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == TokenKind::Table)
                .map(|index| index + 1)?
        }
        TokenKind::Copy => 1,
        _ => return None,
    };
    let (parts, next) = parse_qualified_name(tokens, start, 0);
    if parts.is_empty() {
        return None;
    }
    let (alias, _, _) = parse_alias(tokens, next, 0, false);
    Some(TargetRelation {
        name: qualified_name(parts),
        alias,
    })
}

fn top_level_dml_statement(tokens: &[DepthToken]) -> Option<(usize, TokenKind)> {
    tokens.iter().enumerate().find_map(|(index, token)| {
        (token.depth == 0
            && matches!(
                token.token.kind,
                TokenKind::Insert | TokenKind::Update | TokenKind::DeleteP | TokenKind::Merge
            ))
        .then_some((index, token.token.kind))
    })
}

fn find_top_level_token_after(
    tokens: &[DepthToken],
    start: usize,
    kind: TokenKind,
) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, token)| (token.depth == 0 && token.token.kind == kind).then_some(index))
}

fn token_name(token: Option<&Token>) -> Option<String> {
    let token = token?;
    match &token.value {
        Some(TokenValue::String(value)) => Some(value.clone()),
        Some(TokenValue::Keyword(word)) => {
            let keyword = lookup_keyword(word)?;
            (keyword.category != KeywordCategory::Reserved).then(|| (*word).to_owned())
        }
        _ => None,
    }
}

fn is_alias_token(token: &Token) -> bool {
    token_name(Some(token)).is_some()
        && !matches!(
            token.kind,
            TokenKind::Where
                | TokenKind::GroupP
                | TokenKind::Having
                | TokenKind::Window
                | TokenKind::Order
                | TokenKind::Limit
                | TokenKind::Offset
                | TokenKind::Fetch
                | TokenKind::For
                | TokenKind::Union
                | TokenKind::Intersect
                | TokenKind::Except
                | TokenKind::Join
                | TokenKind::InnerP
                | TokenKind::Left
                | TokenKind::Right
                | TokenKind::Full
                | TokenKind::Cross
                | TokenKind::Natural
                | TokenKind::On
                | TokenKind::Using
                | TokenKind::Tablesample
                | TokenKind::Repeatable
                | TokenKind::With
        )
}

fn matching_paren(tokens: &[DepthToken], open: usize) -> Option<usize> {
    let depth = tokens.get(open)?.depth;
    tokens
        .iter()
        .enumerate()
        .skip(open + 1)
        .find_map(|(index, token)| {
            (token.token.kind == TokenKind::Char(')') && token.depth == depth + 1).then_some(index)
        })
}

fn skip_join_condition(tokens: &[DepthToken], mut index: usize, depth: usize) -> usize {
    while index < tokens.len() {
        let token = &tokens[index];
        if token.depth == depth
            && (token.token.kind == TokenKind::Char(',')
                || is_join_start(token.token.kind)
                || is_from_terminator(token.token.kind))
        {
            break;
        }
        index += 1;
    }
    index
}

fn qualified_name(parts: Vec<String>) -> QualifiedName {
    match parts.as_slice() {
        [name] => QualifiedName {
            name: name.clone(),
            ..QualifiedName::default()
        },
        [schema, name] => QualifiedName {
            schema: Some(schema.clone()),
            name: name.clone(),
            ..QualifiedName::default()
        },
        _ => QualifiedName {
            catalog: parts.get(parts.len().saturating_sub(3)).cloned(),
            schema: parts.get(parts.len().saturating_sub(2)).cloned(),
            name: parts.last().cloned().unwrap_or_default(),
        },
    }
}

fn is_join_noise(kind: TokenKind) -> bool {
    kind == TokenKind::Char(',') || is_join_start(kind) || kind == TokenKind::OuterP
}

fn is_join_start(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Join
            | TokenKind::InnerP
            | TokenKind::Left
            | TokenKind::Right
            | TokenKind::Full
            | TokenKind::Cross
            | TokenKind::Natural
    )
}

fn is_from_terminator(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Where
            | TokenKind::GroupP
            | TokenKind::Having
            | TokenKind::Window
            | TokenKind::Order
            | TokenKind::Limit
            | TokenKind::Offset
            | TokenKind::Fetch
            | TokenKind::For
            | TokenKind::Union
            | TokenKind::Intersect
            | TokenKind::Except
            | TokenKind::Returning
    )
}

fn is_query_boundary(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Union | TokenKind::Intersect | TokenKind::Except | TokenKind::Char(')')
    )
}
