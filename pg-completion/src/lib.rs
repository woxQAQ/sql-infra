use std::collections::{HashMap, HashSet};

use pg_parser::{
    ColumnContext, CompletionContext, CteBinding, CteBindingId, Expectation, NameExpectation,
    QualifiedName, RangeBinding, RangeBindingId, RangeBindingKind, RangeSource, RowColumnOrigin,
    RowShape, RowShapeItem, TextRange, TextSize, TokenKind, collect_completion, keyword_text,
    lookup_keyword,
};

#[derive(Clone, Copy, Debug)]
pub struct CompletionRequest<'a> {
    pub sql: &'a str,
    pub cursor: TextSize,
    pub search_path: &'a [&'a str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionResult {
    pub replacement: TextRange,
    pub items: Vec<CompletionItem>,
    pub is_incomplete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub catalog_identity: Option<CatalogObjectIdentity>,
    pub insert_text: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompletionKind {
    Keyword,
    Catalog(CatalogObjectKind),
    Cte,
    Alias,
}

#[allow(non_upper_case_globals)]
impl CompletionKind {
    pub const Schema: Self = Self::Catalog(CatalogObjectKind::Schema);
    pub const Table: Self = Self::Catalog(CatalogObjectKind::Table);
    pub const View: Self = Self::Catalog(CatalogObjectKind::View);
    pub const MaterializedView: Self = Self::Catalog(CatalogObjectKind::MaterializedView);
    pub const Column: Self = Self::Catalog(CatalogObjectKind::Column);
    pub const Function: Self = Self::Catalog(CatalogObjectKind::Function);
    pub const Type: Self = Self::Catalog(CatalogObjectKind::Type);
}

/// Metadata seam consumed by the completion module.
///
/// Implementations may query a live database, a cache, or an in-memory test
/// catalog. Adapters may return a superset: the completion module validates
/// object kind, prefix, namespace, and owner before ranking results. Database
/// permissions and connection-specific discoverability remain adapter concerns.
pub trait Catalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem>;
}

#[derive(Clone, Copy, Debug)]
pub struct CatalogQuery<'a> {
    pub kinds: &'a [CatalogObjectKind],
    pub prefix: &'a str,
    pub scope: CatalogQueryScope<'a>,
    pub search_path: &'a [&'a str],
}

impl<'a> CatalogQuery<'a> {
    pub fn global(kinds: &'a [CatalogObjectKind], prefix: &'a str) -> Self {
        Self {
            kinds,
            prefix,
            scope: CatalogQueryScope::Global,
            search_path: &[],
        }
    }

    pub fn in_schema(
        kinds: &'a [CatalogObjectKind],
        prefix: &'a str,
        schema: Option<&'a str>,
        search_path: &'a [&'a str],
    ) -> Self {
        Self {
            kinds,
            prefix,
            scope: CatalogQueryScope::Schema(schema),
            search_path,
        }
    }

    pub fn in_relation(
        kinds: &'a [CatalogObjectKind],
        prefix: &'a str,
        relation: &'a QualifiedName,
        search_path: &'a [&'a str],
    ) -> Self {
        Self {
            kinds,
            prefix,
            scope: CatalogQueryScope::Relation(relation),
            search_path,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CatalogQueryScope<'a> {
    Global,
    Schema(Option<&'a str>),
    Relation(&'a QualifiedName),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogItem {
    pub identity: CatalogObjectIdentity,
    pub definition: Option<String>,
    pub documentation: Option<String>,
    pub row_shape: Option<Vec<CatalogColumn>>,
}

impl CatalogItem {
    pub fn new(identity: CatalogObjectIdentity) -> Self {
        Self {
            identity,
            definition: None,
            documentation: None,
            row_shape: None,
        }
    }

    pub fn with_definition(mut self, definition: impl Into<String>) -> Self {
        self.definition = Some(definition.into());
        self
    }

    pub fn with_documentation(mut self, documentation: impl Into<String>) -> Self {
        self.documentation = Some(documentation.into());
        self
    }

    pub fn with_row_shape(mut self, columns: impl IntoIterator<Item = CatalogColumn>) -> Self {
        self.row_shape = Some(columns.into_iter().collect());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogColumn {
    pub name: String,
    pub definition: Option<String>,
}

impl CatalogColumn {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            definition: None,
        }
    }

    pub fn with_definition(mut self, definition: impl Into<String>) -> Self {
        self.definition = Some(definition.into());
        self
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CatalogObjectIdentity {
    pub kind: CatalogObjectKind,
    pub name: String,
    pub namespace: CatalogObjectNamespace,
    pub signature: Vec<String>,
}

impl CatalogObjectIdentity {
    pub fn global(kind: CatalogObjectKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            namespace: CatalogObjectNamespace::Global,
            signature: Vec::new(),
        }
    }

    pub fn in_schema(
        kind: CatalogObjectKind,
        schema: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            namespace: CatalogObjectNamespace::Schema(schema.into()),
            signature: Vec::new(),
        }
    }

    pub fn owned_by_relation(
        kind: CatalogObjectKind,
        relation: QualifiedName,
        name: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            namespace: CatalogObjectNamespace::Relation(relation),
            signature: Vec::new(),
        }
    }

    pub fn with_signature(
        mut self,
        signature: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.signature = signature.into_iter().map(Into::into).collect();
        self
    }

    pub fn schema(&self) -> Option<&str> {
        match &self.namespace {
            CatalogObjectNamespace::Schema(schema) => Some(schema),
            CatalogObjectNamespace::Relation(relation) => relation.schema.as_deref(),
            CatalogObjectNamespace::Global => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CatalogObjectNamespace {
    Global,
    Schema(String),
    Relation(QualifiedName),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CatalogObjectKind {
    AccessMethod,
    Aggregate,
    Cast,
    Schema,
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Sequence,
    Index,
    Column,
    Function,
    Procedure,
    Routine,
    Type,
    Domain,
    Collation,
    Conversion,
    Database,
    Role,
    Tablespace,
    Constraint,
    Trigger,
    EventTrigger,
    Rule,
    Policy,
    Operator,
    OperatorClass,
    OperatorFamily,
    Extension,
    Language,
    ForeignDataWrapper,
    ForeignServer,
    UserMapping,
    Publication,
    Subscription,
    Statistics,
    TextSearchConfiguration,
    TextSearchDictionary,
    TextSearchParser,
    TextSearchTemplate,
    Transform,
    PropertyGraph,
}

const SCHEMA_KINDS: &[CatalogObjectKind] = &[CatalogObjectKind::Schema];
const RELATION_KINDS: &[CatalogObjectKind] = &[
    CatalogObjectKind::Table,
    CatalogObjectKind::View,
    CatalogObjectKind::MaterializedView,
    CatalogObjectKind::ForeignTable,
    CatalogObjectKind::Sequence,
];
const COLUMN_KINDS: &[CatalogObjectKind] = &[CatalogObjectKind::Column];
const FUNCTION_KINDS: &[CatalogObjectKind] =
    &[CatalogObjectKind::Function, CatalogObjectKind::Aggregate];
const TYPE_KINDS: &[CatalogObjectKind] = &[CatalogObjectKind::Type, CatalogObjectKind::Domain];

/// Complete SQL at `request.cursor`.
///
/// This is the crate's external interface: callers do not need to coordinate
/// parsing, scope resolution, prefix filtering, quoting, deduplication, or
/// ranking themselves.
pub fn complete(
    request: CompletionRequest<'_>,
    catalog: Option<&dyn Catalog>,
) -> Result<CompletionResult, pg_parser::CompletionError> {
    let context = collect_completion(request.sql, request.cursor)?;
    let quoted = request
        .sql
        .get(usize::from(context.replacement.start())..)
        .is_some_and(|text| text.starts_with('"'));
    let mut candidates = Resolver {
        request,
        context: &context,
        catalog,
        quoted,
    }
    .resolve();
    candidates.retain(|candidate| prefix_matches(&candidate.item.label, &context.prefix, quoted));
    for candidate in &mut candidates {
        candidate.score += prefix_score(&candidate.item.label, &context.prefix, quoted);
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                left.item
                    .label
                    .to_lowercase()
                    .cmp(&right.item.label.to_lowercase())
            })
            .then_with(|| left.item.detail.cmp(&right.item.detail))
    });
    let mut seen = HashSet::new();
    let items = candidates
        .into_iter()
        .filter_map(|candidate| {
            let key = candidate.item.catalog_identity.clone().map_or_else(
                || CompletionIdentity::Local {
                    kind: candidate.item.kind,
                    label: candidate.item.label.to_lowercase(),
                    detail: candidate.item.detail.clone(),
                },
                CompletionIdentity::Catalog,
            );
            seen.insert(key).then_some(candidate.item)
        })
        .collect();
    Ok(CompletionResult {
        replacement: context.replacement,
        items,
        is_incomplete: catalog.is_none()
            && context.expectations.iter().any(
                |expectation| matches!(expectation, Expectation::Name(name) if name.is_reference()),
            ),
    })
}

struct Resolver<'a> {
    request: CompletionRequest<'a>,
    context: &'a CompletionContext,
    catalog: Option<&'a dyn Catalog>,
    quoted: bool,
}

#[derive(Clone)]
struct ResolvedColumn {
    name: String,
    definition: Option<String>,
    catalog_identity: Option<CatalogObjectIdentity>,
}

impl ResolvedColumn {
    fn local(name: String) -> Self {
        Self {
            name,
            definition: None,
            catalog_identity: None,
        }
    }

    fn catalog(item: CatalogItem) -> Self {
        Self {
            name: item.identity.name.clone(),
            definition: item.definition,
            catalog_identity: Some(item.identity),
        }
    }

    fn rename(&mut self, name: &str) {
        if !self.name.eq_ignore_ascii_case(name) {
            self.catalog_identity = None;
        }
        self.name = name.to_owned();
    }
}

fn apply_column_aliases(columns: &mut Vec<ResolvedColumn>, aliases: &[String]) {
    for (index, alias) in aliases.iter().enumerate() {
        if let Some(column) = columns.get_mut(index) {
            column.rename(alias);
        } else {
            columns.push(ResolvedColumn::local(alias.clone()));
        }
    }
}

impl Resolver<'_> {
    fn resolve(&self) -> Vec<RankedCandidate> {
        let mut result = Vec::new();
        for expectation in &self.context.expectations {
            match expectation {
                Expectation::Token(token) => self.resolve_token(*token, &mut result),
                Expectation::Name(expectation) => self.resolve_name(expectation, &mut result),
            }
        }
        result
    }

    fn resolve_token(&self, token: TokenKind, result: &mut Vec<RankedCandidate>) {
        let Some(keyword) = keyword_text(token) else {
            return;
        };
        result.push(RankedCandidate::new(
            CompletionItem {
                label: keyword.to_ascii_uppercase(),
                kind: CompletionKind::Keyword,
                catalog_identity: None,
                insert_text: keyword.to_ascii_uppercase(),
                detail: None,
                documentation: None,
            },
            100,
        ));
    }

    fn resolve_name(&self, expectation: &NameExpectation, result: &mut Vec<RankedCandidate>) {
        match expectation {
            NameExpectation::Schema => self.search_catalog(
                CatalogQuery::global(SCHEMA_KINDS, &self.context.prefix),
                180,
                result,
            ),
            NameExpectation::Relation { schema } => {
                if schema.is_none() {
                    let ctes = self.visible_ctes();
                    let shadowed = ctes
                        .iter()
                        .map(|id| self.context.scope.cte(*id).name.to_ascii_lowercase())
                        .collect::<HashSet<_>>();
                    for id in ctes {
                        result.push(self.cte_candidate(self.context.scope.cte(id), 520));
                    }
                    for item in self.search(CatalogQuery::in_schema(
                        RELATION_KINDS,
                        &self.context.prefix,
                        None,
                        self.request.search_path,
                    )) {
                        if !shadowed.contains(&item.identity.name.to_ascii_lowercase()) {
                            result.push(self.catalog_candidate(item, 400));
                        }
                    }
                } else {
                    self.search_catalog(
                        CatalogQuery::in_schema(
                            RELATION_KINDS,
                            &self.context.prefix,
                            schema.as_deref(),
                            self.request.search_path,
                        ),
                        400,
                        result,
                    );
                }
            }
            NameExpectation::Column(context) => self.resolve_columns(context, result),
            NameExpectation::Function { schema } => self.search_catalog(
                CatalogQuery::in_schema(
                    FUNCTION_KINDS,
                    &self.context.prefix,
                    schema.as_deref(),
                    self.request.search_path,
                ),
                300,
                result,
            ),
            NameExpectation::Type { schema } => self.search_catalog(
                CatalogQuery::in_schema(
                    TYPE_KINDS,
                    &self.context.prefix,
                    schema.as_deref(),
                    self.request.search_path,
                ),
                300,
                result,
            ),
            NameExpectation::Declaration(_) => {}
        }
    }

    fn resolve_columns(&self, context: &ColumnContext, result: &mut Vec<RankedCandidate>) {
        match context {
            ColumnContext::VisibleScope => {
                let mut hidden_columns = HashSet::new();
                let visible_name_frames = self.visible_range_frames();
                for (frame_index, frame) in self.context.scope.frames().iter().enumerate() {
                    let mut frame_columns = Vec::new();
                    for id in visible_name_frames
                        .get(frame_index)
                        .into_iter()
                        .flat_map(|ids| ids.iter().copied())
                    {
                        let binding = self.context.scope.range(id);
                        if binding.alias.is_some() {
                            result.push(self.binding_candidate(
                                binding,
                                CompletionKind::Alias,
                                540,
                            ));
                        }
                    }
                    for id in frame.ranges() {
                        let binding = self.context.scope.range(*id);
                        frame_columns.extend(
                            self.range_columns(*id)
                                .into_iter()
                                .filter(|column| {
                                    !hidden_columns.contains(&column.name.to_ascii_lowercase())
                                })
                                .map(|column| (binding.exposed_name().to_owned(), column)),
                        );
                    }
                    for (relation, column) in &frame_columns {
                        result.push(self.resolved_column_candidate(
                            column.clone(),
                            Some(relation.clone()),
                            500,
                        ));
                    }
                    hidden_columns.extend(
                        frame_columns
                            .into_iter()
                            .map(|(_, column)| column.name.to_ascii_lowercase()),
                    );
                }
            }
            ColumnContext::Qualified(qualifier) => {
                if let Some(frame) = self.visible_range_frames().into_iter().find_map(|frame| {
                    let matches = frame
                        .into_iter()
                        .filter(|id| {
                            names_equal(
                                self.context.scope.range(*id).exposed_name(),
                                qualifier,
                                self.quoted,
                            )
                        })
                        .collect::<Vec<_>>();
                    (!matches.is_empty()).then_some(matches)
                }) {
                    for id in frame {
                        let binding = self.context.scope.range(id);
                        for column in self.range_columns(id) {
                            result.push(self.resolved_column_candidate(
                                column,
                                Some(binding.exposed_name().to_owned()),
                                720,
                            ));
                        }
                    }
                } else {
                    let relation = QualifiedName {
                        name: qualifier.clone(),
                        ..QualifiedName::default()
                    };
                    for column in self.search(CatalogQuery::in_relation(
                        COLUMN_KINDS,
                        "",
                        &relation,
                        self.request.search_path,
                    )) {
                        result.push(self.catalog_candidate(column, 650));
                    }
                }
            }
            ColumnContext::JoinUsing => {
                let mut occurrences: HashMap<String, (String, usize)> = HashMap::new();
                let frame = self
                    .visible_range_frames()
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                for id in frame {
                    let mut seen_in_binding = HashSet::new();
                    for column in self.range_columns(id) {
                        let name = column.name;
                        let key = name.to_lowercase();
                        if seen_in_binding.insert(key.clone()) {
                            let entry = occurrences.entry(key).or_insert((name, 0));
                            entry.1 += 1;
                        }
                    }
                }
                for (_, (name, count)) in occurrences {
                    if count >= 2 {
                        result.push(self.column_candidate(name, None, None, None, 680));
                    }
                }
            }
            ColumnContext::TargetRelation => {
                if let Some(target) = self.context.scope.target_relation() {
                    for column in self.search(CatalogQuery::in_relation(
                        COLUMN_KINDS,
                        "",
                        &target.name,
                        self.request.search_path,
                    )) {
                        result.push(self.catalog_candidate(column, 680));
                    }
                }
            }
        }
    }

    fn visible_ctes(&self) -> Vec<CteBindingId> {
        let mut hidden = HashSet::new();
        let mut result = Vec::new();
        for frame in self.context.scope.frames() {
            for id in frame.ctes() {
                let name = self.context.scope.cte(*id).name.to_ascii_lowercase();
                if hidden.insert(name) {
                    result.push(*id);
                }
            }
        }
        result
    }

    fn visible_range_frames(&self) -> Vec<Vec<RangeBindingId>> {
        let mut hidden = HashSet::new();
        let mut result = Vec::new();
        for frame in self.context.scope.frames() {
            let mut visible = Vec::new();
            let mut frame_names = HashSet::new();
            for id in frame.ranges() {
                let name = self.context.scope.range(*id).exposed_name();
                if name.is_empty() || !hidden.contains(&name.to_ascii_lowercase()) {
                    visible.push(*id);
                    if !name.is_empty() {
                        frame_names.insert(name.to_ascii_lowercase());
                    }
                }
            }
            hidden.extend(frame_names);
            result.push(visible);
        }
        result
    }

    fn range_columns(&self, id: RangeBindingId) -> Vec<ResolvedColumn> {
        self.range_columns_guarded(id, &mut HashSet::new(), &mut HashSet::new())
    }

    fn range_columns_guarded(
        &self,
        id: RangeBindingId,
        visiting_ranges: &mut HashSet<RangeBindingId>,
        visiting_ctes: &mut HashSet<CteBindingId>,
    ) -> Vec<ResolvedColumn> {
        if !visiting_ranges.insert(id) {
            return Vec::new();
        }
        let binding = self.context.scope.range(id);
        let mut columns = match &binding.source {
            RangeSource::Relation(relation) => self.relation_columns(relation),
            RangeSource::Cte(cte) => self.cte_columns(*cte, visiting_ranges, visiting_ctes),
            RangeSource::Derived(shape) => {
                self.row_shape_columns(shape, visiting_ranges, visiting_ctes)
            }
            RangeSource::Function(function) => self.function_columns(function),
        };
        visiting_ranges.remove(&id);
        apply_column_aliases(&mut columns, &binding.column_aliases);
        columns
    }

    fn cte_columns(
        &self,
        id: CteBindingId,
        visiting_ranges: &mut HashSet<RangeBindingId>,
        visiting_ctes: &mut HashSet<CteBindingId>,
    ) -> Vec<ResolvedColumn> {
        let cte = self.context.scope.cte(id);
        if !visiting_ctes.insert(id) {
            return cte
                .column_aliases
                .iter()
                .cloned()
                .map(ResolvedColumn::local)
                .collect();
        }
        let mut columns = self.row_shape_columns(&cte.row_shape, visiting_ranges, visiting_ctes);
        visiting_ctes.remove(&id);
        apply_column_aliases(&mut columns, &cte.column_aliases);
        columns
    }

    fn row_shape_columns(
        &self,
        shape: &RowShape,
        visiting_ranges: &mut HashSet<RangeBindingId>,
        visiting_ctes: &mut HashSet<CteBindingId>,
    ) -> Vec<ResolvedColumn> {
        let mut result = Vec::new();
        for item in &shape.items {
            match item {
                RowShapeItem::Column { name, origin } => {
                    let mut column = match origin {
                        RowColumnOrigin::Expression => ResolvedColumn::local(name.clone()),
                        RowColumnOrigin::Column {
                            binding,
                            name: source_name,
                        } => {
                            let sources = binding
                                .map(|binding| vec![binding])
                                .unwrap_or_else(|| shape.sources.clone());
                            sources
                                .into_iter()
                                .find_map(|source| {
                                    self.range_columns_guarded(
                                        source,
                                        visiting_ranges,
                                        visiting_ctes,
                                    )
                                    .into_iter()
                                    .find(|column| column.name.eq_ignore_ascii_case(source_name))
                                })
                                .unwrap_or_else(|| ResolvedColumn::local(source_name.clone()))
                        }
                    };
                    column.rename(name);
                    result.push(column);
                }
                RowShapeItem::Wildcard { binding } => {
                    let sources = binding
                        .map(|binding| vec![binding])
                        .unwrap_or_else(|| shape.sources.clone());
                    for source in sources {
                        result.extend(self.range_columns_guarded(
                            source,
                            visiting_ranges,
                            visiting_ctes,
                        ));
                    }
                }
            }
        }
        result
    }

    fn relation_columns(&self, relation: &QualifiedName) -> Vec<ResolvedColumn> {
        self.search(CatalogQuery::in_relation(
            COLUMN_KINDS,
            "",
            relation,
            self.request.search_path,
        ))
        .into_iter()
        .map(ResolvedColumn::catalog)
        .collect()
    }

    fn function_columns(&self, function: &QualifiedName) -> Vec<ResolvedColumn> {
        self.search(CatalogQuery::in_schema(
            FUNCTION_KINDS,
            &function.name,
            function.schema.as_deref(),
            self.request.search_path,
        ))
        .into_iter()
        .filter(|item| item.identity.name.eq_ignore_ascii_case(&function.name))
        .flat_map(|item| item.row_shape.unwrap_or_default())
        .map(|column| ResolvedColumn {
            name: column.name,
            definition: column.definition,
            catalog_identity: None,
        })
        .collect()
    }

    fn column_candidate(
        &self,
        name: String,
        relation: Option<String>,
        definition: Option<String>,
        catalog_identity: Option<CatalogObjectIdentity>,
        score: i32,
    ) -> RankedCandidate {
        let detail = match (relation, definition) {
            (Some(relation), Some(definition)) => Some(format!("{relation}.{name} {definition}")),
            (Some(relation), None) => Some(format!("{relation}.{name}")),
            (None, definition) => definition,
        };
        RankedCandidate::new(
            CompletionItem {
                label: name.clone(),
                kind: CompletionKind::Column,
                catalog_identity,
                insert_text: quote_identifier(&name, self.quoted),
                detail,
                documentation: None,
            },
            score,
        )
    }

    fn resolved_column_candidate(
        &self,
        column: ResolvedColumn,
        relation: Option<String>,
        score: i32,
    ) -> RankedCandidate {
        self.column_candidate(
            column.name,
            relation,
            column.definition,
            column.catalog_identity,
            score,
        )
    }

    fn binding_candidate(
        &self,
        binding: &RangeBinding,
        kind: CompletionKind,
        score: i32,
    ) -> RankedCandidate {
        let label = binding.exposed_name().to_owned();
        RankedCandidate::new(
            CompletionItem {
                label: label.clone(),
                kind,
                catalog_identity: None,
                insert_text: quote_identifier(&label, self.quoted),
                detail: Some(
                    match binding.kind() {
                        RangeBindingKind::Cte => "common table expression",
                        RangeBindingKind::Derived => "derived table",
                        RangeBindingKind::Function => "table function",
                        RangeBindingKind::Relation => "relation",
                    }
                    .to_owned(),
                ),
                documentation: None,
            },
            score,
        )
    }

    fn cte_candidate(&self, cte: &CteBinding, score: i32) -> RankedCandidate {
        RankedCandidate::new(
            CompletionItem {
                label: cte.name.clone(),
                kind: CompletionKind::Cte,
                catalog_identity: None,
                insert_text: quote_identifier(&cte.name, self.quoted),
                detail: Some("common table expression".to_owned()),
                documentation: None,
            },
            score,
        )
    }

    fn search_catalog(
        &self,
        query: CatalogQuery<'_>,
        score: i32,
        result: &mut Vec<RankedCandidate>,
    ) {
        for item in self.search(query) {
            result.push(self.catalog_candidate(item, score));
        }
    }

    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        self.catalog.map_or_else(Vec::new, |catalog| {
            catalog
                .search(query)
                .into_iter()
                .filter(|item| catalog_item_matches_query(&query, item))
                .collect()
        })
    }

    fn catalog_candidate(&self, item: CatalogItem, score: i32) -> RankedCandidate {
        let identity = item.identity;
        let detail = match (identity.schema(), &item.definition) {
            (Some(schema), Some(definition)) => {
                Some(format!("{schema}.{} {definition}", identity.name))
            }
            (Some(schema), None) => Some(format!("{schema}.{}", identity.name)),
            (None, definition) => definition.clone(),
        };
        let schema_score = search_path_score(identity.schema(), self.request.search_path);
        RankedCandidate::new(
            CompletionItem {
                label: identity.name.clone(),
                kind: CompletionKind::Catalog(identity.kind),
                catalog_identity: Some(identity.clone()),
                insert_text: quote_identifier(&identity.name, self.quoted),
                detail,
                documentation: item.documentation,
            },
            score + schema_score,
        )
    }
}

fn catalog_item_matches_query(query: &CatalogQuery<'_>, item: &CatalogItem) -> bool {
    query.kinds.contains(&item.identity.kind)
        && prefix_matches(&item.identity.name, query.prefix, false)
        && match (&query.scope, &item.identity.namespace) {
            (CatalogQueryScope::Global, CatalogObjectNamespace::Global) => true,
            (CatalogQueryScope::Schema(None), CatalogObjectNamespace::Schema(_)) => true,
            (CatalogQueryScope::Schema(Some(expected)), CatalogObjectNamespace::Schema(actual)) => {
                actual.eq_ignore_ascii_case(expected)
            }
            (CatalogQueryScope::Relation(expected), CatalogObjectNamespace::Relation(actual)) => {
                qualified_names_equal(actual, expected)
            }
            _ => false,
        }
}

fn qualified_names_equal(left: &QualifiedName, right: &QualifiedName) -> bool {
    names_equal(&left.name, &right.name, false)
        && match (left.schema.as_deref(), right.schema.as_deref()) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            (Some(_), None) => true,
            (None, None) => true,
            _ => false,
        }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CompletionIdentity {
    Catalog(CatalogObjectIdentity),
    Local {
        kind: CompletionKind,
        label: String,
        detail: Option<String>,
    },
}

struct RankedCandidate {
    item: CompletionItem,
    score: i32,
}

impl RankedCandidate {
    fn new(item: CompletionItem, score: i32) -> Self {
        Self { item, score }
    }
}

fn prefix_matches(candidate: &str, prefix: &str, quoted: bool) -> bool {
    prefix.is_empty()
        || if quoted {
            candidate.starts_with(prefix)
        } else {
            candidate
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
        }
}

fn prefix_score(candidate: &str, prefix: &str, quoted: bool) -> i32 {
    if prefix.is_empty() {
        return 0;
    }
    if names_equal(candidate, prefix, quoted) {
        120
    } else if prefix_matches(candidate, prefix, quoted) {
        50
    } else {
        0
    }
}

fn names_equal(left: &str, right: &str, quoted: bool) -> bool {
    if quoted {
        left == right
    } else {
        left.eq_ignore_ascii_case(right)
    }
}

fn search_path_score(schema: Option<&str>, search_path: &[&str]) -> i32 {
    let Some(schema) = schema else {
        return 0;
    };
    search_path
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(schema))
        .map_or(0, |index| 80 - index.min(7) as i32 * 10)
}

fn quote_identifier(identifier: &str, force: bool) -> String {
    if force || needs_quoting(identifier) {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    } else {
        identifier.to_owned()
    }
}

fn needs_quoting(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return true;
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
        return true;
    }
    lookup_keyword(identifier)
        .is_some_and(|keyword| keyword.category == pg_parser::KeywordCategory::Reserved)
}

/// Simple catalog adapter useful for embedding and tests.
#[derive(Default)]
pub struct MemoryCatalog {
    items: Vec<CatalogItem>,
}

impl MemoryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, item: CatalogItem) {
        self.items.push(item);
    }

    pub fn add_schema(&mut self, name: impl Into<String>) {
        self.add(CatalogItem::new(CatalogObjectIdentity::global(
            CatalogObjectKind::Schema,
            name,
        )));
    }

    pub fn add_relation(
        &mut self,
        schema: impl Into<String>,
        name: impl Into<String>,
        kind: CatalogObjectKind,
        columns: impl IntoIterator<Item = (String, String)>,
    ) {
        let schema = schema.into();
        let name = name.into();
        self.add(CatalogItem::new(CatalogObjectIdentity::in_schema(
            kind,
            schema.clone(),
            name.clone(),
        )));
        let relation = QualifiedName {
            schema: Some(schema),
            name,
            ..QualifiedName::default()
        };
        for (name, definition) in columns {
            self.add(
                CatalogItem::new(CatalogObjectIdentity::owned_by_relation(
                    CatalogObjectKind::Column,
                    relation.clone(),
                    name,
                ))
                .with_definition(definition),
            );
        }
    }

    pub fn add_function(
        &mut self,
        schema: impl Into<String>,
        name: impl Into<String>,
        definition: impl Into<String>,
    ) {
        self.add(
            CatalogItem::new(CatalogObjectIdentity::in_schema(
                CatalogObjectKind::Function,
                schema,
                name,
            ))
            .with_definition(definition),
        );
    }

    pub fn add_table_function(
        &mut self,
        schema: impl Into<String>,
        name: impl Into<String>,
        definition: impl Into<String>,
        columns: impl IntoIterator<Item = CatalogColumn>,
    ) {
        self.add(
            CatalogItem::new(CatalogObjectIdentity::in_schema(
                CatalogObjectKind::Function,
                schema,
                name,
            ))
            .with_definition(definition)
            .with_row_shape(columns),
        );
    }

    pub fn add_type(&mut self, schema: impl Into<String>, name: impl Into<String>) {
        self.add(CatalogItem::new(CatalogObjectIdentity::in_schema(
            CatalogObjectKind::Type,
            schema,
            name,
        )));
    }
}

impl Catalog for MemoryCatalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        let resolved_relation_schema = match query.scope {
            CatalogQueryScope::Relation(relation) if relation.schema.is_none() => query
                .search_path
                .iter()
                .find(|schema| {
                    self.items.iter().any(|item| {
                        matches!(
                            &item.identity.namespace,
                            CatalogObjectNamespace::Relation(owner)
                                if owner.name.eq_ignore_ascii_case(&relation.name)
                                    && owner.schema.as_deref().is_some_and(|owner_schema| {
                                        owner_schema.eq_ignore_ascii_case(schema)
                                    })
                        )
                    })
                })
                .copied(),
            _ => None,
        };
        self.items
            .iter()
            .filter(|item| {
                catalog_item_matches_query(&query, item)
                    && resolved_relation_schema.is_none_or(|schema| {
                        matches!(
                            &item.identity.namespace,
                            CatalogObjectNamespace::Relation(owner)
                                if owner.schema.as_deref().is_some_and(|owner_schema| {
                                    owner_schema.eq_ignore_ascii_case(schema)
                                })
                        )
                    })
            })
            .cloned()
            .collect()
    }
}
