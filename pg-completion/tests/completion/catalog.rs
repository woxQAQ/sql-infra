use pg_completion::{CatalogObjectKind, CatalogObjectNamespace, CompletionKind, MemoryCatalog};

use super::support::Fixture;

#[test]
fn memory_catalog_supports_every_public_metadata_family() {
    let mut catalog = MemoryCatalog::new();
    catalog.add_schema("app");
    catalog.add_relation(
        "app",
        "customers",
        CatalogObjectKind::Table,
        [
            ("id".into(), "integer".into()),
            ("email".into(), "text".into()),
        ],
    );
    catalog.add_function("app", "customer_count", "customer_count() -> bigint");
    catalog.add_type("app", "customer_state");

    let fixture = Fixture::default();
    fixture
        .complete_with("SELECT * FROM ap|", Some(&catalog))
        .assert_has("app", CompletionKind::Schema);
    fixture
        .complete_with("SELECT * FROM app.cus|", Some(&catalog))
        .assert_has("customers", CompletionKind::Table);
    fixture
        .complete_with("SELECT c.em| FROM app.customers c", Some(&catalog))
        .assert_has("email", CompletionKind::Column)
        .assert_has_matching("email", CompletionKind::Column, |item| {
            item.catalog_identity.as_ref().is_some_and(|identity| {
                matches!(
                    &identity.namespace,
                    CatalogObjectNamespace::Relation(relation)
                        if relation.schema.as_deref() == Some("app")
                            && relation.name == "customers"
                )
            })
        });
    fixture
        .complete_with("SELECT app.customer_|", Some(&catalog))
        .assert_has("customer_count", CompletionKind::Function);
    fixture
        .complete_with("SELECT 1::customer_|", Some(&catalog))
        .assert_has("customer_state", CompletionKind::Type);
}

#[test]
fn memory_catalog_resolves_unqualified_columns_by_search_path() {
    let mut catalog = MemoryCatalog::new();
    catalog.add_relation(
        "public",
        "customers",
        CatalogObjectKind::Table,
        [("public_only".into(), "text".into())],
    );
    catalog.add_relation(
        "audit",
        "customers",
        CatalogObjectKind::View,
        [("audit_only".into(), "text".into())],
    );

    Fixture::default()
        .with_search_path(&["audit", "public"])
        .complete_with("SELECT c.| FROM customers c", Some(&catalog))
        .assert_kind_labels(CompletionKind::Column, &["audit_only"]);
    Fixture::default()
        .with_search_path(&["public", "audit"])
        .complete_with("SELECT c.| FROM customers c", Some(&catalog))
        .assert_kind_labels(CompletionKind::Column, &["public_only"]);
    Fixture::default()
        .with_search_path(&["public", "audit"])
        .complete_with("SELECT c.| FROM audit.customers c", Some(&catalog))
        .assert_kind_labels(CompletionKind::Column, &["audit_only"]);
}
