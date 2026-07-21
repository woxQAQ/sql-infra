use pg_completion::{CatalogItem, CatalogObjectIdentity, CatalogObjectKind, CompletionKind};

use super::support::Fixture;

#[test]
fn exact_prefix_matches_rank_before_longer_matches() {
    let mut fixture = Fixture::default();
    fixture.catalog.add_relation(
        "public",
        "user",
        CatalogObjectKind::Table,
        [("id".into(), "integer".into())],
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
        .add_relation("public", "users", CatalogObjectKind::Table, []);
    fixture
        .complete("SELECT * FROM users|")
        .assert_count("users", CompletionKind::Table, 1);
}

#[test]
fn equal_scores_have_deterministic_case_insensitive_label_order() {
    let mut fixture = Fixture::default();
    fixture.catalog.add_relation(
        "public",
        "beta",
        CatalogObjectKind::Table,
        [("id".into(), "integer".into())],
    );
    fixture.catalog.add_relation(
        "public",
        "alpha",
        CatalogObjectKind::Table,
        [("id".into(), "integer".into())],
    );
    fixture
        .complete("SELECT * FROM |")
        .assert_labels_in_order(CompletionKind::Table, &["alpha", "beta"]);
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
        .add_function("pg_catalog", "count", "count(any) -> bigint");
    fixture.catalog.add_type("pg_catalog", "integer");

    fixture
        .complete("SELECT cou|")
        .assert_count("count", CompletionKind::Function, 1);
    fixture
        .complete("SELECT 1::inte|")
        .assert_count("integer", CompletionKind::Type, 1);
}

#[test]
fn overloaded_catalog_objects_remain_distinct_by_structured_identity() {
    let mut fixture = Fixture::default();
    fixture.catalog.add(
        CatalogItem::new(
            CatalogObjectIdentity::in_schema(
                CatalogObjectKind::Function,
                "public",
                "calculate_total",
            )
            .with_signature(["integer"]),
        )
        .with_definition("calculate_total(integer) -> numeric"),
    );

    fixture
        .complete("SELECT calculate_| ")
        .assert_count("calculate_total", CompletionKind::Function, 2)
        .assert_has_matching("calculate_total", CompletionKind::Function, |item| {
            item.catalog_identity
                .as_ref()
                .is_some_and(|identity| identity.signature.is_empty())
        })
        .assert_has_matching("calculate_total", CompletionKind::Function, |item| {
            item.catalog_identity
                .as_ref()
                .is_some_and(|identity| identity.signature == ["integer"])
        });
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
    fixture.catalog.add_relation(
        "audit",
        "users",
        CatalogObjectKind::View,
        [("id".into(), "integer".into())],
    );
    fixture
        .complete("SELECT * FROM users|")
        .assert_has_detail("users", CompletionKind::Table, "public.users")
        .assert_has_detail("users", CompletionKind::View, "audit.users");
}

#[test]
fn search_path_ranks_same_named_functions_and_types() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .add_function("audit", "calculate_total", "audit calculation");
    fixture.catalog.add_type("audit", "order_status");

    fixture
        .complete("SELECT calculate_|")
        .assert_details_in_order(
            CompletionKind::Function,
            &[
                "public.calculate_total calculate_total(numeric) -> numeric",
                "audit.calculate_total audit calculation",
            ],
        );

    fixture
        .complete("SELECT 1::order_|")
        .assert_details_in_order(
            CompletionKind::Type,
            &["public.order_status", "audit.order_status"],
        );
}
