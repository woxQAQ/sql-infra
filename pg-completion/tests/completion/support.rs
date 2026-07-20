use std::collections::HashMap;

use pg_completion::{
    Catalog, CatalogItem, CatalogItemKind, CatalogQuery, CompletionError, CompletionItem,
    CompletionKind, CompletionRequest, CompletionResult, complete,
};
use pg_parser::{QualifiedName, TextSize};

#[derive(Default)]
pub struct TestCatalog {
    schemas: Vec<CatalogItem>,
    relations: Vec<CatalogItem>,
    functions: Vec<CatalogItem>,
    types: Vec<CatalogItem>,
    columns: HashMap<(Option<String>, String), Vec<CatalogItem>>,
}

impl TestCatalog {
    pub fn standard() -> Self {
        let mut catalog = Self::default();
        catalog.schema("public");
        catalog.schema("audit");
        catalog.schema("pg_catalog");
        catalog.relation(
            "public",
            "users",
            CatalogItemKind::Table,
            &[("id", "integer"), ("name", "text"), ("select", "text")],
        );
        catalog.relation(
            "public",
            "orders",
            CatalogItemKind::Table,
            &[
                ("id", "integer"),
                ("user_id", "integer"),
                ("amount", "numeric"),
            ],
        );
        catalog.relation(
            "audit",
            "active_users",
            CatalogItemKind::View,
            &[("id", "integer"), ("name", "text")],
        );
        catalog.relation(
            "audit",
            "recent_orders",
            CatalogItemKind::MaterializedView,
            &[("id", "integer"), ("amount", "numeric")],
        );
        catalog.relation(
            "public",
            "UserProfile",
            CatalogItemKind::Table,
            &[("DisplayName", "text")],
        );
        catalog.function(
            "pg_catalog",
            "count",
            "count(any) -> bigint",
            Some("number of input rows"),
        );
        catalog.function(
            "public",
            "calculate_total",
            "calculate_total(numeric) -> numeric",
            None,
        );
        catalog.ty("pg_catalog", "integer");
        catalog.ty("public", "order_status");
        catalog
    }

    pub fn schema(&mut self, name: &str) {
        self.schemas
            .push(item(name, None, CatalogItemKind::Schema, None, None));
    }

    pub fn relation(
        &mut self,
        schema: &str,
        name: &str,
        kind: CatalogItemKind,
        columns: &[(&str, &str)],
    ) {
        self.relations
            .push(item(name, Some(schema), kind, None, None));
        self.columns.insert(
            (Some(schema.to_owned()), name.to_owned()),
            columns
                .iter()
                .map(|(name, definition)| {
                    item(name, None, CatalogItemKind::Column, Some(definition), None)
                })
                .collect(),
        );
    }

    pub fn function(
        &mut self,
        schema: &str,
        name: &str,
        definition: &str,
        documentation: Option<&str>,
    ) {
        self.functions.push(item(
            name,
            Some(schema),
            CatalogItemKind::Function,
            Some(definition),
            documentation,
        ));
    }

    pub fn ty(&mut self, schema: &str, name: &str) {
        self.types
            .push(item(name, Some(schema), CatalogItemKind::Type, None, None));
    }

    pub fn duplicate_relation(&mut self, schema: &str, name: &str, kind: CatalogItemKind) {
        self.relations
            .push(item(name, Some(schema), kind, None, None));
    }

    pub fn duplicate_function(&mut self, schema: &str, name: &str, definition: &str) {
        self.functions.push(item(
            name,
            Some(schema),
            CatalogItemKind::Function,
            Some(definition),
            None,
        ));
    }

    pub fn duplicate_type(&mut self, schema: &str, name: &str) {
        self.types
            .push(item(name, Some(schema), CatalogItemKind::Type, None, None));
    }
}

impl Catalog for TestCatalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        match query {
            CatalogQuery::Schemas { prefix } => filter(&self.schemas, prefix, None),
            CatalogQuery::Relations { prefix, schema, .. } => {
                filter(&self.relations, prefix, schema)
            }
            CatalogQuery::Columns {
                relation,
                search_path,
            } => self.columns(relation, search_path),
            CatalogQuery::Functions { prefix, schema, .. } => {
                filter(&self.functions, prefix, schema)
            }
            CatalogQuery::Types { prefix, schema, .. } => filter(&self.types, prefix, schema),
        }
    }
}

impl TestCatalog {
    fn columns(&self, relation: &QualifiedName, search_path: &[&str]) -> Vec<CatalogItem> {
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

pub struct Fixture {
    pub catalog: TestCatalog,
    search_path: Vec<&'static str>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            catalog: TestCatalog::standard(),
            search_path: vec!["public", "pg_catalog"],
        }
    }
}

impl Fixture {
    pub fn with_search_path(mut self, search_path: &[&'static str]) -> Self {
        self.search_path = search_path.to_vec();
        self
    }

    pub fn complete(&self, marked: &str) -> Completed {
        self.complete_with_catalog(marked, Some(&self.catalog))
    }

    pub fn complete_without_catalog(&self, marked: &str) -> Completed {
        self.complete_with_catalog(marked, None)
    }

    pub fn complete_with(&self, marked: &str, catalog: Option<&dyn Catalog>) -> Completed {
        self.complete_with_catalog(marked, catalog)
    }

    pub fn error_without_catalog(&self, sql: &str, cursor: usize) -> CompletionError {
        complete(
            CompletionRequest {
                sql,
                cursor: TextSize::try_from(cursor).expect("test cursor fits in TextSize"),
                search_path: &self.search_path,
            },
            None,
        )
        .expect_err("completion request should fail")
    }

    fn complete_with_catalog(&self, marked: &str, catalog: Option<&dyn Catalog>) -> Completed {
        let (sql, cursor) = marked_sql(marked);
        let result = complete(
            CompletionRequest {
                sql: &sql,
                cursor: TextSize::try_from(cursor).expect("test cursor fits in TextSize"),
                search_path: &self.search_path,
            },
            catalog,
        )
        .unwrap_or_else(|error| panic!("completion failed for {marked:?}: {error}"));
        Completed {
            marked: marked.to_owned(),
            sql,
            cursor,
            result,
        }
    }
}

pub struct Completed {
    marked: String,
    sql: String,
    cursor: usize,
    pub result: CompletionResult,
}

impl Completed {
    pub fn assert_has(&self, label: &str, kind: CompletionKind) -> &Self {
        assert!(
            self.find(label, kind).is_some(),
            "expected {label:?} ({kind:?}) for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_lacks(&self, label: &str, kind: CompletionKind) -> &Self {
        assert!(
            self.find(label, kind).is_none(),
            "did not expect {label:?} ({kind:?}) for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_lacks_kind(&self, kind: CompletionKind) -> &Self {
        let unexpected = self
            .result
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert!(
            unexpected.is_empty(),
            "did not expect {kind:?} candidates for {:?}; got {unexpected:?}",
            self.marked
        );
        self
    }

    pub fn assert_value_expression(&self) -> &Self {
        self.assert_has("count", CompletionKind::Function)
            .assert_has("NULL", CompletionKind::Keyword)
            .assert_lacks_kind(CompletionKind::Schema)
            .assert_lacks_kind(CompletionKind::Table)
            .assert_lacks_kind(CompletionKind::View)
            .assert_lacks_kind(CompletionKind::MaterializedView)
            .assert_lacks_kind(CompletionKind::Type)
            .assert_lacks("LATERAL", CompletionKind::Keyword)
    }

    pub fn assert_required_value_expression(&self) -> &Self {
        self.assert_value_expression()
            .assert_lacks("FROM", CompletionKind::Keyword)
            .assert_lacks("WHERE", CompletionKind::Keyword)
            .assert_lacks("GROUP", CompletionKind::Keyword)
            .assert_lacks("HAVING", CompletionKind::Keyword)
            .assert_lacks("ORDER", CompletionKind::Keyword)
            .assert_lacks("LIMIT", CompletionKind::Keyword)
            .assert_lacks("OFFSET", CompletionKind::Keyword)
            .assert_lacks("RETURNING", CompletionKind::Keyword)
    }

    pub fn assert_visible_value_expression(&self) -> &Self {
        self.assert_value_expression()
            .assert_has("name", CompletionKind::Column)
    }

    pub fn assert_required_visible_value_expression(&self) -> &Self {
        self.assert_required_value_expression()
            .assert_has("name", CompletionKind::Column)
    }

    pub fn assert_no_duplicate_items(&self) -> &Self {
        let mut seen = std::collections::HashSet::new();
        for item in &self.result.items {
            let identity = (
                item.kind,
                item.label.to_ascii_lowercase(),
                item.detail.as_deref(),
            );
            assert!(
                seen.insert(identity),
                "duplicate completion item for {:?}: {:?}",
                self.marked,
                item
            );
        }
        self
    }

    pub fn assert_prefix_filtered(&self, prefix: &str, case_sensitive: bool) -> &Self {
        for item in &self.result.items {
            let matches = if case_sensitive {
                item.label.starts_with(prefix)
            } else {
                item.label
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
            };
            assert!(
                matches,
                "completion item {:?} does not match prefix {prefix:?} for {:?}",
                item, self.marked
            );
        }
        self
    }

    pub fn assert_replacement_contract(&self) -> &Self {
        let start = usize::from(self.result.replacement.start());
        let end = usize::from(self.result.replacement.end());
        assert!(
            start <= end,
            "invalid replacement range for {:?}",
            self.marked
        );
        assert!(
            end <= self.sql.len(),
            "replacement exceeds source for {:?}",
            self.marked
        );
        assert!(
            self.sql.is_char_boundary(start) && self.sql.is_char_boundary(end),
            "replacement is not on UTF-8 boundaries for {:?}",
            self.marked
        );
        assert!(
            start <= self.cursor && self.cursor <= end,
            "replacement does not contain cursor for {:?}: {start}..{end}, cursor {}",
            self.marked,
            self.cursor
        );
        self
    }

    pub fn assert_first(&self, label: &str, kind: CompletionKind) -> &Self {
        let first = self
            .result
            .items
            .first()
            .unwrap_or_else(|| panic!("no completion items for {:?}", self.marked));
        assert_eq!(
            (&*first.label, first.kind),
            (label, kind),
            "{:?}",
            self.marked
        );
        self
    }

    pub fn assert_incomplete(&self, expected: bool) -> &Self {
        assert_eq!(self.result.is_incomplete, expected, "{:?}", self.marked);
        self
    }

    pub fn assert_replaces(&self, expected: &str) -> &Self {
        let start = usize::from(self.result.replacement.start());
        let end = usize::from(self.result.replacement.end());
        assert_eq!(&self.sql[start..end], expected, "{:?}", self.marked);
        self
    }

    pub fn item(&self, label: &str, kind: CompletionKind) -> &CompletionItem {
        self.find(label, kind).unwrap_or_else(|| {
            panic!(
                "missing {label:?} ({kind:?}) for {:?}; got {:?}",
                self.marked,
                self.summary()
            )
        })
    }

    pub fn count(&self, label: &str, kind: CompletionKind) -> usize {
        self.result
            .items
            .iter()
            .filter(|item| item.label == label && item.kind == kind)
            .count()
    }

    pub fn assert_kind_labels(&self, kind: CompletionKind, expected: &[&str]) -> &Self {
        let actual = self
            .result
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{:?}", self.marked);
        self
    }

    fn find(&self, label: &str, kind: CompletionKind) -> Option<&CompletionItem> {
        self.result
            .items
            .iter()
            .find(|item| item.label == label && item.kind == kind)
    }

    fn summary(&self) -> Vec<(&str, CompletionKind)> {
        self.result
            .items
            .iter()
            .map(|item| (item.label.as_str(), item.kind))
            .collect()
    }
}

fn marked_sql(marked: &str) -> (String, usize) {
    let mut markers = marked.match_indices('|');
    let (cursor, _) = markers
        .next()
        .unwrap_or_else(|| panic!("completion test must contain one '|' marker: {marked:?}"));
    assert!(
        markers.next().is_none(),
        "completion test must contain exactly one '|' marker: {marked:?}"
    );
    (marked.replacen('|', "", 1), cursor)
}

fn item(
    name: &str,
    schema: Option<&str>,
    kind: CatalogItemKind,
    definition: Option<&str>,
    documentation: Option<&str>,
) -> CatalogItem {
    CatalogItem {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        kind,
        definition: definition.map(str::to_owned),
        documentation: documentation.map(str::to_owned),
    }
}

fn filter(items: &[CatalogItem], prefix: &str, schema: Option<&str>) -> Vec<CatalogItem> {
    items
        .iter()
        .filter(|item| {
            item.name
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
                && schema.is_none_or(|schema| {
                    item.schema
                        .as_deref()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(schema))
                })
        })
        .cloned()
        .collect()
}
