use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};

use crate::{
    KEYWORDS, KeywordCategory, ObjectType, TextRange, TextSize, Token, TokenKind, TokenValue, lex,
    lookup_keyword,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionContext {
    pub replacement: TextRange,
    pub prefix: String,
    pub statement: TextRange,
    pub expectations: Vec<Expectation>,
    pub slots: Vec<CompletionSlot>,
    pub scope: ScopeSnapshot,
}

macro_rules! define_completion_slots {
    ($($slot:ident,)*) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum CompletionSlot {
            $($slot,)*
        }

        impl CompletionSlot {
            pub const ALL: &'static [Self] = &[$(Self::$slot,)*];
        }
    };
}

define_completion_slots! {
    StatementStart,
    CreateObjectKind,
    AlterObjectKind,
    DropObjectKind,
    FromItem,
    SelectTarget,
    SelectTargetAfterComma,
    SelectDistinctOn,
    SelectDistinctOnAfterComma,
    SelectWhere,
    SelectGroupBy,
    SelectGroupByAfterComma,
    SelectHaving,
    SelectOrderBy,
    SelectOrderByAfterComma,
    SelectLimit,
    SelectOffset,
    SelectFetchCount,
    ValuesExpression,
    ValuesExpressionAfterComma,
    WindowPartitionExpression,
    WindowPartitionExpressionAfterComma,
    WindowOrderExpression,
    WindowOrderExpressionAfterComma,
    WindowFrameStartOffset,
    WindowFrameEndOffset,
    TableSampleArgument,
    TableSampleArgumentAfterComma,
    TableSampleRepeatable,
    RowsFromFunction,
    RowsFromFunctionAfterComma,
    JoinOn,
    XmlTableNamespace,
    XmlTableNamespaceAfterComma,
    XmlTableRowExpression,
    XmlTableDocumentExpression,
    FunctionArgument,
    FunctionArgumentAfterComma,
    FunctionOrderBy,
    FunctionOrderByAfterComma,
    WithinGroupOrderBy,
    WithinGroupOrderByAfterComma,
    FunctionFilter,
    ArrayElement,
    ArrayElementAfterComma,
    ParenthesizedExpression,
    ParenthesizedExpressionAfterComma,
    CoalesceArgument,
    CoalesceArgumentAfterComma,
    MinmaxArgument,
    MinmaxArgumentAfterComma,
    NullifArgument,
    NullifArgumentAfterComma,
    InListExpression,
    InListExpressionAfterComma,
    GroupingArgument,
    GroupingArgumentAfterComma,
    CaseOperand,
    CaseWhenCondition,
    CaseThenResult,
    CaseElseResult,
    CastArgument,
    ExtractArgument,
    NormalizeArgument,
    PositionNeedle,
    PositionHaystack,
    OverlaySource,
    OverlayReplacement,
    OverlayStart,
    OverlayCount,
    SubstringSource,
    SubstringStart,
    SubstringCount,
    SubstringPattern,
    SubstringEscape,
    TrimArgument,
    TrimArgumentAfterComma,
    TrimSource,
    TrimSourceAfterComma,
    XmlExistsXpath,
    XmlExistsDocument,
    RowElement,
    RowElementAfterComma,
    XmlConcatArgument,
    XmlConcatArgumentAfterComma,
    XmlElementContent,
    XmlElementContentAfterComma,
    XmlAttributeExpression,
    XmlAttributeExpressionAfterComma,
    XmlForestExpression,
    XmlForestExpressionAfterComma,
    XmlParseValue,
    XmlPiValue,
    XmlRootDocument,
    XmlRootVersion,
    XmlSerializeValue,
    ExecuteParameter,
    ExecuteParameterAfterComma,
    PartitionListValue,
    PartitionListValueAfterComma,
    PartitionRangeFromValue,
    PartitionRangeFromValueAfterComma,
    PartitionRangeToValue,
    PartitionRangeToValueAfterComma,
    MergeInsertValue,
    MergeInsertValueAfterComma,
    ReturningExpression,
    ReturningExpressionAfterComma,
    GraphTableColumnExpression,
    GraphTableColumnExpressionAfterComma,
    PropertyGraphPropertyExpression,
    PropertyGraphPropertyExpressionAfterComma,
    JsonArrayAggOrderBy,
    JsonArrayAggOrderByAfterComma,
    CteName,
    CteAliasColumn,
    CteAliasColumnAfterComma,
    CteContinuation,
    UpdateWhere,
    DeleteWhere,
    ForPortionTarget,
    ForPortionStart,
    ForPortionEnd,
    OnConflictInferenceWhere,
    OnConflictUpdateWhere,
    OnConflictSelectWhere,
    UpdateSetTarget,
    UpdateSetTargetAfterComma,
    UpdateSetValue,
    OnConflictSetTarget,
    OnConflictSetTargetAfterComma,
    OnConflictSetValue,
    MergeSetTarget,
    MergeSetTargetAfterComma,
    MergeSetValue,
    AssignmentSubscriptLowerOrIndex,
    AssignmentSliceUpper,
    MergeJoinCondition,
    MergeWhenCondition,
    AlterColumnUsing,
    AlterColumnDefault,
    AlterColumnExpression,
    PublicationRowFilter,
    RuleWhere,
    ColumnDefault,
    ColumnCheck,
    ColumnGenerated,
    TableCheck,
    ExclusionWhere,
    TriggerWhen,
    IndexPredicate,
    DomainDefault,
    DomainCheck,
    AlterDomainDefault,
    AlterDomainCheck,
    CopyWhere,
    CreatePolicyUsing,
    CreatePolicyCheck,
    AlterPolicyUsing,
    AlterPolicyCheck,
    ReturnExpression,
    GraphTableWhere,
    GraphPathWhere,
    GraphElementWhere,
    JsonTableContext,
    JsonTablePassingArgument,
    JsonTablePassingArgumentAfterComma,
    StatisticsExpression,
    StatisticsExpressionAfterComma,
    CreateIndexElement,
    CreateIndexElementAfterComma,
    ExclusionElement,
    ExclusionElementAfterComma,
    OnConflictInferenceElement,
    OnConflictInferenceElementAfterComma,
    PartitionKeyExpression,
    PartitionKeyExpressionAfterComma,
    JsonTableDefaultBehavior,
    CallRoutine,
    InsertTargetRelation,
    UpdateTargetRelation,
    DeleteTargetRelation,
    IndexRelation,
    AlterTableRelation,
    InsertColumn,
}

impl CompletionSlot {
    fn column_context(self) -> ColumnContext {
        if matches!(
            self,
            Self::AlterColumnUsing
                | Self::AlterColumnDefault
                | Self::AlterColumnExpression
                | Self::PublicationRowFilter
                | Self::RuleWhere
                | Self::ColumnCheck
                | Self::ColumnGenerated
                | Self::TableCheck
                | Self::ExclusionWhere
                | Self::TriggerWhen
                | Self::IndexPredicate
                | Self::CreateIndexElement
                | Self::CreateIndexElementAfterComma
                | Self::ExclusionElement
                | Self::ExclusionElementAfterComma
                | Self::OnConflictInferenceElement
                | Self::OnConflictInferenceElementAfterComma
                | Self::CopyWhere
                | Self::CreatePolicyUsing
                | Self::CreatePolicyCheck
                | Self::AlterPolicyUsing
                | Self::AlterPolicyCheck
        ) {
            ColumnContext::TargetRelation
        } else {
            ColumnContext::VisibleScope
        }
    }
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
    Declaration(DeclarationKind),
}

impl NameExpectation {
    pub fn is_reference(&self) -> bool {
        !matches!(self, Self::Declaration(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeclarationKind {
    Cte,
    Alias,
    Column,
    Object(ObjectType),
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
    slots: Vec<CompletionSlot>,
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

    pub(crate) fn record_at(&mut self, slot: CompletionSlot, expectation: Expectation) {
        if !self.slots.contains(&slot) {
            self.slots.push(slot);
        }
        if !self.expectations.contains(&expectation) {
            self.expectations.push(expectation);
        }
    }

    pub(crate) fn record_expression_at(&mut self, slot: CompletionSlot) {
        self.record_expression_with_tokens(slot, expression_start_tokens().iter().copied());
    }

    pub(crate) fn record_restricted_expression_at(&mut self, slot: CompletionSlot) {
        self.record_expression_with_tokens(
            slot,
            expression_start_tokens()
                .iter()
                .copied()
                .filter(|token| *token != TokenKind::Not),
        );
    }

    fn record_expression_with_tokens(
        &mut self,
        slot: CompletionSlot,
        tokens: impl IntoIterator<Item = TokenKind>,
    ) {
        self.record_at(slot, Expectation::Expression);
        self.record_at(
            slot,
            Expectation::Name(NameExpectation::Column(slot.column_context())),
        );
        self.record_at(
            slot,
            Expectation::Name(NameExpectation::Function { schema: None }),
        );
        for token in tokens {
            self.record_at(slot, Expectation::Token(token));
        }
    }
}

fn expression_start_tokens() -> &'static [TokenKind] {
    &[
        TokenKind::NullP,
        TokenKind::TrueP,
        TokenKind::FalseP,
        TokenKind::Not,
        TokenKind::Exists,
        TokenKind::Array,
        TokenKind::Case,
        TokenKind::Grouping,
        TokenKind::Collation,
        TokenKind::Cast,
        TokenKind::Treat,
        TokenKind::Extract,
        TokenKind::Normalize,
        TokenKind::Position,
        TokenKind::Overlay,
        TokenKind::Substring,
        TokenKind::Trim,
        TokenKind::Xmlexists,
        TokenKind::SystemUser,
        TokenKind::CurrentDate,
        TokenKind::CurrentTime,
        TokenKind::CurrentTimestamp,
        TokenKind::Localtime,
        TokenKind::Localtimestamp,
        TokenKind::CurrentRole,
        TokenKind::CurrentUser,
        TokenKind::User,
        TokenKind::SessionUser,
        TokenKind::CurrentCatalog,
        TokenKind::CurrentSchema,
        TokenKind::Xmlconcat,
        TokenKind::Xmlelement,
        TokenKind::Xmlforest,
        TokenKind::Xmlparse,
        TokenKind::Xmlpi,
        TokenKind::Xmlroot,
        TokenKind::Xmlserialize,
        TokenKind::Json,
        TokenKind::JsonObject,
        TokenKind::JsonArray,
        TokenKind::JsonScalar,
        TokenKind::JsonSerialize,
        TokenKind::JsonQuery,
        TokenKind::JsonExists,
        TokenKind::JsonValue,
        TokenKind::JsonObjectagg,
        TokenKind::JsonArrayagg,
        TokenKind::Row,
        TokenKind::Coalesce,
        TokenKind::Greatest,
        TokenKind::Least,
        TokenKind::Nullif,
    ]
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
    let (expectations, slots) = if suppress_completion_at_cursor(sql, cursor_usize) {
        (Vec::new(), Vec::new())
    } else {
        let (mut expectations, slots) =
            collect_parser_expectations(&statement_tokens, replacement_start);
        let parser_expects_expression = expectations.contains(&Expectation::Expression);
        if !slots.contains(&CompletionSlot::CteContinuation) {
            for fallback in collect_tricky_expectations(
                &statement_tokens,
                replacement_start,
                &scope,
                parser_expects_expression,
            ) {
                if !expectations.contains(&fallback) {
                    expectations.push(fallback);
                }
            }
        }
        (expectations, slots)
    };

    Ok(CompletionContext {
        replacement,
        prefix,
        statement,
        expectations,
        slots,
        scope,
    })
}

fn collect_parser_expectations(
    tokens: &[Token],
    offset: usize,
) -> (Vec<Expectation>, Vec<CompletionSlot>) {
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
        .map(|recorder| {
            let recorder = recorder.into_inner();
            (recorder.expectations, recorder.slots)
        })
        .unwrap_or_else(|recorder| {
            let recorder = recorder.borrow();
            (recorder.expectations.clone(), recorder.slots.clone())
        })
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
    let mut local_references = frames
        .pop()
        .map(|(_, references)| references)
        .unwrap_or_default();
    if let Some((statement, statement_kind)) = top_level_dml_statement(&tokens) {
        if statement_kind == TokenKind::Insert
            && [TokenKind::Conflict, TokenKind::Returning]
                .into_iter()
                .any(|kind| {
                    find_top_level_token_after(&tokens, statement, kind)
                        .is_some_and(|index| tokens[index].token.location() < cursor)
                })
        {
            local_references.clear();
        }
        if statement_kind == TokenKind::Merge {
            local_references.extend(collect_merge_references(&tokens));
        } else if matches!(statement_kind, TokenKind::Update | TokenKind::DeleteP) {
            local_references.extend(collect_update_delete_references(&tokens, &ctes));
        }
    }
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
    let (parts, _) = parse_qualified_name(tokens, start, 0);
    (!parts.is_empty()).then(|| qualified_name(parts))
}

fn collect_merge_references(tokens: &[DepthToken]) -> Vec<RangeReference> {
    [TokenKind::Into, TokenKind::Using]
        .into_iter()
        .filter_map(|marker| {
            let start = tokens
                .iter()
                .position(|token| token.depth == 0 && token.token.kind == marker)?
                + 1;
            let first = tokens.get(start)?;
            let (parts, next) = parse_qualified_name(tokens, start, 0);
            if parts.is_empty() {
                return None;
            }
            let (alias, alias_columns, end) = parse_alias(tokens, next, 0);
            let end_location = tokens
                .get(end.saturating_sub(1))
                .map_or(first.token.end_location(), |token| {
                    token.token.end_location()
                });
            Some(RangeReference {
                kind: RangeReferenceKind::Relation,
                name: qualified_name(parts),
                alias,
                alias_columns,
                range: TextRange::new(first.token.range.start(), text_size(end_location)),
                lateral: false,
            })
        })
        .collect()
}

fn collect_update_delete_references(
    tokens: &[DepthToken],
    ctes: &[RangeReference],
) -> Vec<RangeReference> {
    let Some((statement, first)) = top_level_dml_statement(tokens) else {
        return Vec::new();
    };
    let (target_start, source_marker) = match first {
        TokenKind::Update => (statement + 1, TokenKind::From),
        TokenKind::DeleteP => {
            let Some(from) = find_top_level_token_after(tokens, statement, TokenKind::From) else {
                return Vec::new();
            };
            (from + 1, TokenKind::Using)
        }
        _ => return Vec::new(),
    };

    let mut result = relation_reference_at(tokens, target_start)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(source) = tokens
        .iter()
        .position(|token| token.depth == 0 && token.token.kind == source_marker)
    {
        result.extend(parse_from_references(tokens, source + 1, ctes));
    }
    result
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

fn relation_reference_at(tokens: &[DepthToken], start: usize) -> Option<RangeReference> {
    let first = tokens.get(start)?;
    let (parts, next) = parse_qualified_name(tokens, start, 0);
    if parts.is_empty() {
        return None;
    }
    let (alias, alias_columns, end) = parse_alias(tokens, next, 0);
    let end_location = tokens
        .get(end.saturating_sub(1))
        .map_or(first.token.end_location(), |token| {
            token.token.end_location()
        });
    Some(RangeReference {
        kind: RangeReferenceKind::Relation,
        name: qualified_name(parts),
        alias,
        alias_columns,
        range: TextRange::new(first.token.range.start(), text_size(end_location)),
        lateral: false,
    })
}

/// Enrich parser-produced candidates for cursor shapes that cannot be
/// represented by the strict token stream, such as a partial identifier after
/// a qualifier. Grammar alternatives still come from the recursive-descent
/// productions through `collect_parser_expectations`.
fn collect_tricky_expectations(
    tokens: &[Token],
    offset: usize,
    scope: &ScopeSnapshot,
    parser_expects_expression: bool,
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
    let cursor_expects_expression = parser_expects_expression
        || matches!(
            last.kind,
            TokenKind::Select
                | TokenKind::Where
                | TokenKind::Having
                | TokenKind::On
                | TokenKind::By
                | TokenKind::Returning
                | TokenKind::Set
        )
        || (last.kind == TokenKind::Char(',')
            && current_clause(&before) == Some(TokenKind::Select));

    if last.kind == TokenKind::Char('.') {
        if let Some(qualifier) = before
            .get(before.len().saturating_sub(2))
            .and_then(|token| token_name(Some(token)))
        {
            let before_qualifier = &before[..before.len().saturating_sub(2)];
            if expects_type_name(before_qualifier) {
                push_unique(
                    &mut result,
                    Expectation::Name(NameExpectation::Type {
                        schema: Some(qualifier),
                    }),
                );
            } else if qualifier_is_relation_schema(&before) {
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

    if inside_create_index_columns(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
        );
        add_expression_expectations(&mut result);
        push_unique(&mut result, Expectation::Token(TokenKind::Char(')')));
        return result;
    }

    if after_alter_table_column_keyword(&before) {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
        );
        return result;
    }

    if expects_existing_relation(&before) && !parser_expects_expression {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Relation { schema: None }),
        );
        push_unique(&mut result, Expectation::Name(NameExpectation::Schema));
        return result;
    }

    if cursor_expects_expression
        && target_relation_columns_visible(&before)
        && scope.target_relation.is_some()
        && !scope.references.iter().any(|reference| {
            scope
                .target_relation
                .as_ref()
                .is_some_and(|target| reference.name == *target)
        })
    {
        push_unique(
            &mut result,
            Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
        );
    }
    if cursor_expects_expression && default_expression_is_valid(&before) {
        push_unique(&mut result, Expectation::Token(TokenKind::Default));
    }

    match last.kind {
        TokenKind::Select | TokenKind::Where | TokenKind::Having | TokenKind::On => {
            add_expression_expectations(&mut result);
        }
        TokenKind::Char(',') if current_clause(&before) == Some(TokenKind::Select) => {
            add_expression_expectations(&mut result);
        }
        TokenKind::From | TokenKind::Join | TokenKind::Into | TokenKind::Update
            if !parser_expects_expression =>
        {
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
        _ if !parser_expects_expression => {
            if current_clause(&before).is_some() {
                add_expression_tail_expectations(&mut result);
            } else if scope.references.is_empty() {
                add_statement_starters(&mut result);
            }
        }
        _ => {}
    }
    result
}

fn target_relation_columns_visible(tokens: &[&Token]) -> bool {
    let Some((statement, first)) = top_level_statement(tokens) else {
        return false;
    };
    match first {
        TokenKind::Update | TokenKind::DeleteP | TokenKind::Copy => true,
        TokenKind::Insert => top_level_tokens_after(tokens, statement)
            .any(|token| matches!(token.kind, TokenKind::Returning | TokenKind::Conflict)),
        TokenKind::Create => top_level_tokens_after(tokens, statement).any(|token| {
            matches!(
                token.kind,
                TokenKind::Index
                    | TokenKind::Policy
                    | TokenKind::Publication
                    | TokenKind::Rule
                    | TokenKind::Trigger
            )
        }),
        TokenKind::Alter => top_level_tokens_after(tokens, statement)
            .any(|token| matches!(token.kind, TokenKind::Table | TokenKind::Policy)),
        _ => false,
    }
}

fn default_expression_is_valid(tokens: &[&Token]) -> bool {
    let Some((statement, kind)) = top_level_statement(tokens) else {
        return false;
    };
    match kind {
        TokenKind::Update => current_clause(tokens) == Some(TokenKind::Set),
        TokenKind::Insert => {
            top_level_tokens_after(tokens, statement).any(|token| token.kind == TokenKind::Values)
                || (top_level_tokens_after(tokens, statement)
                    .any(|token| token.kind == TokenKind::Conflict)
                    && current_clause(tokens) == Some(TokenKind::Set))
        }
        _ => false,
    }
}

fn top_level_statement(tokens: &[&Token]) -> Option<(usize, TokenKind)> {
    top_level_tokens(tokens).find_map(|(index, token)| {
        matches!(
            token.kind,
            TokenKind::Select
                | TokenKind::Insert
                | TokenKind::Update
                | TokenKind::DeleteP
                | TokenKind::Merge
                | TokenKind::Create
                | TokenKind::Alter
                | TokenKind::Copy
        )
        .then_some((index, token.kind))
    })
}

fn top_level_tokens<'a>(tokens: &'a [&'a Token]) -> impl Iterator<Item = (usize, &'a Token)> {
    let mut depth = 0usize;
    tokens.iter().enumerate().filter_map(move |(index, token)| {
        let token_depth = depth;
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
        (token_depth == 0).then_some((index, *token))
    })
}

fn top_level_tokens_after<'a>(
    tokens: &'a [&'a Token],
    start: usize,
) -> impl Iterator<Item = &'a Token> {
    top_level_tokens(tokens)
        .filter(move |(index, _)| *index >= start)
        .map(|(_, token)| token)
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
    add_tokens(result, expression_start_tokens());
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
                return index > 0
                    && tokens[index - 1].kind == TokenKind::Using
                    && tokens[..index - 1]
                        .iter()
                        .any(|token| token.kind == TokenKind::Join);
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
    if tokens
        .iter()
        .any(|token| matches!(token.kind, TokenKind::Values | TokenKind::Select))
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
                | TokenKind::Tablesample
                | TokenKind::Repeatable
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
        collect_parser_expectations(&statement_tokens, cursor).0
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
            (
                "CALL |",
                Expectation::Name(NameExpectation::Function { schema: None }),
            ),
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
        for marked in ["SELECT | FROM users u", "SELECT id, | FROM users u"] {
            let context = collect(marked);
            assert!(
                context
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Column(
                        ColumnContext::VisibleScope
                    ))),
                "{marked}"
            );
            assert!(
                context
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Function {
                        schema: None
                    })),
                "{marked}"
            );
            assert_eq!(context.scope.references[0].name.name, "users", "{marked}");
            assert_eq!(
                context.scope.references[0].alias.as_deref(),
                Some("u"),
                "{marked}"
            );
        }
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

        for marked in [
            "SELECT id::pg_catalog.inte| FROM users",
            "SELECT CAST(id AS pg_catalog.inte|) FROM users",
            "ALTER TABLE users ALTER COLUMN id TYPE pg_catalog.inte|",
        ] {
            assert!(
                collect(marked)
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Type {
                        schema: Some("pg_catalog".into()),
                    })),
                "{marked}"
            );
        }
    }

    #[test]
    fn relation_owned_expression_contexts_keep_the_target_relation() {
        for marked in [
            "CREATE INDEX users_idx ON users (|",
            "CREATE POLICY users_policy ON users USING (|",
            "CREATE PUBLICATION users_publication FOR TABLE users WHERE (|",
            "COPY users FROM STDIN WHERE |",
        ] {
            let context = collect(marked);
            assert_eq!(
                context
                    .scope
                    .target_relation
                    .as_ref()
                    .map(|name| name.name.as_str()),
                Some("users"),
                "{marked}: {:?}",
                context.scope
            );
            assert!(
                context
                    .expectations
                    .contains(&Expectation::Name(NameExpectation::Column(
                        ColumnContext::TargetRelation
                    ))),
                "{marked}: {:?}",
                context.expectations
            );
        }
    }

    #[test]
    fn merge_expression_context_exposes_target_and_source_relations() {
        let context =
            collect("MERGE INTO users u USING orders o ON | WHEN MATCHED THEN DO NOTHING");
        assert_eq!(
            context
                .scope
                .references
                .iter()
                .map(RangeReference::exposed_name)
                .collect::<Vec<_>>(),
            ["u", "o"]
        );
        assert_eq!(
            context
                .scope
                .target_relation
                .as_ref()
                .map(|name| name.name.as_str()),
            Some("users")
        );
    }

    #[test]
    fn update_from_and_delete_using_expose_all_expression_relations() {
        for (marked, expected) in [
            ("UPDATE users u SET name = | FROM orders o", ["u", "o"]),
            ("DELETE FROM users u USING orders o WHERE |", ["u", "o"]),
        ] {
            let context = collect(marked);
            assert_eq!(
                context
                    .scope
                    .references
                    .iter()
                    .map(RangeReference::exposed_name)
                    .collect::<Vec<_>>(),
                expected,
                "{marked}"
            );
        }
    }

    #[test]
    fn with_dml_statements_keep_ctes_targets_and_sources_in_scope() {
        let context = collect(
            "WITH recent(order_id, user_id, amount) AS \
             (SELECT id, user_id, amount FROM orders) \
             UPDATE users u SET name = | FROM recent r",
        );
        assert_eq!(
            context
                .scope
                .references
                .iter()
                .map(RangeReference::exposed_name)
                .collect::<Vec<_>>(),
            ["u", "r"]
        );
        assert_eq!(
            context
                .scope
                .target_relation
                .as_ref()
                .map(|name| name.name.as_str()),
            Some("users")
        );
    }

    #[test]
    fn insert_select_scope_ends_before_conflict_and_returning_clauses() {
        let source = collect("INSERT INTO users(name) SELECT | FROM orders o");
        assert_eq!(
            source
                .scope
                .references
                .iter()
                .map(RangeReference::exposed_name)
                .collect::<Vec<_>>(),
            ["o"]
        );

        let returning =
            collect("INSERT INTO users(name) SELECT amount::text FROM orders o RETURNING |");
        assert!(returning.scope.references.is_empty());
        assert_eq!(
            returning
                .scope
                .target_relation
                .as_ref()
                .map(|name| name.name.as_str()),
            Some("users")
        );
    }
}
