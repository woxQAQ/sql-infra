use pg_completion::{CatalogObjectKind, CompletionKind};

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
    fixture.catalog.add_relation(
        "public",
        "active",
        CatalogObjectKind::Table,
        [("id".into(), "integer".into())],
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
    fixture.catalog.add_relation(
        "audit",
        "users",
        CatalogObjectKind::View,
        [("id".into(), "integer".into())],
    );
    fixture
        .complete("SELECT * FROM user|")
        .assert_has("users", CompletionKind::Table)
        .assert_has("users", CompletionKind::View)
        .assert_candidates_in_order(&[
            (CompletionKind::Table, "public.users"),
            (CompletionKind::View, "audit.users"),
        ]);

    Fixture::default()
        .with_search_path(&["audit", "public", "pg_catalog"])
        .complete("SELECT * FROM active_|")
        .assert_first("active_users", CompletionKind::View);
}

#[test]
fn quoted_relation_completion_is_case_sensitive_and_quotes_insert_text() {
    Fixture::default()
        .complete("SELECT * FROM \"User|")
        .assert_insert_text("UserProfile", CompletionKind::Table, "\"UserProfile\"")
        .assert_lacks("users", CompletionKind::Table)
        .assert_replaces("\"User");
}

#[test]
fn relation_insert_text_quotes_unsafe_catalog_identifiers() {
    let mut fixture = Fixture::default();
    for name in ["", "123users", "user profiles", "user\"profiles"] {
        fixture
            .catalog
            .add_relation("public", name, CatalogObjectKind::Table, []);
    }

    fixture
        .complete("SELECT * FROM |")
        .assert_insert_text("", CompletionKind::Table, "\"\"")
        .assert_insert_text("123users", CompletionKind::Table, "\"123users\"")
        .assert_insert_text("user profiles", CompletionKind::Table, "\"user profiles\"")
        .assert_insert_text(
            "user\"profiles",
            CompletionKind::Table,
            "\"user\"\"profiles\"",
        );
}

#[test]
fn quoted_relation_prefixes_decode_escaped_quotes_and_unicode() {
    let mut fixture = Fixture::default();
    fixture
        .catalog
        .add_relation("public", "user\"profiles", CatalogObjectKind::Table, []);
    fixture
        .catalog
        .add_relation("public", "用户资料", CatalogObjectKind::Table, []);

    fixture
        .complete("SELECT * FROM \"user\"\"p|\"")
        .assert_insert_text(
            "user\"profiles",
            CompletionKind::Table,
            "\"user\"\"profiles\"",
        )
        .assert_replaces("\"user\"\"p\"");

    fixture
        .complete("SELECT * FROM \"用|\"")
        .assert_has("用户资料", CompletionKind::Table)
        .assert_replaces("\"用\"");
}
