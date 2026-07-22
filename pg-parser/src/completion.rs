use std::{cell::RefCell, rc::Rc};

use crate::{KEYWORDS, ObjectType, TextRange, TextSize, Token, TokenKind, TokenValue, lex};

#[path = "completion/scope.rs"]
mod scope;

use scope::collect_scope;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionContext {
    pub replacement: TextRange,
    pub prefix: String,
    pub statement: TextRange,
    pub expectations: Vec<Expectation>,
    pub scope: ScopeSnapshot,
}

macro_rules! define_completion_slots {
    ($($slot:ident,)*) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub(crate) enum CompletionSlot {
            $($slot,)*
        }

        impl CompletionSlot {
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[$(Self::$slot,)*];
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
    SelectContinuation,
    JoinUsingColumn,
    TypeName,
    AlterTableColumnName,
    DropRelation,
    ObjectColumnName,
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

    fn includes_target_relation_columns(self) -> bool {
        matches!(
            self,
            Self::ReturningExpression
                | Self::ReturningExpressionAfterComma
                | Self::OnConflictInferenceElement
                | Self::OnConflictInferenceElementAfterComma
                | Self::OnConflictInferenceWhere
                | Self::OnConflictUpdateWhere
                | Self::OnConflictSelectWhere
                | Self::OnConflictSetValue
                | Self::UpdateSetValue
                | Self::UpdateWhere
                | Self::DeleteWhere
                | Self::MergeJoinCondition
                | Self::MergeWhenCondition
                | Self::MergeSetValue
                | Self::MergeInsertValue
                | Self::MergeInsertValueAfterComma
        )
    }

    fn allows_default(self) -> bool {
        matches!(
            self,
            Self::UpdateSetValue | Self::OnConflictSetValue | Self::MergeSetValue
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Expectation {
    Token(TokenKind),
    Name(NameExpectation),
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
    frames: Vec<ScopeFrame>,
    graph: BindingGraph,
    target_relation: Option<TargetRelationId>,
}

impl ScopeSnapshot {
    /// Query scopes ordered from the cursor's scope to its outermost scope.
    pub fn frames(&self) -> &[ScopeFrame] {
        &self.frames
    }

    pub fn range(&self, id: RangeBindingId) -> &RangeBinding {
        &self.graph.ranges[id.0]
    }

    pub fn cte(&self, id: CteBindingId) -> &CteBinding {
        &self.graph.ctes[id.0]
    }

    pub fn target(&self, id: TargetRelationId) -> &TargetRelation {
        &self.graph.target_relations[id.0]
    }

    pub fn target_relation_id(&self) -> Option<TargetRelationId> {
        self.target_relation
    }

    pub fn target_relation(&self) -> Option<&TargetRelation> {
        self.target_relation.map(|id| self.target(id))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeFrame {
    ranges: Vec<RangeBindingId>,
    ctes: Vec<CteBindingId>,
}

impl ScopeFrame {
    pub fn ranges(&self) -> &[RangeBindingId] {
        &self.ranges
    }

    pub fn ctes(&self) -> &[CteBindingId] {
        &self.ctes
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BindingGraph {
    ctes: Vec<CteBinding>,
    ranges: Vec<RangeBinding>,
    target_relations: Vec<TargetRelation>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CteBindingId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RangeBindingId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TargetRelationId(usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CteBinding {
    pub name: String,
    pub column_aliases: Vec<String>,
    pub row_shape: RowShape,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeBindingKind {
    Relation,
    Cte,
    Derived,
    Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeBinding {
    pub source: RangeSource,
    pub name: String,
    pub alias: Option<String>,
    pub column_aliases: Vec<String>,
    pub range: TextRange,
    pub lateral: bool,
}

impl RangeBinding {
    pub fn kind(&self) -> RangeBindingKind {
        match self.source {
            RangeSource::Relation(_) => RangeBindingKind::Relation,
            RangeSource::Cte(_) => RangeBindingKind::Cte,
            RangeSource::Derived(_) => RangeBindingKind::Derived,
            RangeSource::Function(_) => RangeBindingKind::Function,
        }
    }

    pub fn exposed_name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeSource {
    Relation(QualifiedName),
    Cte(CteBindingId),
    Derived(RowShape),
    Function(QualifiedName),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetRelation {
    pub name: QualifiedName,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RowShape {
    pub sources: Vec<RangeBindingId>,
    pub items: Vec<RowShapeItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RowShapeItem {
    Column {
        name: String,
        origin: RowColumnOrigin,
    },
    Wildcard {
        binding: Option<RangeBindingId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RowColumnOrigin {
    Expression,
    Column {
        binding: Option<RangeBindingId>,
        name: String,
    },
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
    events: Vec<CompletionEvent>,
    default_allowed: bool,
}

pub(crate) type SharedCompletionRecorder = Rc<RefCell<CompletionRecorder>>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionEvent {
    slot: CompletionSlot,
    signal: CompletionSignal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CompletionSignal {
    Expectation(Expectation),
    Expression,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ParserCompletion {
    events: Vec<CompletionEvent>,
}

impl ParserCompletion {
    fn expectations(&self) -> Vec<Expectation> {
        let mut result = Vec::new();
        for event in &self.events {
            let CompletionSignal::Expectation(expectation) = &event.signal else {
                continue;
            };
            if !result.contains(expectation) {
                result.push(expectation.clone());
            }
        }
        result
    }

    #[cfg(test)]
    fn contains(&self, slot: CompletionSlot, expectation: &Expectation) -> bool {
        self.events.iter().any(|event| {
            event.slot == slot && event.signal == CompletionSignal::Expectation(expectation.clone())
        })
    }

    #[cfg(test)]
    fn contains_expression(&self, slot: CompletionSlot) -> bool {
        self.events
            .iter()
            .any(|event| event.slot == slot && event.signal == CompletionSignal::Expression)
    }

    #[cfg(test)]
    fn expects_expression(&self) -> bool {
        self.events
            .iter()
            .any(|event| event.signal == CompletionSignal::Expression)
    }
}

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

    pub(crate) fn replace_default_allowed(&mut self, allowed: bool) -> bool {
        std::mem::replace(&mut self.default_allowed, allowed)
    }

    pub(crate) fn record_at(&mut self, slot: CompletionSlot, expectation: Expectation) {
        self.record_signal(slot, CompletionSignal::Expectation(expectation));
    }

    fn record_signal(&mut self, slot: CompletionSlot, signal: CompletionSignal) {
        let event = CompletionEvent { slot, signal };
        if !self.events.contains(&event) {
            self.events.push(event);
        }
    }

    pub(crate) fn record_expression_at(&mut self, slot: CompletionSlot) {
        self.record_expression_with_tokens(slot, expression_start_tokens().iter().copied());
    }

    pub(crate) fn record_expression_at_with_root(
        &mut self,
        slot: CompletionSlot,
        root_slot: CompletionSlot,
    ) {
        self.record_expression_at(slot);
        self.record_root_column_context(slot, root_slot);
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

    pub(crate) fn record_restricted_expression_at_with_root(
        &mut self,
        slot: CompletionSlot,
        root_slot: CompletionSlot,
    ) {
        self.record_restricted_expression_at(slot);
        self.record_root_column_context(slot, root_slot);
    }

    fn record_root_column_context(&mut self, slot: CompletionSlot, root_slot: CompletionSlot) {
        if (root_slot.column_context() == ColumnContext::TargetRelation
            || root_slot.includes_target_relation_columns())
            && slot.column_context() != ColumnContext::TargetRelation
        {
            self.record_at(
                slot,
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            );
        }
    }

    fn record_expression_with_tokens(
        &mut self,
        slot: CompletionSlot,
        tokens: impl IntoIterator<Item = TokenKind>,
    ) {
        self.record_signal(slot, CompletionSignal::Expression);
        self.record_at(
            slot,
            Expectation::Name(NameExpectation::Column(slot.column_context())),
        );
        if slot.includes_target_relation_columns()
            && slot.column_context() != ColumnContext::TargetRelation
        {
            self.record_at(
                slot,
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            );
        }
        self.record_at(
            slot,
            Expectation::Name(NameExpectation::Function { schema: None }),
        );
        if self.default_allowed || slot.allows_default() {
            self.record_at(slot, Expectation::Token(TokenKind::Default));
        }
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
    let expectations = if suppress_completion_at_cursor(sql, cursor_usize) {
        Vec::new()
    } else {
        let parser_completion = collect_parser_expectations(&statement_tokens, replacement_start);
        parser_completion.expectations()
    };

    Ok(CompletionContext {
        replacement,
        prefix,
        statement,
        expectations,
        scope,
    })
}

fn collect_parser_expectations(tokens: &[Token], offset: usize) -> ParserCompletion {
    let mut prefix: Vec<Token> = tokens
        .iter()
        .filter(|token| token.location() < offset)
        .cloned()
        .collect();
    prefix.push(Token::synthetic(TokenKind::Eof, offset));
    let recorder = Rc::new(RefCell::new(CompletionRecorder::default()));
    let mut parser = crate::parser::Parser::for_completion(prefix, recorder.clone());
    parser.parse_completion_statement();
    Rc::try_unwrap(recorder)
        .map(|recorder| {
            let recorder = recorder.into_inner();
            ParserCompletion {
                events: recorder.events,
            }
        })
        .unwrap_or_else(|recorder| {
            let recorder = recorder.borrow();
            ParserCompletion {
                events: recorder.events.clone(),
            }
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
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[path = "slot_fixtures.rs"]
    mod slot_fixtures;

    use slot_fixtures::{SLOT_FIXTURES, SlotContract};

    fn collect(marked: &str) -> CompletionContext {
        let cursor = marked.find('|').expect("test input must contain a cursor");
        let sql = marked.replacen('|', "", 1);
        collect_completion(&sql, text_size(cursor)).unwrap()
    }

    fn range_names(context: &CompletionContext) -> Vec<&str> {
        context
            .scope
            .frames()
            .iter()
            .flat_map(ScopeFrame::ranges)
            .map(|id| context.scope.range(*id).exposed_name())
            .collect()
    }

    fn local_range(context: &CompletionContext, index: usize) -> &RangeBinding {
        context
            .scope
            .range(context.scope.frames()[0].ranges()[index])
    }

    fn collect_parser_completion(marked: &str) -> ParserCompletion {
        let cursor = marked.find('|').expect("test input must contain a cursor");
        let sql = marked.replacen('|', "", 1);
        let tokens = completion_tokens(&sql, cursor);
        let (_, statement_tokens) = statement_at(&sql, cursor, &tokens);
        collect_parser_expectations(&statement_tokens, cursor)
    }

    fn collect_from_parser(marked: &str) -> Vec<Expectation> {
        collect_parser_completion(marked).expectations()
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
        for marked in [
            "SELECT |",
            "SELECT value + |",
            "SELECT count(|",
            "WITH x AS (SELECT |",
            "SELECT (SELECT |",
            "SELECT 1 IN (SELECT |",
            "SELECT 1 = ANY (SELECT |",
            "SELECT JSON_ARRAY(SELECT |",
            "CREATE RULE r AS ON UPDATE TO t DO (SELECT |",
            "SELECT sum(x) OVER (PARTITION BY |",
            "SELECT json_arrayagg(x ORDER BY |",
            "SELECT * FROM JSON_TABLE(|",
            "SELECT * FROM ROWS FROM (|",
            "SELECT * FROM XMLTABLE(|",
            "CREATE STATISTICS s ON |",
            "UPDATE t SET value[|",
        ] {
            let actual = collect_parser_completion(marked);
            assert!(actual.expects_expression(), "{marked}: {:?}", actual.events);
        }

        let cases = [
            ("|", Expectation::Token(TokenKind::Select)),
            (
                "CALL |",
                Expectation::Name(NameExpectation::Function { schema: None }),
            ),
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
    fn recursive_descent_expectations_keep_semantic_slot_provenance() {
        for (marked, slot) in [
            ("SELECT |", CompletionSlot::SelectTarget),
            ("SELECT count(|", CompletionSlot::FunctionArgument),
            (
                "SELECT sum(x) OVER (PARTITION BY |",
                CompletionSlot::WindowPartitionExpression,
            ),
        ] {
            let completion = collect_parser_completion(marked);
            assert!(
                completion.contains_expression(slot),
                "{marked}: expected expression provenance at {slot:?}, got {:?}",
                completion.events
            );
        }

        let cases = [
            (
                "|",
                CompletionSlot::StatementStart,
                Expectation::Token(TokenKind::Select),
            ),
            (
                "SELECT |",
                CompletionSlot::SelectTarget,
                Expectation::Name(NameExpectation::Column(ColumnContext::VisibleScope)),
            ),
            (
                "SELECT count(|",
                CompletionSlot::FunctionArgument,
                Expectation::Name(NameExpectation::Function { schema: None }),
            ),
            (
                "SELECT * FROM |",
                CompletionSlot::FromItem,
                Expectation::Name(NameExpectation::Relation { schema: None }),
            ),
            (
                "INSERT INTO |",
                CompletionSlot::InsertTargetRelation,
                Expectation::Name(NameExpectation::Relation { schema: None }),
            ),
            (
                "UPDATE users SET |",
                CompletionSlot::UpdateSetTarget,
                Expectation::Name(NameExpectation::Column(ColumnContext::TargetRelation)),
            ),
        ];
        for (marked, slot, expectation) in cases {
            let completion = collect_parser_completion(marked);
            assert!(
                completion.contains(slot, &expectation),
                "{marked}: expected {slot:?} -> {expectation:?}, got {:?}",
                completion.events
            );
        }
    }

    #[test]
    fn every_completion_slot_has_a_parser_owned_contract() {
        let mut classified = HashSet::new();
        for (slot, marked, contract) in SLOT_FIXTURES {
            assert!(classified.insert(*slot), "duplicate fixture for {slot:?}");
            let completion = collect_parser_completion(marked);
            let slot_events = completion
                .events
                .iter()
                .filter(|event| event.slot == *slot)
                .collect::<Vec<_>>();
            assert!(
                !slot_events.is_empty(),
                "{marked:?} did not reach {slot:?}; got {:?}",
                completion.events
            );
            let has_expectation = |predicate: fn(&Expectation) -> bool| {
                slot_events.iter().any(|event| {
                    matches!(&event.signal, CompletionSignal::Expectation(value) if predicate(value))
                })
            };
            let satisfied = match contract {
                SlotContract::Keyword => {
                    has_expectation(|value| matches!(value, Expectation::Token(_)))
                }
                SlotContract::Relation => has_expectation(|value| {
                    matches!(value, Expectation::Name(NameExpectation::Relation { .. }))
                }),
                SlotContract::Column => has_expectation(|value| {
                    matches!(value, Expectation::Name(NameExpectation::Column(_)))
                }),
                SlotContract::JoinUsingColumn => has_expectation(|value| {
                    matches!(
                        value,
                        Expectation::Name(NameExpectation::Column(ColumnContext::JoinUsing))
                    )
                }),
                SlotContract::Type => has_expectation(|value| {
                    matches!(value, Expectation::Name(NameExpectation::Type { .. }))
                }),
                SlotContract::Value => slot_events
                    .iter()
                    .any(|event| event.signal == CompletionSignal::Expression),
                SlotContract::Declaration => has_expectation(|value| {
                    matches!(value, Expectation::Name(NameExpectation::Declaration(_)))
                }),
                SlotContract::FunctionReference => has_expectation(|value| {
                    matches!(value, Expectation::Name(NameExpectation::Function { .. }))
                }),
                SlotContract::FromItem => {
                    has_expectation(|value| {
                        matches!(value, Expectation::Name(NameExpectation::Relation { .. }))
                    }) && has_expectation(|value| {
                        matches!(value, Expectation::Name(NameExpectation::Function { .. }))
                    }) && has_expectation(|value| {
                        matches!(value, Expectation::Token(TokenKind::LateralP))
                    })
                }
            };
            assert!(
                satisfied,
                "{marked:?} reached {slot:?} without its {contract:?} contract: {slot_events:?}"
            );
        }
        assert_eq!(
            classified,
            CompletionSlot::ALL.iter().copied().collect(),
            "the parser fixture matrix must classify every completion slot"
        );
    }

    #[test]
    fn implicit_completion_recording_is_forbidden() {
        let sources = parser_sources();
        for pattern in [
            "record_completion(",
            "record_expression_completion(",
            "record_restricted_expression_completion(",
            ".record_expression(",
            ".record_restricted_expression(",
        ] {
            let actual = sources.matches(pattern).count();
            assert_eq!(
                actual, 0,
                "implicit completion path {pattern:?} is forbidden; register a semantic CompletionSlot instead"
            );
        }
    }

    fn parser_sources() -> String {
        let parser_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![parser_root.join("parser.rs")];
        collect_rust_files(&parser_root.join("parser"), &mut files);
        files.sort();
        files
            .into_iter()
            .map(|path| {
                fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in
            fs::read_dir(directory).unwrap_or_else(|error| panic!("{directory:?}: {error}"))
        {
            let path = entry.expect("parser source entry").path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
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
            assert!(matches!(
                &local_range(&context, 0).source,
                RangeSource::Relation(name) if name.name == "users"
            ));
            assert_eq!(
                local_range(&context, 0).alias.as_deref(),
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
        assert_eq!(context.scope.frames()[0].ranges().len(), 2);
    }

    #[test]
    fn insert_column_list_targets_insert_relation() {
        let context = collect("INSERT INTO users (|");
        let target = context.scope.target_relation_id().unwrap();
        assert_eq!(context.scope.target(target).name.name, "users");
        assert_eq!(
            context.scope.target_relation(),
            Some(context.scope.target(target))
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
        let cte = context.scope.frames()[0].ctes()[0];
        assert_eq!(context.scope.cte(cte).name, "active");
        assert_eq!(local_range(&context, 0).kind(), RangeBindingKind::Cte);
        assert!(matches!(local_range(&context, 0).source, RangeSource::Cte(id) if id == cte));
        assert!(local_range(&context, 0).column_aliases.is_empty());
        assert!(matches!(
            context.scope.cte(cte).row_shape.items.as_slice(),
            [RowShapeItem::Column { name, .. }] if name == "id"
        ));
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
            assert_eq!(context.scope.target_relation().unwrap().name.name, "users");
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
        assert_eq!(range_names(&context), ["users"]);
    }

    #[test]
    fn correlated_subquery_inherits_outer_scope() {
        let context = collect(
            "SELECT * FROM users u WHERE EXISTS (SELECT | FROM orders o WHERE o.user_id = u.id)",
        );
        assert_eq!(context.scope.frames().len(), 2);
        assert_eq!(local_range(&context, 0).exposed_name(), "o");
        assert_eq!(
            context
                .scope
                .range(context.scope.frames()[1].ranges()[0])
                .exposed_name(),
            "u"
        );
        assert_eq!(range_names(&context), vec!["o", "u"]);
    }

    #[test]
    fn from_subquery_only_inherits_outer_scope_when_lateral() {
        let non_lateral = collect("SELECT * FROM users u, (SELECT |) s");
        assert_eq!(non_lateral.scope.frames().len(), 1);

        let lateral = collect("SELECT * FROM users u, LATERAL (SELECT |) s");
        assert_eq!(lateral.scope.frames().len(), 2);
        assert_eq!(range_names(&lateral), ["u"]);
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
                    .target_relation()
                    .map(|target| target.name.name.as_str()),
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
        assert_eq!(range_names(&context), ["u", "o"]);
        assert_eq!(
            context
                .scope
                .target_relation()
                .map(|target| target.name.name.as_str()),
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
            assert_eq!(range_names(&context), expected, "{marked}");
        }
    }

    #[test]
    fn with_dml_statements_keep_ctes_targets_and_sources_in_scope() {
        let context = collect(
            "WITH recent(order_id, user_id, amount) AS \
             (SELECT id, user_id, amount FROM orders) \
             UPDATE users u SET name = | FROM recent r",
        );
        assert_eq!(range_names(&context), ["u", "r"]);
        assert_eq!(
            context
                .scope
                .target_relation()
                .map(|target| target.name.name.as_str()),
            Some("users")
        );
    }

    #[test]
    fn insert_select_scope_ends_before_conflict_and_returning_clauses() {
        let source = collect("INSERT INTO users(name) SELECT | FROM orders o");
        assert_eq!(range_names(&source), ["o"]);

        let returning =
            collect("INSERT INTO users(name) SELECT amount::text FROM orders o RETURNING |");
        assert!(returning.scope.frames()[0].ranges().is_empty());
        assert_eq!(
            returning
                .scope
                .target_relation()
                .map(|target| target.name.name.as_str()),
            Some("users")
        );
    }
}
