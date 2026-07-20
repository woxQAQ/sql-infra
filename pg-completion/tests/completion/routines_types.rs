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
            .assert_lacks("order_status", CompletionKind::Type)
            .assert_kind_labels(CompletionKind::Type, &["integer"]);
    }
}

#[test]
fn type_slots_exclude_other_catalog_families_and_honor_schema_qualification() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT 1::|")
        .assert_has("integer", CompletionKind::Type)
        .assert_has("order_status", CompletionKind::Type)
        .assert_lacks("count", CompletionKind::Function)
        .assert_lacks("users", CompletionKind::Table)
        .assert_lacks("id", CompletionKind::Column);
    fixture
        .complete("SELECT 1::pg_catalog.inte|")
        .assert_has("integer", CompletionKind::Type)
        .assert_lacks("order_status", CompletionKind::Type);
}

#[test]
fn routine_and_type_insert_text_quotes_unsafe_identifiers() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .function("public", "calculate total", "calculate total()", None);
    fixture.catalog.ty("public", "order state");

    assert_eq!(
        fixture
            .complete("SELECT calculate| ")
            .item("calculate total", CompletionKind::Function)
            .insert_text,
        "\"calculate total\""
    );
    assert_eq!(
        fixture
            .complete("SELECT 1::order|")
            .item("order state", CompletionKind::Type)
            .insert_text,
        "\"order state\""
    );
}

#[test]
fn quoted_routine_and_type_prefixes_are_case_sensitive() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .function("public", "CalculateTotal", "CalculateTotal()", None);
    fixture.catalog.ty("public", "OrderState");

    let function = fixture.complete("SELECT \"Calc|\"");
    assert_eq!(
        function
            .item("CalculateTotal", CompletionKind::Function)
            .insert_text,
        "\"CalculateTotal\""
    );
    fixture
        .complete("SELECT \"calc|\"")
        .assert_lacks("CalculateTotal", CompletionKind::Function);

    let ty = fixture.complete("SELECT 1::\"Order|\"");
    assert_eq!(
        ty.item("OrderState", CompletionKind::Type).insert_text,
        "\"OrderState\""
    );
    fixture
        .complete("SELECT 1::\"order|\"")
        .assert_lacks("OrderState", CompletionKind::Type);
}
