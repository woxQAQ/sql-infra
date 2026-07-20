use std::cell::RefCell;

use pg_completion::{Catalog, CatalogItem, CatalogItemKind, CatalogQuery, CompletionKind};

use super::support::Fixture;

#[derive(Debug, Eq, PartialEq)]
enum ObservedQuery {
    Schemas {
        prefix: String,
    },
    Relations {
        prefix: String,
        schema: Option<String>,
        search_path: Vec<String>,
    },
    Columns {
        relation: String,
        schema: Option<String>,
        search_path: Vec<String>,
    },
    Functions {
        prefix: String,
        schema: Option<String>,
        search_path: Vec<String>,
    },
    Types {
        prefix: String,
        schema: Option<String>,
        search_path: Vec<String>,
    },
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
        let observed = match query {
            CatalogQuery::Schemas { prefix } => ObservedQuery::Schemas {
                prefix: prefix.to_owned(),
            },
            CatalogQuery::Relations {
                prefix,
                schema,
                search_path,
            } => ObservedQuery::Relations {
                prefix: prefix.to_owned(),
                schema: schema.map(str::to_owned),
                search_path: owned_path(search_path),
            },
            CatalogQuery::Columns {
                relation,
                search_path,
            } => ObservedQuery::Columns {
                relation: relation.name.clone(),
                schema: relation.schema.clone(),
                search_path: owned_path(search_path),
            },
            CatalogQuery::Functions {
                prefix,
                schema,
                search_path,
            } => ObservedQuery::Functions {
                prefix: prefix.to_owned(),
                schema: schema.map(str::to_owned),
                search_path: owned_path(search_path),
            },
            CatalogQuery::Types {
                prefix,
                schema,
                search_path,
            } => ObservedQuery::Types {
                prefix: prefix.to_owned(),
                schema: schema.map(str::to_owned),
                search_path: owned_path(search_path),
            },
        };
        self.queries.borrow_mut().push(observed);
        self.items.clone()
    }
}

#[test]
fn completion_filters_results_from_an_unfiltered_catalog() {
    let catalog = ProbeCatalog::returning([
        item("users", "public", CatalogItemKind::Table),
        item("orders", "public", CatalogItemKind::Table),
    ]);

    Fixture::default()
        .complete_with("SELECT * FROM us|", Some(&catalog))
        .assert_kind_labels(CompletionKind::Table, &["users"]);
}

#[test]
fn completion_prefix_filters_every_catalog_query_family() {
    let fixture = Fixture::default();

    let schemas = ProbeCatalog::returning([
        CatalogItem {
            name: "public".into(),
            schema: None,
            kind: CatalogItemKind::Schema,
            definition: None,
            documentation: None,
        },
        CatalogItem {
            name: "audit".into(),
            schema: None,
            kind: CatalogItemKind::Schema,
            definition: None,
            documentation: None,
        },
    ]);
    fixture
        .complete_with("SELECT * FROM pub|", Some(&schemas))
        .assert_kind_labels(CompletionKind::Schema, &["public"]);

    let columns = ProbeCatalog::returning([
        item("name", "public", CatalogItemKind::Column),
        item("amount", "public", CatalogItemKind::Column),
    ]);
    fixture
        .complete_with("SELECT na| FROM users", Some(&columns))
        .assert_kind_labels(CompletionKind::Column, &["name"]);

    let functions = ProbeCatalog::returning([
        item("count", "pg_catalog", CatalogItemKind::Function),
        item("calculate_total", "public", CatalogItemKind::Function),
    ]);
    fixture
        .complete_with("SELECT cou|", Some(&functions))
        .assert_kind_labels(CompletionKind::Function, &["count"]);

    let types = ProbeCatalog::returning([
        item("integer", "pg_catalog", CatalogItemKind::Type),
        item("interval", "pg_catalog", CatalogItemKind::Type),
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
    assert!(catalog.recorded(&ObservedQuery::Relations {
        prefix: "us".to_owned(),
        schema: Some("audit".to_owned()),
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery::Columns {
        relation: "users".to_owned(),
        schema: None,
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery::Functions {
        prefix: "cou".to_owned(),
        schema: Some("pg_catalog".to_owned()),
        search_path: path.clone(),
    }));
    assert!(catalog.recorded(&ObservedQuery::Types {
        prefix: "inte".to_owned(),
        schema: Some("pg_catalog".to_owned()),
        search_path: path,
    }));
}

fn owned_path(search_path: &[&str]) -> Vec<String> {
    search_path
        .iter()
        .map(|schema| (*schema).to_owned())
        .collect()
}

fn item(name: &str, schema: &str, kind: CatalogItemKind) -> CatalogItem {
    CatalogItem {
        name: name.to_owned(),
        schema: Some(schema.to_owned()),
        kind,
        definition: None,
        documentation: None,
    }
}
