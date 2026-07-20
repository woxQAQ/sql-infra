use std::cell::RefCell;

use pg_completion::{
    Catalog, CatalogItem, CatalogObjectIdentity, CatalogObjectKind, CatalogQuery,
    CatalogQueryScope, CompletionKind,
};
use pg_parser::QualifiedName;

use super::support::Fixture;

#[derive(Debug, Eq, PartialEq)]
struct ObservedQuery {
    kinds: Vec<CatalogObjectKind>,
    prefix: String,
    scope: ObservedScope,
    search_path: Vec<String>,
}

#[derive(Debug, Eq, PartialEq)]
enum ObservedScope {
    Global,
    Schema(Option<String>),
    Relation(QualifiedName),
}

#[derive(Default)]
struct ProbeCatalog {
    items: Vec<CatalogItem>,
    queries: RefCell<Vec<ObservedQuery>>,
}

impl ProbeCatalog {
    fn returning(items: impl IntoIterator<Item = CatalogItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            queries: RefCell::default(),
        }
    }

    fn recorded(&self, expected: &ObservedQuery) -> bool {
        self.queries.borrow().contains(expected)
    }
}

impl Catalog for ProbeCatalog {
    fn search(&self, query: CatalogQuery<'_>) -> Vec<CatalogItem> {
        let scope = match query.scope {
            CatalogQueryScope::Global => ObservedScope::Global,
            CatalogQueryScope::Schema(schema) => ObservedScope::Schema(schema.map(str::to_owned)),
            CatalogQueryScope::Relation(relation) => ObservedScope::Relation(relation.clone()),
        };
        let observed = ObservedQuery {
            kinds: query.kinds.to_vec(),
            prefix: query.prefix.to_owned(),
            scope,
            search_path: owned_path(query.search_path),
        };
        self.queries.borrow_mut().push(observed);
        self.items.clone()
    }
}

#[test]
fn completion_filters_results_from_an_unfiltered_catalog() {
    let catalog = ProbeCatalog::returning([
        item("users", "public", CatalogObjectKind::Table),
        item("orders", "public", CatalogObjectKind::Table),
    ]);

    Fixture::default()
        .complete_with("SELECT * FROM us|", Some(&catalog))
        .assert_kind_labels(CompletionKind::Table, &["users"]);
}

#[test]
fn completion_prefix_filters_every_catalog_query_family() {
    let fixture = Fixture::default();

    let schemas = ProbeCatalog::returning([
        CatalogItem::new(CatalogObjectIdentity::global(
            CatalogObjectKind::Schema,
            "public",
        )),
        CatalogItem::new(CatalogObjectIdentity::global(
            CatalogObjectKind::Schema,
            "audit",
        )),
    ]);
    fixture
        .complete_with("SELECT * FROM pub|", Some(&schemas))
        .assert_kind_labels(CompletionKind::Schema, &["public"]);

    let columns = ProbeCatalog::returning([
        column("name", "public", "users"),
        column("amount", "public", "users"),
    ]);
    fixture
        .complete_with("SELECT na| FROM users", Some(&columns))
        .assert_kind_labels(CompletionKind::Column, &["name"]);

    let functions = ProbeCatalog::returning([
        item("count", "pg_catalog", CatalogObjectKind::Function),
        item("calculate_total", "public", CatalogObjectKind::Function),
    ]);
    fixture
        .complete_with("SELECT cou|", Some(&functions))
        .assert_kind_labels(CompletionKind::Function, &["count"]);

    let types = ProbeCatalog::returning([
        item("integer", "pg_catalog", CatalogObjectKind::Type),
        item("interval", "pg_catalog", CatalogObjectKind::Type),
    ]);
    fixture
        .complete_with("SELECT 1::integ|", Some(&types))
        .assert_kind_labels(CompletionKind::Type, &["integer"]);
}

#[test]
fn catalog_queries_preserve_prefix_qualification_and_search_path() {
    let catalog = ProbeCatalog::default();
    let fixture = Fixture::default().with_search_path(&["app", "pg_catalog"]);

    fixture.complete_with("SELECT * FROM audit.us|", Some(&catalog));
    fixture.complete_with("SELECT u.na| FROM users u", Some(&catalog));
    fixture.complete_with("SELECT pg_catalog.cou|", Some(&catalog));
    fixture.complete_with("SELECT 1::pg_catalog.inte|", Some(&catalog));

    let path = vec!["app".to_owned(), "pg_catalog".to_owned()];
    assert!(catalog.recorded(&ObservedQuery {
        kinds: vec![
            CatalogObjectKind::Table,
            CatalogObjectKind::View,
            CatalogObjectKind::MaterializedView,
            CatalogObjectKind::ForeignTable,
            CatalogObjectKind::Sequence,
        ],
        prefix: "us".to_owned(),
        scope: ObservedScope::Schema(Some("audit".to_owned())),
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery {
        kinds: vec![CatalogObjectKind::Column],
        prefix: String::new(),
        scope: ObservedScope::Relation(QualifiedName {
            name: "users".to_owned(),
            ..QualifiedName::default()
        }),
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery {
        kinds: vec![CatalogObjectKind::Function, CatalogObjectKind::Aggregate],
        prefix: "cou".to_owned(),
        scope: ObservedScope::Schema(Some("pg_catalog".to_owned())),
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery {
        kinds: vec![CatalogObjectKind::Type, CatalogObjectKind::Domain],
        prefix: "inte".to_owned(),
        scope: ObservedScope::Schema(Some("pg_catalog".to_owned())),
        search_path: path,
    }));
}

fn owned_path(search_path: &[&str]) -> Vec<String> {
    search_path
        .iter()
        .map(|schema| (*schema).to_owned())
        .collect()
}

fn item(name: &str, schema: &str, kind: CatalogObjectKind) -> CatalogItem {
    CatalogItem::new(CatalogObjectIdentity::in_schema(kind, schema, name))
}

fn column(name: &str, schema: &str, relation: &str) -> CatalogItem {
    CatalogItem::new(CatalogObjectIdentity::owned_by_relation(
        CatalogObjectKind::Column,
        QualifiedName {
            schema: Some(schema.to_owned()),
            name: relation.to_owned(),
            ..QualifiedName::default()
        },
        name,
    ))
}
