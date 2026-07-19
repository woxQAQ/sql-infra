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
}

impl Catalog for TestCatalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        match query {
            CatalogQuery::Schemas { prefix } => filter(&self.schemas, prefix, None),
            CatalogQuery::Relations { prefix, schema, .. } => {
                filter(&self.relations, prefix, schema)
            }
            CatalogQuery::Columns { relation } => self.columns(relation),
            CatalogQuery::Functions { prefix, schema, .. } => {
                filter(&self.functions, prefix, schema)
            }
            CatalogQuery::Types { prefix, schema, .. } => filter(&self.types, prefix, schema),
        }
    }
}

impl TestCatalog {
    fn columns(&self, relation: &QualifiedName) -> Vec<CatalogItem> {
        self.columns
            .get(&(relation.schema.clone(), relation.name.clone()))
            .or_else(|| {
                self.columns.iter().find_map(|((_, name), columns)| {
                    name.eq_ignore_ascii_case(&relation.name).then_some(columns)
                })
            })
            .cloned()
            .unwrap_or_default()
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
            result,
        }
    }
}

pub struct Completed {
    marked: String,
    sql: String,
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
