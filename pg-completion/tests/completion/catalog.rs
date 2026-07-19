use pg_completion::{CatalogItemKind, CompletionKind, MemoryCatalog};

use super::support::Fixture;

#[test]
fn memory_catalog_supports_every_public_metadata_family() {
    let mut catalog = MemoryCatalog::new();
    catalog.add_schema("app");
    catalog.add_relation(
        "app",
        "customers",
        CatalogItemKind::Table,
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
        .assert_has("email", CompletionKind::Column);
    fixture
        .complete_with("SELECT app.customer_|", Some(&catalog))
        .assert_has("customer_count", CompletionKind::Function);
    fixture
        .complete_with("SELECT 1::customer_|", Some(&catalog))
        .assert_has("customer_state", CompletionKind::Type);
}
