use pg_completion::{CatalogItemKind, CompletionKind};

use super::support::Fixture;

#[test]
fn exact_prefix_matches_rank_before_longer_matches() {
    let mut fixture = Fixture::default();
    fixture.catalog.relation(
        "public",
        "user",
        CatalogItemKind::Table,
        &[("id", "integer")],
    );
    fixture
        .complete("SELECT * FROM user|")
        .assert_first("user", CompletionKind::Table)
        .assert_has("users", CompletionKind::Table);
}

#[test]
fn duplicate_catalog_rows_are_deduplicated_by_visible_identity() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .duplicate_relation("public", "users", CatalogItemKind::Table);
    let result = fixture.complete("SELECT * FROM users|");
    assert_eq!(result.count("users", CompletionKind::Table), 1);
}

#[test]
fn equal_scores_have_deterministic_case_insensitive_label_order() {
    let mut fixture = Fixture::default();
    fixture.catalog.relation(
        "public",
        "beta",
        CatalogItemKind::Table,
        &[("id", "integer")],
    );
    fixture.catalog.relation(
        "public",
        "alpha",
        CatalogItemKind::Table,
        &[("id", "integer")],
    );
    let result = fixture.complete("SELECT * FROM |");
    let labels = result
        .result
        .items
        .iter()
        .filter(|item| matches!(item.label.as_str(), "alpha" | "beta"))
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["alpha", "beta"]);
}

#[test]
fn expression_candidates_rank_aliases_columns_functions_and_keywords_by_relevance() {
    Fixture::default()
        .complete("SELECT | FROM users")
        .assert_first("id", CompletionKind::Column);
    Fixture::default()
        .complete("SELECT u| FROM users u")
        .assert_first("u", CompletionKind::Alias);
    Fixture::default()
        .complete("SELECT cou| FROM users")
        .assert_first("count", CompletionKind::Function);
    Fixture::default()
        .complete_without_catalog("SELECT NU|")
        .assert_first("NULL", CompletionKind::Keyword);
}

#[test]
fn duplicate_functions_and_types_are_deduplicated_by_visible_identity() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .duplicate_function("pg_catalog", "count", "count(any) -> bigint");
    fixture.catalog.duplicate_type("pg_catalog", "integer");

    assert_eq!(
        fixture
            .complete("SELECT cou|")
            .count("count", CompletionKind::Function),
        1
    );
    assert_eq!(
        fixture
            .complete("SELECT 1::inte|")
            .count("integer", CompletionKind::Type),
        1
    );
}

#[test]
fn every_returned_item_obeys_prefix_filtering() {
    let fixture = Fixture::default();
    for (marked, prefix) in [
        ("SEL|", "SEL"),
        ("SELECT na| FROM users", "na"),
        ("SELECT cou| FROM users", "cou"),
        ("SELECT * FROM act|", "act"),
        ("SELECT 1::inte|", "inte"),
    ] {
        fixture
            .complete(marked)
            .assert_prefix_filtered(prefix, false);
    }
    fixture
        .complete("SELECT * FROM \"User|")
        .assert_prefix_filtered("User", true);
}

#[test]
fn relation_details_distinguish_same_labels_from_different_schemas() {
    let mut fixture = Fixture::default();
    fixture.catalog.relation(
        "audit",
        "users",
        CatalogItemKind::View,
        &[("id", "integer")],
    );
    let result = fixture.complete("SELECT * FROM users|");
    assert!(result.result.items.iter().any(|item| {
        item.kind == CompletionKind::Table && item.detail.as_deref() == Some("public.users")
    }));
    assert!(result.result.items.iter().any(|item| {
        item.kind == CompletionKind::View && item.detail.as_deref() == Some("audit.users")
    }));
}

#[test]
fn search_path_ranks_same_named_functions_and_types() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .function("audit", "calculate_total", "audit calculation", None);
    fixture.catalog.ty("audit", "order_status");

    let functions = fixture.complete("SELECT calculate_|");
    let public_function = functions
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::Function
                && item.detail.as_deref()
                    == Some("public.calculate_total calculate_total(numeric) -> numeric")
        })
        .unwrap();
    let audit_function = functions
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::Function
                && item.detail.as_deref() == Some("audit.calculate_total audit calculation")
        })
        .unwrap();
    assert!(public_function < audit_function);

    let types = fixture.complete("SELECT 1::order_|");
    let public_type = types
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::Type
                && item.detail.as_deref() == Some("public.order_status")
        })
        .unwrap();
    let audit_type = types
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::Type
                && item.detail.as_deref() == Some("audit.order_status")
        })
        .unwrap();
    assert!(public_type < audit_type);
}
