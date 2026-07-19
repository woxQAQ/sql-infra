use pg_completion::{CatalogItemKind, CompletionKind};

use super::support::Fixture;

#[test]
fn relation_candidates_preserve_catalog_kinds() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT * FROM us|")
        .assert_has("users", CompletionKind::Table)
        .assert_lacks("orders", CompletionKind::Table);
    fixture
        .complete("SELECT * FROM |")
        .assert_lacks("name", CompletionKind::Column)
        .assert_lacks("integer", CompletionKind::Type);
    fixture
        .complete("SELECT * FROM active_|")
        .assert_has("active_users", CompletionKind::View);
    fixture
        .complete("SELECT * FROM recent_|")
        .assert_has("recent_orders", CompletionKind::MaterializedView);
}

#[test]
fn schemas_and_schema_qualified_relations_are_resolved_separately() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT * FROM pub|")
        .assert_has("public", CompletionKind::Schema);
    fixture
        .complete("SELECT * FROM audit.active_|")
        .assert_has("active_users", CompletionKind::View)
        .assert_lacks("users", CompletionKind::Table)
        .assert_replaces("active_");
    fixture
        .complete("SELECT * FROM audit.|")
        .assert_has("active_users", CompletionKind::View)
        .assert_has("recent_orders", CompletionKind::MaterializedView)
        .assert_replaces("");
}

#[test]
fn ctes_are_relation_candidates_and_outrank_catalog_relations() {
    let mut fixture = Fixture::default();
    fixture.catalog.relation(
        "public",
        "active",
        CatalogItemKind::Table,
        &[("id", "integer")],
    );
    fixture
        .complete("WITH active(id) AS (SELECT id FROM users) SELECT * FROM ac|")
        .assert_has("active", CompletionKind::Cte)
        .assert_has("active", CompletionKind::Table)
        .assert_first("active", CompletionKind::Cte);
}

#[test]
fn search_path_relations_rank_before_other_schemas() {
    let mut fixture = Fixture::default();
    fixture.catalog.relation(
        "audit",
        "users",
        CatalogItemKind::View,
        &[("id", "integer")],
    );
    let result = fixture.complete("SELECT * FROM user|");
    result
        .assert_has("users", CompletionKind::Table)
        .assert_has("users", CompletionKind::View);
    let public = result
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::Table && item.detail.as_deref() == Some("public.users")
        })
        .unwrap();
    let audit = result
        .result
        .items
        .iter()
        .position(|item| {
            item.kind == CompletionKind::View && item.detail.as_deref() == Some("audit.users")
        })
        .unwrap();
    assert!(public < audit);

    Fixture::default()
        .with_search_path(&["audit", "public", "pg_catalog"])
        .complete("SELECT * FROM active_|")
        .assert_first("active_users", CompletionKind::View);
}

#[test]
fn quoted_relation_completion_is_case_sensitive_and_quotes_insert_text() {
    let result = Fixture::default().complete("SELECT * FROM \"User|");
    let item = result.item("UserProfile", CompletionKind::Table);
    assert_eq!(item.insert_text, "\"UserProfile\"");
    result
        .assert_lacks("users", CompletionKind::Table)
        .assert_replaces("\"User");
}

#[test]
fn relation_insert_text_quotes_unsafe_catalog_identifiers() {
    let mut fixture = Fixture::default();
    for name in ["123users", "user profiles", "user\"profiles"] {
        fixture
            .catalog
            .relation("public", name, CatalogItemKind::Table, &[]);
    }

    let result = fixture.complete("SELECT * FROM |");
    assert_eq!(
        result.item("123users", CompletionKind::Table).insert_text,
        "\"123users\""
    );
    assert_eq!(
        result
            .item("user profiles", CompletionKind::Table)
            .insert_text,
        "\"user profiles\""
    );
    assert_eq!(
        result
            .item("user\"profiles", CompletionKind::Table)
            .insert_text,
        "\"user\"\"profiles\""
    );
}
