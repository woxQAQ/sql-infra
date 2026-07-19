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
        "alpha",
        CatalogItemKind::Table,
        &[("id", "integer")],
    );
    fixture.catalog.relation(
        "public",
        "beta",
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
