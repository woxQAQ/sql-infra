use pg_completion::CompletionKind;

use super::support::Fixture;

#[test]
fn functions_complete_in_expression_slots_with_metadata() {
    Fixture::default()
        .complete("SELECT cou| FROM users")
        .assert_has_detail(
            "count",
            CompletionKind::Function,
            "pg_catalog.count count(any) -> bigint",
        )
        .assert_documentation("count", CompletionKind::Function, "number of input rows");
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
        .add_function("public", "calculate total", "calculate total()");
    fixture.catalog.add_type("public", "order state");

    fixture.complete("SELECT calculate| ").assert_insert_text(
        "calculate total",
        CompletionKind::Function,
        "\"calculate total\"",
    );
    fixture.complete("SELECT 1::order|").assert_insert_text(
        "order state",
        CompletionKind::Type,
        "\"order state\"",
    );
}

#[test]
fn quoted_routine_and_type_prefixes_are_case_sensitive() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .add_function("public", "CalculateTotal", "CalculateTotal()");
    fixture.catalog.add_type("public", "OrderState");

    fixture.complete("SELECT \"Calc|\"").assert_insert_text(
        "CalculateTotal",
        CompletionKind::Function,
        "\"CalculateTotal\"",
    );
    fixture
        .complete("SELECT \"calc|\"")
        .assert_lacks("CalculateTotal", CompletionKind::Function);

    fixture.complete("SELECT 1::\"Order|\"").assert_insert_text(
        "OrderState",
        CompletionKind::Type,
        "\"OrderState\"",
    );
    fixture
        .complete("SELECT 1::\"order|\"")
        .assert_lacks("OrderState", CompletionKind::Type);
}
