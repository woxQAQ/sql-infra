use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};

use crate::{
    KEYWORDS, KeywordCategory, TextRange, TextSize, Token, TokenKind, TokenValue, lex,
    lookup_keyword,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionContext {
    pub replacement: TextRange,
    pub prefix: String,
    pub statement: TextRange,
    pub expectations: Vec<Expectation>,
    pub scope: ScopeSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Expectation {
    Token(TokenKind),
    Name(NameExpectation),
    Expression,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NameExpectation {
    Schema,
    Relation { schema: Option<String> },
    Column(ColumnContext),
    Function { schema: Option<String> },
    Type { schema: Option<String> },
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ColumnContext {
    VisibleScope,
    Qualified(String),
    JoinUsing,
    TargetRelation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeSnapshot {
    pub local_references: Vec<RangeReference>,
    pub outer_references: Vec<Vec<RangeReference>>,
    pub references: Vec<RangeReference>,
    pub ctes: Vec<RangeReference>,
    pub target_relation: Option<QualifiedName>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeReferenceKind {
    Relation,
    Cte,
    Subquery,
    Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeReference {
    pub kind: RangeReferenceKind,
    pub name: QualifiedName,
    pub alias: Option<String>,
    pub alias_columns: Vec<String>,
    pub range: TextRange,
    pub lateral: bool,
}

impl RangeReference {
    pub fn exposed_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name.name)
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct QualifiedName {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionError {
    CursorOutOfBounds { cursor: TextSize, len: TextSize },
    CursorNotCharBoundary { cursor: TextSize },
    SourceTooLarge,
}

#[derive(Default)]
pub(crate) struct CompletionRecorder {
    cursor: Option<usize>,
    expectations: Vec<Expectation>,
}

pub(crate) type SharedCompletionRecorder = Rc<RefCell<CompletionRecorder>>;

impl CompletionRecorder {
    pub(crate) fn set_cursor(&mut self, cursor: usize) {
        match self.cursor {
            Some(existing) => debug_assert_eq!(existing, cursor),
            None => self.cursor = Some(cursor),
        }
    }

    pub(crate) fn is_cursor(&self, location: usize) -> bool {
        self.cursor == Some(location)
    }

    pub(crate) fn record(&mut self, expectation: Expectation) {
        if !self.expectations.contains(&expectation) {
            self.expectations.push(expectation);
        }
    }
}

impl std::fmt::Display for CompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CursorOutOfBounds { cursor, len } => write!(
                f,
                "completion cursor {} is beyond source length {}",
                cursor.get(),
                len.get()
            ),
            Self::CursorNotCharBoundary { cursor } => write!(
                f,
                "completion cursor {} is not a UTF-8 character boundary",
                cursor.get()
            ),
            Self::SourceTooLarge => write!(f, "SQL source exceeds the supported size"),
        }
    }
}

impl std::error::Error for CompletionError {}

/// Collect PostgreSQL syntax facts needed by a completion engine.
///
/// Ordinary parsing remains strict. This interface deliberately returns
/// completion-only state rather than a partial raw parse tree.
pub fn collect_completion(
    sql: &str,
    cursor: TextSize,
) -> Result<CompletionContext, CompletionError> {
    let len = TextSize::try_from(sql.len()).map_err(|_| CompletionError::SourceTooLarge)?;
    if cursor > len {
        return Err(CompletionError::CursorOutOfBounds { cursor, len });
    }
    let cursor_usize = usize::from(cursor);
    if !sql.is_char_boundary(cursor_usize) {
        return Err(CompletionError::CursorNotCharBoundary { cursor });
    }

    let tokens = completion_tokens(sql, cursor_usize);
    let replacement = replacement_range(sql, cursor_usize, &tokens);
    let replacement_start = usize::from(replacement.start());
    let prefix = sql
        .get(replacement_start..cursor_usize)
        .unwrap_or_default()
        .trim_start_matches('"')
        .replace("\"\"", "\"");
    let (statement, statement_tokens) = statement_at(sql, cursor_usize, &tokens);
    let scope = collect_scope(&statement_tokens, cursor_usize);
    let expectations = if suppress_completion_at_cursor(sql, cursor_usize) {
        Vec::new()
    } else {
        let mut expectations = collect_parser_expectations(&statement_tokens, replacement_start);
        for fallback in collect_tricky_expectations(&statement_tokens, replacement_start, &scope) {
            if !expectations.contains(&fallback) {
                expectations.push(fallback);
            }
        }
        expectations
    };

    Ok(CompletionContext {
        replacement,
        prefix,
        statement,
        expectations,
        scope,
    })
}

fn collect_parser_expectations(tokens: &[Token], offset: usize) -> Vec<Expectation> {
    let mut prefix: Vec<Token> = tokens
        .iter()
        .filter(|token| token.location() < offset)
        .cloned()
        .collect();
    prefix.push(Token::synthetic(TokenKind::Eof, offset));
    let recorder = Rc::new(RefCell::new(CompletionRecorder::default()));
    let mut parser = crate::parser::Parser::for_completion(prefix, recorder.clone());
    let _ = parser.parse_completion_statement();
    Rc::try_unwrap(recorder)
        .map(|recorder| recorder.into_inner().expectations)
        .unwrap_or_else(|recorder| recorder.borrow().expectations.clone())
}

pub fn keyword_text(kind: TokenKind) -> Option<&'static str> {
    KEYWORDS
        .iter()
        .find(|keyword| keyword.kind == kind)
        .map(|keyword| keyword.word)
}

fn completion_tokens(sql: &str, cursor: usize) -> Vec<Token> {
    match lex(sql) {
        Ok(tokens) => tokens,
        Err(_) => {
            let start = manual_replacement_start(sql, cursor);
            let mut tokens = lex(&sql[..start]).unwrap_or_default();
            tokens.retain(|token| token.kind != TokenKind::Eof);
            tokens.push(Token::synthetic(TokenKind::Eof, start));
            tokens
        }
    }
}

fn replacement_range(sql: &str, cursor: usize, tokens: &[Token]) -> TextRange {
    for token in tokens {
        let start = token.location();
        let end = token.end_location();
        if is_replaceable(token)
            && ((start <= cursor && cursor < end) || (end == cursor && start < cursor))
        {
            return token.range;
        }
    }
    let start = manual_replacement_start(sql, cursor);
    let end = manual_replacement_end(sql, cursor);
    TextRange::new(text_size(start), text_size(end))
}

fn is_replaceable(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Ident | TokenKind::UIdent)
        || matches!(token.value, Some(TokenValue::Keyword(_)))
}

fn manual_replacement_start(sql: &str, cursor: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut start = cursor;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start > 0 && bytes[start - 1] == b'"' {
        start - 1
    } else {
        start
    }
}

fn manual_replacement_end(sql: &str, cursor: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut end = cursor;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'"' {
        end + 1
    } else {
        end
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn statement_at(sql: &str, cursor: usize, tokens: &[Token]) -> (TextRange, Vec<Token>) {
    let mut start = 0usize;
    let mut end = sql.len();
    for token in tokens {
        if token.kind == TokenKind::Char(';') {
            if token.end_location() <= cursor {
                start = token.end_location();
            } else if token.location() >= cursor {
                end = token.location();
                break;
            }
        }
    }
    let statement_tokens = tokens
        .iter()
        .filter(|token| {
            token.kind != TokenKind::Eof && token.location() >= start && token.location() < end
        })
        .cloned()
        .collect();
    (
        TextRange::new(text_size(start), text_size(end)),
        statement_tokens,
    )
}

#[derive(Clone)]
struct DepthToken {
    token: Token,
    depth: usize,
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
            TokenKind::Char(')') | TokenKind::Char(']') => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    result
}

fn collect_scope(tokens: &[Token], cursor: usize) -> ScopeSnapshot {
    let tokens = with_depth(tokens);
    let cursor_depth = depth_at(&tokens, cursor);
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
    let innermost_select = selects.last().map(|(_, index)| *index);
    let ctes = collect_ctes(&tokens, innermost_select.unwrap_or(tokens.len()));
    let mut frames = Vec::new();
    for (_, select) in selects {
        let references = find_from(&tokens, select)
            .map(|from| parse_from_references(&tokens, from + 1, ctes.as_slice()))
            .unwrap_or_default();
        frames.push((select, references));
    }
    let local_references = frames
        .pop()
        .map(|(_, references)| references)
        .unwrap_or_default();
    let outer_references: Vec<Vec<RangeReference>> = frames
        .into_iter()
        .rev()
        .filter_map(|(select, mut references)| {
            if let Some((lateral, open_location)) = cursor_from_subquery(&tokens, select, cursor) {
                if !lateral {
                    references.clear();
                } else {
                    references
                        .retain(|reference| usize::from(reference.range.start()) < open_location);
                }
            }
            (!references.is_empty()).then_some(references)
        })
        .collect();
    let mut references = local_references.clone();
    for outer in &outer_references {
        references.extend(outer.iter().cloned());
    }
    let target_relation = collect_target_relation(&tokens);
    ScopeSnapshot {
        local_references,
        outer_references,
        references,
        ctes,
        target_relation,
    }
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

fn collect_ctes(tokens: &[DepthToken], before: usize) -> Vec<RangeReference> {
    let Some(with_index) = tokens[..before]
        .iter()
        .position(|token| token.depth == 0 && token.token.kind == TokenKind::With)
    else {
        return Vec::new();
    };
    let mut index = with_index + 1;
    if tokens
        .get(index)
        .is_some_and(|token| token.token.kind == TokenKind::Recursive)
    {
        index += 1;
    }
    let mut result = Vec::new();
    while index < before {
        let Some(name) = token_name(tokens.get(index).map(|token| &token.token)) else {
            break;
        };
        let start = tokens[index].token.location();
        index += 1;
        let mut alias_columns = Vec::new();
        if tokens
            .get(index)
            .is_some_and(|token| token.token.kind == TokenKind::Char('('))
        {
            let Some(close) = matching_paren(tokens, index) else {
                break;
            };
            for token in &tokens[index + 1..close] {
                if let Some(column) = token_name(Some(&token.token)) {
                    alias_columns.push(column);
                }
            }
            index = close + 1;
        }
        if tokens
            .get(index)
            .is_some_and(|token| token.token.kind == TokenKind::As)
        {
            index += 1;
        }
        if tokens
            .get(index)
            .is_some_and(|token| token.token.kind == TokenKind::Not)
        {
            index += 1;
        }
        if tokens
            .get(index)
            .is_some_and(|token| token.token.kind == TokenKind::Materialized)
        {
            index += 1;
        }
        let Some(open) = tokens
            .get(index)
            .filter(|token| token.token.kind == TokenKind::Char('('))
            .map(|_| index)
        else {
            break;
        };
        let Some(close) = matching_paren(tokens, open) else {
            break;
        };
        result.push(RangeReference {
            kind: RangeReferenceKind::Cte,
            name: QualifiedName {
                name,
                ..QualifiedName::default()
            },
            alias: None,
            alias_columns,
            range: TextRange::new(
                text_size(start),
                text_size(tokens[close].token.end_location()),
            ),
            lateral: false,
        });
        index = close + 1;
        if tokens
            .get(index)
            .is_some_and(|token| token.token.kind == TokenKind::Char(','))
        {
            index += 1;
            continue;
        }
        break;
    }
    result
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

fn parse_from_references(
    tokens: &[DepthToken],
    mut index: usize,
    ctes: &[RangeReference],
) -> Vec<RangeReference> {
    let depth = tokens.get(index).map_or(0, |token| token.depth);
    let mut result = Vec::new();
    while index < tokens.len() {
        let token = &tokens[index];
        if token.depth < depth || (token.depth == depth && is_from_terminator(token.token.kind)) {
            break;
        }
        if token.depth != depth || is_join_noise(token.token.kind) {
            index += 1;
            continue;
        }
        if matches!(token.token.kind, TokenKind::On | TokenKind::Using) {
            index = skip_join_condition(tokens, index + 1, depth);
            continue;
        }
        let lateral = token.token.kind == TokenKind::LateralP;
        if lateral {
            index += 1;
        }
        let Some(current) = tokens.get(index) else {
            break;
        };
        if current.token.kind == TokenKind::Char('(') {
            let Some(close) = matching_paren(tokens, index) else {
                break;
            };
            let (alias, alias_columns, next) = parse_alias(tokens, close + 1, depth);
            if alias.is_some() {
                result.push(RangeReference {
                    kind: RangeReferenceKind::Subquery,
                    name: QualifiedName {
                        name: alias.clone().unwrap_or_default(),
                        ..QualifiedName::default()
                    },
                    alias,
                    alias_columns,
                    range: TextRange::new(
                        current.token.range.start(),
                        text_size(tokens[next.saturating_sub(1)].token.end_location()),
                    ),
                    lateral,
                });
            }
            index = next;
            continue;
        }
        let start = current.token.location();
        let (parts, next) = parse_qualified_name(tokens, index, depth);
        if parts.is_empty() {
            index += 1;
            continue;
        }
        index = next;
        let is_function = tokens
            .get(index)
            .is_some_and(|token| token.depth == depth && token.token.kind == TokenKind::Char('('));
        if is_function && let Some(close) = matching_paren(tokens, index) {
            index = close + 1;
        }
        let (alias, alias_columns, next) = parse_alias(tokens, index, depth);
        index = next;
        let name = qualified_name(parts);
        let kind = if is_function {
            RangeReferenceKind::Function
        } else if ctes
            .iter()
            .any(|cte| cte.name.name.eq_ignore_ascii_case(&name.name))
        {
            RangeReferenceKind::Cte
        } else {
            RangeReferenceKind::Relation
        };
        let end = tokens
            .get(index.saturating_sub(1))
            .map_or(current.token.end_location(), |token| {
                token.token.end_location()
            });
        result.push(RangeReference {
            kind,
            name,
            alias,
            alias_columns,
            range: TextRange::new(text_size(start), text_size(end)),
            lateral,
        });
    }
    result
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
        for token in &tokens[index + 1..close] {
            if let Some(name) = token_name(Some(&token.token)) {
                columns.push(name);
            }
        }
        index = close + 1;
    }
    (alias, columns, index)
}

fn collect_target_relation(tokens: &[DepthToken]) -> Option<QualifiedName> {
    let first = tokens.first()?.token.kind;
    let start = match first {
        TokenKind::Insert => tokens
            .iter()
            .position(|token| token.depth == 0 && token.token.kind == TokenKind::Into)
            .map(|index| index + 1)?,
        TokenKind::Update => 1,
        TokenKind::DeleteP => tokens
            .iter()
            .position(|token| token.depth == 0 && token.token.kind == TokenKind::From)
            .map(|index| index + 1)?,
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
        _ => return None,
    };
    let (parts, _) = parse_qualified_name(tokens, start, 0);
    (!parts.is_empty()).then(|| qualified_name(parts))
}

/// Enrich parser-produced candidates for cursor shapes that cannot be
/// represented by the strict token stream, such as a partial identifier after
/// a qualifier. Grammar alternatives still come from the recursive-descent
/// productions through `collect_parser_expectations`.
fn collect_tricky_expectations(
    tokens: &[Token],
    offset: usize,
    scope: &ScopeSnapshot,
) -> Vec<Expectation> {
    let before: Vec<&Token> = tokens
        .iter()
        .filter(|token| token.location() < offset)
        .collect();
    let mut result = Vec::new();
    let Some(last) = before.last().copied() else {
        add_statement_starters(&mut result);
        return result;
    };

    if last.kind == TokenKind::Char('.') {
        if let Some(qualifier) = before
            .get(before.len().saturating_sub(2))
            .and_then(|token| token_name(Some(token)))
        {
            if qualifier_is_relation_schema(&before) {
                push_unique(
                    &mut result,
                    Expectation::Name(NameExpectation::Relation {
                        schema: Some(qualifier),
                    }),
                );
            } else {
                push_unique(
                    &mut result,
                    Expectation::Name(NameExpectation::Column(ColumnContext::Qualified(
                        qualifier.clone(),
                    ))),
                );
                push_unique(
                    &mut result,
                    Expectation::Name(NameExpectation::Function {
                        schema: Some(qualifier),
                    }),
                );
            }
        }
        return result;
    }

    if expects_type_name(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Type { schema: None }),
        );
        return result;
    }

    if inside_join_using(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::JoinUsing)),
        );
        push_unique(&mut result, Expectation::Token(TokenKind::Char(')')));
        return result;
    }

    if inside_insert_columns(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
        );
        push_unique(&mut result, Expectation::Token(TokenKind::Char(')')));
        return result;
    }

    if inside_create_index_columns(&before) || after_alter_table_column_keyword(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
        );
        if inside_create_index_columns(&before) {
            push_unique(&mut result, Expectation::Token(TokenKind::Char(')')));
        }
        return result;
    }

    if expects_existing_relation(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Relation { schema: None }),
        );
        push_unique(&mut result, Expectation::Name(NameExpectation::Schema));
        return result;
    }

    match last.kind {
        TokenKind::Select | TokenKind::Where | TokenKind::Having | TokenKind::On => {
            add_expression_expectations(&mut result);
        }
        TokenKind::From | TokenKind::Join | TokenKind::Into | TokenKind::Update => {
            push_unique(
                &mut result,
                Expectation::Name(NameExpectation::Relation { schema: None }),
            );
            push_unique(&mut result, Expectation::Name(NameExpectation::Schema));
            if matches!(last.kind, TokenKind::From | TokenKind::Join) {
                push_unique(
                    &mut result,
                    Expectation::Name(NameExpectation::Function { schema: None }),
                );
                push_unique(&mut result, Expectation::Token(TokenKind::LateralP));
            }
        }
        TokenKind::Set if scope.target_relation.is_some() => {
            push_unique(
                &mut result,
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            );
        }
        TokenKind::By | TokenKind::Returning | TokenKind::Set => {
            add_expression_expectations(&mut result);
        }
        TokenKind::Create => add_tokens(
            &mut result,
            &[
                TokenKind::Table,
                TokenKind::View,
                TokenKind::Index,
                TokenKind::Schema,
                TokenKind::Function,
                TokenKind::TypeP,
            ],
        ),
        TokenKind::Alter => add_tokens(
            &mut result,
            &[
                TokenKind::Table,
                TokenKind::Schema,
                TokenKind::Database,
                TokenKind::Role,
                TokenKind::TypeP,
            ],
        ),
        TokenKind::Drop => add_tokens(
            &mut result,
            &[
                TokenKind::Table,
                TokenKind::View,
                TokenKind::Index,
                TokenKind::Schema,
                TokenKind::Function,
                TokenKind::TypeP,
            ],
        ),
        _ => {
            if current_clause(&before).is_some() {
                add_expression_tail_expectations(&mut result);
            } else if scope.references.is_empty() {
                add_statement_starters(&mut result);
            }
        }
    }
    result
}

fn add_statement_starters(result: &mut Vec<Expectation>) {
    add_tokens(
        result,
        &[
            TokenKind::Select,
            TokenKind::With,
            TokenKind::Insert,
            TokenKind::Update,
            TokenKind::DeleteP,
            TokenKind::Create,
            TokenKind::Alter,
            TokenKind::Drop,
            TokenKind::Values,
            TokenKind::Explain,
        ],
    );
}

fn add_expression_expectations(result: &mut Vec<Expectation>) {
    push_unique(result, Expectation::Expression);
    push_unique(
        result,
        Expectation::Name(NameExpectation::Column(ColumnContext::VisibleScope)),
    );
    push_unique(
        result,
        Expectation::Name(NameExpectation::Function { schema: None }),
    );
    add_tokens(
        result,
        &[
            TokenKind::NullP,
            TokenKind::TrueP,
            TokenKind::FalseP,
            TokenKind::Case,
            TokenKind::Cast,
        ],
    );
}

fn add_expression_tail_expectations(result: &mut Vec<Expectation>) {
    add_tokens(
        result,
        &[
            TokenKind::And,
            TokenKind::Or,
            TokenKind::From,
            TokenKind::Where,
            TokenKind::GroupP,
            TokenKind::Having,
            TokenKind::Order,
            TokenKind::Limit,
            TokenKind::Offset,
            TokenKind::Returning,
        ],
    );
}

fn add_tokens(result: &mut Vec<Expectation>, tokens: &[TokenKind]) {
    for token in tokens {
        push_unique(result, Expectation::Token(*token));
    }
}

fn push_unique(result: &mut Vec<Expectation>, expectation: Expectation) {
    if !result.contains(&expectation) {
        result.push(expectation);
    }
}

fn inside_join_using(tokens: &[&Token]) -> bool {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().rev() {
        match token.kind {
            TokenKind::Char(')') => depth += 1,
            TokenKind::Char('(') if depth > 0 => depth -= 1,
            TokenKind::Char('(') => {
                return index > 0 && tokens[index - 1].kind == TokenKind::Using;
            }
            _ => {}
        }
    }
    false
}

fn inside_insert_columns(tokens: &[&Token]) -> bool {
    if tokens.first().map(|token| token.kind) != Some(TokenKind::Insert) {
        return false;
    }
    let mut depth = 0usize;
    for token in tokens.iter().rev() {
        match token.kind {
            TokenKind::Char(')') => depth += 1,
            TokenKind::Char('(') if depth > 0 => depth -= 1,
            TokenKind::Char('(') => return true,
            TokenKind::Values | TokenKind::Select => return false,
            _ => {}
        }
    }
    false
}

fn inside_create_index_columns(tokens: &[&Token]) -> bool {
    if tokens.first().map(|token| token.kind) != Some(TokenKind::Create)
        || !tokens.iter().any(|token| token.kind == TokenKind::Index)
        || !tokens.iter().any(|token| token.kind == TokenKind::On)
    {
        return false;
    }
    let mut depth = 0usize;
    for token in tokens.iter().rev() {
        match token.kind {
            TokenKind::Char(')') => depth += 1,
            TokenKind::Char('(') if depth > 0 => depth -= 1,
            TokenKind::Char('(') => return true,
            _ => {}
        }
    }
    false
}

fn after_alter_table_column_keyword(tokens: &[&Token]) -> bool {
    tokens.first().map(|token| token.kind) == Some(TokenKind::Alter)
        && tokens.get(1).map(|token| token.kind) == Some(TokenKind::Table)
        && matches!(
            tokens.last().map(|token| token.kind),
            Some(TokenKind::Column | TokenKind::Rename)
        )
}

fn expects_existing_relation(tokens: &[&Token]) -> bool {
    matches!(
        tokens,
        [.., alter, table]
            if alter.kind == TokenKind::Alter && table.kind == TokenKind::Table
    ) || matches!(
        tokens,
        [.., drop, object]
            if drop.kind == TokenKind::Drop
                && matches!(object.kind, TokenKind::Table | TokenKind::View | TokenKind::Index)
    )
}

fn qualifier_is_relation_schema(tokens: &[&Token]) -> bool {
    let qualifier_index = tokens.len().saturating_sub(2);
    let Some(previous) = qualifier_index
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
    else {
        return false;
    };
    matches!(
        previous.kind,
        TokenKind::From | TokenKind::Join | TokenKind::Into | TokenKind::Update
    ) || (previous.kind == TokenKind::Table
        && qualifier_index >= 2
        && matches!(
            tokens[qualifier_index - 2].kind,
            TokenKind::Alter | TokenKind::Drop
        ))
}

fn expects_type_name(tokens: &[&Token]) -> bool {
    if tokens
        .last()
        .is_some_and(|token| token.kind == TokenKind::TypeCast)
    {
        return true;
    }
    if tokens
        .last()
        .is_some_and(|token| token.kind == TokenKind::TypeP)
        && tokens
            .first()
            .is_some_and(|token| token.kind == TokenKind::Alter)
    {
        return true;
    }
    if !tokens
        .last()
        .is_some_and(|token| token.kind == TokenKind::As)
    {
        return false;
    }
    let mut depth = 0usize;
    for token in tokens.iter().rev().skip(1) {
        match token.kind {
            TokenKind::Char(')') => depth += 1,
            TokenKind::Char('(') if depth > 0 => depth -= 1,
            TokenKind::Char('(') => {
                return tokens
                    .iter()
                    .take_while(|candidate| candidate.location() < token.location())
                    .last()
                    .is_some_and(|candidate| candidate.kind == TokenKind::Cast);
            }
            _ => {}
        }
    }
    false
}

fn current_clause(tokens: &[&Token]) -> Option<TokenKind> {
    tokens.iter().rev().find_map(|token| {
        matches!(
            token.kind,
            TokenKind::Select
                | TokenKind::Where
                | TokenKind::Having
                | TokenKind::On
                | TokenKind::Returning
                | TokenKind::Set
        )
        .then_some(token.kind)
    })
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
        TokenKind::Union | TokenKind::Intersect | TokenKind::Except | TokenKind::Char(';')
    )
}

fn text_size(offset: usize) -> TextSize {
    TextSize::try_from(offset).expect("completion offsets come from validated input")
}

fn suppress_completion_at_cursor(sql: &str, cursor: usize) -> bool {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Normal,
        SingleQuoted,
        DoubleQuoted,
        LineComment,
        BlockComment(usize),
    }

    let bytes = sql.as_bytes();
    let mut state = State::Normal;
    let mut index = 0usize;
    while index < cursor {
        state = match state {
            State::Normal if bytes.get(index..index + 2) == Some(b"--") => {
                index += 2;
                State::LineComment
            }
            State::Normal if bytes.get(index..index + 2) == Some(b"/*") => {
                index += 2;
                State::BlockComment(1)
            }
            State::Normal if bytes[index] == b'\'' => {
                index += 1;
                State::SingleQuoted
            }
            State::Normal if bytes[index] == b'"' => {
                index += 1;
                State::DoubleQuoted
            }
            State::LineComment if bytes[index] == b'\n' => {
                index += 1;
                State::Normal
            }
            State::LineComment => {
                index += 1;
                State::LineComment
            }
            State::BlockComment(depth) if bytes.get(index..index + 2) == Some(b"/*") => {
                index += 2;
                State::BlockComment(depth + 1)
            }
            State::BlockComment(depth) if bytes.get(index..index + 2) == Some(b"*/") => {
                index += 2;
                if depth == 1 {
                    State::Normal
                } else {
                    State::BlockComment(depth - 1)
                }
            }
            State::BlockComment(depth) => {
                index += 1;
                State::BlockComment(depth)
            }
            State::SingleQuoted
                if bytes[index] == b'\'' && bytes.get(index + 1) == Some(&b'\'') =>
            {
                index += 2;
                State::SingleQuoted
            }
            State::SingleQuoted if bytes[index] == b'\'' => {
                index += 1;
                State::Normal
            }
            State::SingleQuoted => {
                index += 1;
                State::SingleQuoted
            }
            State::DoubleQuoted if bytes[index] == b'"' && bytes.get(index + 1) == Some(&b'"') => {
                index += 2;
                State::DoubleQuoted
            }
            State::DoubleQuoted if bytes[index] == b'"' => {
                index += 1;
                State::Normal
            }
            State::DoubleQuoted => {
                index += 1;
                State::DoubleQuoted
            }
            State::Normal => {
                index += 1;
                State::Normal
            }
        };
    }
    matches!(
        state,
        State::SingleQuoted | State::LineComment | State::BlockComment(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(marked: &str) -> CompletionContext {
        let cursor = marked.find('|').expect("test input must contain a cursor");
        let sql = marked.replacen('|', "", 1);
        collect_completion(&sql, text_size(cursor)).unwrap()
    }

    fn collect_from_parser(marked: &str) -> Vec<Expectation> {
        let cursor = marked.find('|').expect("test input must contain a cursor");
        let sql = marked.replacen('|', "", 1);
        let tokens = completion_tokens(&sql, cursor);
        let (_, statement_tokens) = statement_at(&sql, cursor, &tokens);
        collect_parser_expectations(&statement_tokens, cursor)
    }

    #[test]
    fn statement_start_collects_keywords() {
        let context = collect("|");
        assert!(
            context
                .expectations
                .contains(&Expectation::Token(TokenKind::Select))
        );
        assert!(
            context
                .expectations
                .contains(&Expectation::Token(TokenKind::Insert))
        );
    }

    #[test]
    fn recursive_descent_productions_emit_core_expectations() {
        let cases = [
            ("|", Expectation::Token(TokenKind::Select)),
            ("SELECT |", Expectation::Expression),
            ("SELECT value + |", Expectation::Expression),
            ("SELECT count(|", Expectation::Expression),
            ("WITH x AS (SELECT |", Expectation::Expression),
            ("SELECT (SELECT |", Expectation::Expression),
            ("SELECT 1 IN (SELECT |", Expectation::Expression),
            ("SELECT 1 = ANY (SELECT |", Expectation::Expression),
            ("SELECT JSON_ARRAY(SELECT |", Expectation::Expression),
            (
                "CREATE RULE r AS ON UPDATE TO t DO (SELECT |",
                Expectation::Expression,
            ),
            (
                "SELECT sum(x) OVER (PARTITION BY |",
                Expectation::Expression,
            ),
            ("SELECT json_arrayagg(x ORDER BY |", Expectation::Expression),
            ("SELECT * FROM JSON_TABLE(|", Expectation::Expression),
            ("SELECT * FROM ROWS FROM (|", Expectation::Expression),
            ("SELECT * FROM XMLTABLE(|", Expectation::Expression),
            ("CREATE STATISTICS s ON |", Expectation::Expression),
            ("CALL |", Expectation::Expression),
            ("UPDATE t SET value[|", Expectation::Expression),
            (
                "SELECT * FROM |",
                Expectation::Name(NameExpectation::Relation { schema: None }),
            ),
            (
                "INSERT INTO |",
                Expectation::Name(NameExpectation::Relation { schema: None }),
            ),
            (
                "UPDATE users SET |",
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            ),
            ("CREATE |", Expectation::Token(TokenKind::Table)),
        ];
        for (marked, expected) in cases {
            let actual = collect_from_parser(marked);
            assert!(actual.contains(&expected), "{marked}: {actual:?}");
        }
    }

    #[test]
    fn select_target_has_visible_columns_and_functions() {
        let context = collect("SELECT | FROM users u");
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Column(
                    ColumnContext::VisibleScope
                )))
        );
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Function {
                    schema: None
                }))
        );
        assert_eq!(context.scope.references[0].name.name, "users");
        assert_eq!(context.scope.references[0].alias.as_deref(), Some("u"));
    }

    #[test]
    fn qualified_column_uses_alias() {
        let context = collect("SELECT u.na| FROM users u");
        assert_eq!(context.prefix, "na");
        assert_eq!(usize::from(context.replacement.start()), 9);
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Column(
                    ColumnContext::Qualified("u".into())
                )))
        );
    }

    #[test]
    fn from_collects_relations() {
        let context = collect("SELECT * FROM us|");
        assert_eq!(context.prefix, "us");
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Relation {
                    schema: None
                }))
        );
    }

    #[test]
    fn join_using_collects_columns_from_both_relations() {
        let context = collect("SELECT * FROM users u JOIN orders o USING (|");
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Column(
                    ColumnContext::JoinUsing
                )))
        );
        assert_eq!(context.scope.references.len(), 2);
    }

    #[test]
    fn insert_column_list_targets_insert_relation() {
        let context = collect("INSERT INTO users (|");
        assert_eq!(
            context.scope.target_relation.as_ref().unwrap().name,
            "users"
        );
        assert!(
            context
                .expectations
                .contains(&Expectation::Name(NameExpectation::Column(
                    ColumnContext::TargetRelation
                )))
        );
    }

    #[test]
    fn cte_is_exposed_as_a_scope_reference() {
        let context = collect("WITH active(id) AS (SELECT id FROM users) SELECT | FROM active");
        assert_eq!(context.scope.ctes[0].name.name, "active");
        assert_eq!(context.scope.references[0].kind, RangeReferenceKind::Cte);
        assert_eq!(
            context.scope.references[0].alias_columns,
            Vec::<String>::new()
        );
    }

    #[test]
    fn cursor_in_middle_replaces_whole_identifier() {
        let context = collect("SELECT * FROM us|ers");
        assert_eq!(context.prefix, "us");
        assert_eq!(
            &"SELECT * FROM users"
                [usize::from(context.replacement.start())..usize::from(context.replacement.end())],
            "users"
        );
    }

    #[test]
    fn update_set_and_alter_table_use_target_relation_columns() {
        for marked in [
            "UPDATE users SET |",
            "ALTER TABLE users RENAME COLUMN |",
            "CREATE INDEX users_name ON users (|",
        ] {
            let context = collect(marked);
            assert_eq!(
                context.scope.target_relation.as_ref().unwrap().name,
                "users"
            );
            assert!(
                context
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Column(
                        ColumnContext::TargetRelation
                    )))
            );
        }
    }

    #[test]
    fn multi_statement_context_uses_cursor_statement() {
        let context = collect("SELECT * FROM old_table; SELECT | FROM users");
        assert_eq!(context.scope.references.len(), 1);
        assert_eq!(context.scope.references[0].name.name, "users");
    }

    #[test]
    fn correlated_subquery_inherits_outer_scope() {
        let context = collect(
            "SELECT * FROM users u WHERE EXISTS (SELECT | FROM orders o WHERE o.user_id = u.id)",
        );
        assert_eq!(context.scope.local_references[0].exposed_name(), "o");
        assert_eq!(context.scope.outer_references[0][0].exposed_name(), "u");
        assert_eq!(
            context
                .scope
                .references
                .iter()
                .map(RangeReference::exposed_name)
                .collect::<Vec<_>>(),
            vec!["o", "u"]
        );
    }

    #[test]
    fn from_subquery_only_inherits_outer_scope_when_lateral() {
        let non_lateral = collect("SELECT * FROM users u, (SELECT |) s");
        assert!(non_lateral.scope.outer_references.is_empty());

        let lateral = collect("SELECT * FROM users u, LATERAL (SELECT |) s");
        assert_eq!(lateral.scope.outer_references[0][0].exposed_name(), "u");
    }

    #[test]
    fn strings_and_comments_do_not_offer_sql_completion() {
        for marked in ["SELECT 'abc|", "SELECT 1 -- comment|", "SELECT /* comment|"] {
            assert!(collect(marked).expectations.is_empty(), "{marked}");
        }
        assert!(!collect("SELECT \"na|").expectations.is_empty());
    }

    #[test]
    fn qualified_relation_and_type_slots_are_structured() {
        let relation = collect("SELECT * FROM public.us|");
        assert!(
            relation
                .expectations
                .contains(&Expectation::Name(NameExpectation::Relation {
                    schema: Some("public".into())
                }))
        );

        for marked in [
            "SELECT id::inte| FROM users",
            "SELECT CAST(id AS inte|) FROM users",
            "ALTER TABLE users ALTER COLUMN id TYPE inte|",
        ] {
            assert!(
                collect(marked)
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Type { schema: None })),
                "{marked}"
            );
        }
    }
}
