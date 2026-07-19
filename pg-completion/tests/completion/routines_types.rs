use pg_completion::CompletionKind;

use super::support::Fixture;

#[test]
fn functions_complete_in_expression_slots_with_metadata() {
    let fixture = Fixture::default();
    let result = fixture.complete("SELECT cou| FROM users");
    let item = result.item("count", CompletionKind::Function);
    assert_eq!(
        item.detail.as_deref(),
        Some("pg_catalog.count count(any) -> bigint")
    );
    assert_eq!(item.documentation.as_deref(), Some("number of input rows"));
}

#[test]
fn schema_qualified_functions_do_not_leak_other_schemas() {
    Fixture::default()
        .complete("SELECT pg_catalog.cou| FROM users")
        .assert_has("count", CompletionKind::Function)
        .assert_lacks("calculate_total", CompletionKind::Function);
}

#[test]
fn types_complete_in_cast_and_ddl_slots() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT id::inte| FROM users",
        "SELECT CAST(id AS inte|) FROM users",
        "ALTER TABLE users ALTER COLUMN id TYPE inte|",
    ] {
        fixture
            .complete(marked)
            .assert_has("integer", CompletionKind::Type)
            .assert_lacks("order_status", CompletionKind::Type);
    }
}
