use std::collections::{HashMap, HashSet};

use pg_parser::{
    ColumnContext, CompletionContext, Expectation, NameExpectation, QualifiedName, RangeReference,
    RangeReferenceKind, TextRange, TextSize, TokenKind, collect_completion, keyword_text,
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
    pub insert_text: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompletionKind {
    Keyword,
    Schema,
    Table,
    View,
    MaterializedView,
    Column,
    Function,
    Type,
    Cte,
    Alias,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionError {
    Syntax(pg_parser::CompletionError),
}

impl std::fmt::Display for CompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CompletionError {}

impl From<pg_parser::CompletionError> for CompletionError {
    fn from(value: pg_parser::CompletionError) -> Self {
        Self::Syntax(value)
    }
}

/// Metadata seam consumed by the completion module.
///
/// Implementations may query a live database, a cache, or an in-memory test
/// catalog. The completion module owns visibility, filtering, and ranking.
pub trait Catalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem>;
}

#[derive(Clone, Copy, Debug)]
pub enum CatalogQuery<'a> {
    Schemas {
        prefix: &'a str,
    },
    Relations {
        prefix: &'a str,
        schema: Option<&'a str>,
        search_path: &'a [&'a str],
    },
    Columns {
        relation: &'a QualifiedName,
        search_path: &'a [&'a str],
    },
    Functions {
        prefix: &'a str,
        schema: Option<&'a str>,
        search_path: &'a [&'a str],
    },
    Types {
        prefix: &'a str,
        schema: Option<&'a str>,
        search_path: &'a [&'a str],
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogItem {
    pub name: String,
    pub schema: Option<String>,
    pub kind: CatalogItemKind,
    pub definition: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogItemKind {
    Schema,
    Table,
    View,
    MaterializedView,
    Column,
    Function,
    Type,
}

/// Complete SQL at `request.cursor`.
///
/// This is the crate's external interface: callers do not need to coordinate
/// parsing, scope resolution, prefix filtering, quoting, deduplication, or
/// ranking themselves.
pub fn complete(
    request: CompletionRequest<'_>,
    catalog: Option<&dyn Catalog>,
) -> Result<CompletionResult, CompletionError> {
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
            let key = (
                candidate.item.kind,
                candidate.item.label.to_lowercase(),
                candidate.item.detail.clone(),
            );
            seen.insert(key).then_some(candidate.item)
        })
        .collect();
    Ok(CompletionResult {
        replacement: context.replacement,
        items,
        is_incomplete: catalog.is_none()
            && context
                .expectations
                .iter()
                .any(|expectation| matches!(expectation, Expectation::Name(_))),
    })
}

struct Resolver<'a> {
    request: CompletionRequest<'a>,
    context: &'a CompletionContext,
    catalog: Option<&'a dyn Catalog>,
    quoted: bool,
}

impl Resolver<'_> {
    fn resolve(&self) -> Vec<RankedCandidate> {
        let mut result = Vec::new();
        for expectation in &self.context.expectations {
            match expectation {
                Expectation::Token(token) => self.resolve_token(*token, &mut result),
                Expectation::Name(expectation) => self.resolve_name(expectation, &mut result),
                Expectation::Expression => {}
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
                CatalogQuery::Schemas {
                    prefix: &self.context.prefix,
                },
                180,
                result,
            ),
            NameExpectation::Relation { schema } => {
                if schema.is_none() {
                    for cte in &self.context.scope.ctes {
                        result.push(self.reference_candidate(cte, CompletionKind::Cte, 520));
                    }
                }
                self.search_catalog(
                    CatalogQuery::Relations {
                        prefix: &self.context.prefix,
                        schema: schema.as_deref(),
                        search_path: self.request.search_path,
                    },
                    400,
                    result,
                );
            }
            NameExpectation::Column(context) => self.resolve_columns(context, result),
            NameExpectation::Function { schema } => self.search_catalog(
                CatalogQuery::Functions {
                    prefix: &self.context.prefix,
                    schema: schema.as_deref(),
                    search_path: self.request.search_path,
                },
                300,
                result,
            ),
            NameExpectation::Type { schema } => self.search_catalog(
                CatalogQuery::Types {
                    prefix: &self.context.prefix,
                    schema: schema.as_deref(),
                    search_path: self.request.search_path,
                },
                300,
                result,
            ),
        }
    }

    fn resolve_columns(&self, context: &ColumnContext, result: &mut Vec<RankedCandidate>) {
        match context {
            ColumnContext::VisibleScope => {
                for reference in &self.context.scope.references {
                    self.columns_for_reference(reference, 500, result);
                }
            }
            ColumnContext::Qualified(qualifier) => {
                if let Some(reference) =
                    self.context.scope.references.iter().find(|reference| {
                        names_equal(reference.exposed_name(), qualifier, self.quoted)
                    })
                {
                    self.columns_for_reference(reference, 720, result);
                } else {
                    let relation = QualifiedName {
                        name: qualifier.clone(),
                        ..QualifiedName::default()
                    };
                    for column in self.search(CatalogQuery::Columns {
                        relation: &relation,
                        search_path: self.request.search_path,
                    }) {
                        result.push(self.catalog_candidate(column, 650));
                    }
                }
            }
            ColumnContext::JoinUsing => {
                let mut occurrences: HashMap<String, (String, usize)> = HashMap::new();
                for reference in &self.context.scope.references {
                    for column in self.column_items(reference) {
                        let key = column.name.to_lowercase();
                        let entry = occurrences.entry(key).or_insert((column.name, 0));
                        entry.1 += 1;
                    }
                }
                for (_, (name, count)) in occurrences {
                    if count >= 2 {
                        result.push(self.column_candidate(name, None, None, 680));
                    }
                }
            }
            ColumnContext::TargetRelation => {
                if let Some(relation) = &self.context.scope.target_relation {
                    for column in self.search(CatalogQuery::Columns {
                        relation,
                        search_path: self.request.search_path,
                    }) {
                        result.push(self.catalog_candidate(column, 680));
                    }
                }
            }
        }
    }

    fn columns_for_reference(
        &self,
        reference: &RangeReference,
        score: i32,
        result: &mut Vec<RankedCandidate>,
    ) {
        if !reference.alias_columns.is_empty() {
            for column in &reference.alias_columns {
                result.push(self.column_candidate(
                    column.clone(),
                    Some(reference.exposed_name().to_owned()),
                    None,
                    score,
                ));
            }
            return;
        }
        if reference.kind == RangeReferenceKind::Cte {
            if let Some(cte) = self
                .context
                .scope
                .ctes
                .iter()
                .find(|cte| names_equal(&cte.name.name, &reference.name.name, false))
            {
                for column in &cte.alias_columns {
                    result.push(self.column_candidate(
                        column.clone(),
                        Some(reference.exposed_name().to_owned()),
                        None,
                        score + 30,
                    ));
                }
            }
            return;
        }
        for column in self.column_items(reference) {
            result.push(self.column_candidate(
                column.name,
                Some(reference.exposed_name().to_owned()),
                column.definition,
                score,
            ));
        }
    }

    fn column_items(&self, reference: &RangeReference) -> Vec<CatalogItem> {
        if !reference.alias_columns.is_empty() {
            return reference
                .alias_columns
                .iter()
                .map(|name| CatalogItem {
                    name: name.clone(),
                    schema: None,
                    kind: CatalogItemKind::Column,
                    definition: None,
                    documentation: None,
                })
                .collect();
        }
        self.search(CatalogQuery::Columns {
            relation: &reference.name,
            search_path: self.request.search_path,
        })
    }

    fn column_candidate(
        &self,
        name: String,
        relation: Option<String>,
        definition: Option<String>,
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
                insert_text: quote_identifier(&name, self.quoted),
                detail,
                documentation: None,
            },
            score,
        )
    }

    fn reference_candidate(
        &self,
        reference: &RangeReference,
        kind: CompletionKind,
        score: i32,
    ) -> RankedCandidate {
        let label = reference.exposed_name().to_owned();
        RankedCandidate::new(
            CompletionItem {
                label: label.clone(),
                kind,
                insert_text: quote_identifier(&label, self.quoted),
                detail: Some(
                    match reference.kind {
                        RangeReferenceKind::Cte => "common table expression",
                        RangeReferenceKind::Subquery => "subquery",
                        RangeReferenceKind::Function => "table function",
                        RangeReferenceKind::Relation => "relation",
                    }
                    .to_owned(),
                ),
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
        self.catalog
            .map_or_else(Vec::new, |catalog| catalog.search(query))
    }

    fn catalog_candidate(&self, item: CatalogItem, score: i32) -> RankedCandidate {
        let kind = match item.kind {
            CatalogItemKind::Schema => CompletionKind::Schema,
            CatalogItemKind::Table => CompletionKind::Table,
            CatalogItemKind::View => CompletionKind::View,
            CatalogItemKind::MaterializedView => CompletionKind::MaterializedView,
            CatalogItemKind::Column => CompletionKind::Column,
            CatalogItemKind::Function => CompletionKind::Function,
            CatalogItemKind::Type => CompletionKind::Type,
        };
        let detail = match (&item.schema, &item.definition) {
            (Some(schema), Some(definition)) => {
                Some(format!("{schema}.{} {definition}", item.name))
            }
            (Some(schema), None) => Some(format!("{schema}.{}", item.name)),
            (None, definition) => definition.clone(),
        };
        RankedCandidate::new(
            CompletionItem {
                label: item.name.clone(),
                kind,
                insert_text: quote_identifier(&item.name, self.quoted),
                detail,
                documentation: item.documentation,
            },
            score + search_path_score(item.schema.as_deref(), self.request.search_path),
        )
    }
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
    schemas: Vec<CatalogItem>,
    relations: Vec<CatalogItem>,
    functions: Vec<CatalogItem>,
    types: Vec<CatalogItem>,
    columns: HashMap<(Option<String>, String), Vec<CatalogItem>>,
}

impl MemoryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_schema(&mut self, name: impl Into<String>) {
        self.schemas.push(CatalogItem {
            name: name.into(),
            schema: None,
            kind: CatalogItemKind::Schema,
            definition: None,
            documentation: None,
        });
    }

    pub fn add_relation(
        &mut self,
        schema: impl Into<String>,
        name: impl Into<String>,
        kind: CatalogItemKind,
        columns: impl IntoIterator<Item = (String, String)>,
    ) {
        let schema = schema.into();
        let name = name.into();
        self.relations.push(CatalogItem {
            name: name.clone(),
            schema: Some(schema.clone()),
            kind,
            definition: None,
            documentation: None,
        });
        self.columns.insert(
            (Some(schema), name),
            columns
                .into_iter()
                .map(|(name, definition)| CatalogItem {
                    name,
                    schema: None,
                    kind: CatalogItemKind::Column,
                    definition: Some(definition),
                    documentation: None,
                })
                .collect(),
        );
    }

    pub fn add_function(
        &mut self,
        schema: impl Into<String>,
        name: impl Into<String>,
        definition: impl Into<String>,
    ) {
        self.functions.push(CatalogItem {
            name: name.into(),
            schema: Some(schema.into()),
            kind: CatalogItemKind::Function,
            definition: Some(definition.into()),
            documentation: None,
        });
    }

    pub fn add_type(&mut self, schema: impl Into<String>, name: impl Into<String>) {
        self.types.push(CatalogItem {
            name: name.into(),
            schema: Some(schema.into()),
            kind: CatalogItemKind::Type,
            definition: None,
            documentation: None,
        });
    }
}

impl Catalog for MemoryCatalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        match query {
            CatalogQuery::Schemas { prefix } => filter_items(&self.schemas, prefix),
            CatalogQuery::Relations { prefix, schema, .. } => {
                filter_items_in_schema(&self.relations, prefix, schema)
            }
            CatalogQuery::Columns {
                relation,
                search_path,
            } => self.columns_for_relation(relation, search_path),
            CatalogQuery::Functions { prefix, schema, .. } => {
                filter_items_in_schema(&self.functions, prefix, schema)
            }
            CatalogQuery::Types { prefix, schema, .. } => {
                filter_items_in_schema(&self.types, prefix, schema)
            }
        }
    }
}

impl MemoryCatalog {
    fn columns_for_relation(
        &self,
        relation: &QualifiedName,
        search_path: &[&str],
    ) -> Vec<CatalogItem> {
        let columns = if let Some(schema) = relation.schema.as_deref() {
            self.find_columns(Some(schema), &relation.name)
        } else {
            search_path
                .iter()
                .find_map(|schema| self.find_columns(Some(schema), &relation.name))
                .or_else(|| self.find_columns(None, &relation.name))
        };
        columns.cloned().unwrap_or_default()
    }

    fn find_columns(&self, schema: Option<&str>, relation: &str) -> Option<&Vec<CatalogItem>> {
        self.columns
            .iter()
            .find_map(|((candidate_schema, candidate_relation), columns)| {
                let schema_matches = match (candidate_schema.as_deref(), schema) {
                    (Some(candidate), Some(expected)) => candidate.eq_ignore_ascii_case(expected),
                    (None, None) => true,
                    _ => false,
                };
                (schema_matches && candidate_relation.eq_ignore_ascii_case(relation))
                    .then_some(columns)
            })
    }
}

fn filter_items(items: &[CatalogItem], prefix: &str) -> Vec<CatalogItem> {
    items
        .iter()
        .filter(|item| prefix_matches(&item.name, prefix, false))
        .cloned()
        .collect()
}

fn filter_items_in_schema(
    items: &[CatalogItem],
    prefix: &str,
    schema: Option<&str>,
) -> Vec<CatalogItem> {
    items
        .iter()
        .filter(|item| {
            prefix_matches(&item.name, prefix, false)
                && schema.is_none_or(|schema| {
                    item.schema
                        .as_deref()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(schema))
                })
        })
        .cloned()
        .collect()
}
