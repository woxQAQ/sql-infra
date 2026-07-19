use pg_completion::CompletionKind;

use super::support::Fixture;

#[test]
fn visible_columns_come_from_relations_in_scope() {
    Fixture::default()
        .complete("SELECT na| FROM users u")
        .assert_has("name", CompletionKind::Column)
        .assert_lacks("amount", CompletionKind::Column);
}

#[test]
fn qualified_columns_use_aliases_and_fall_back_to_catalog_relations() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT u.na| FROM users u JOIN orders o ON true")
        .assert_has("name", CompletionKind::Column)
        .assert_lacks("amount", CompletionKind::Column);
    fixture
        .complete("SELECT users.na|")
        .assert_has("name", CompletionKind::Column);
}

#[test]
fn alias_column_lists_override_catalog_column_names() {
    Fixture::default()
        .complete("SELECT u.user_| FROM users u(user_id, user_name)")
        .assert_has("user_id", CompletionKind::Column)
        .assert_has("user_name", CompletionKind::Column)
        .assert_lacks("name", CompletionKind::Column);
}

#[test]
fn join_using_only_returns_columns_present_on_both_sides() {
    Fixture::default()
        .complete("SELECT * FROM users u JOIN orders o USING (|")
        .assert_has("id", CompletionKind::Column)
        .assert_lacks("name", CompletionKind::Column)
        .assert_lacks("amount", CompletionKind::Column)
        .assert_kind_labels(CompletionKind::Column, &["id"]);
}

#[test]
fn target_relation_columns_cover_dml_ddl_and_utility_slots() {
    let fixture = Fixture::default();
    for marked in [
        "INSERT INTO users (na|",
        "UPDATE users SET na|",
        "ALTER TABLE users RENAME COLUMN na|",
        "CREATE INDEX users_name ON users (na|",
        "COMMENT ON COLUMN users.na|",
    ] {
        fixture
            .complete(marked)
            .assert_has("name", CompletionKind::Column)
            .assert_lacks("amount", CompletionKind::Column);
    }
}

#[test]
fn correlated_and_lateral_subqueries_inherit_outer_columns() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT na| FROM orders o)")
        .assert_has("name", CompletionKind::Column);
    fixture
        .complete("SELECT * FROM users u, LATERAL (SELECT na|) s")
        .assert_has("name", CompletionKind::Column);
    fixture
        .complete("SELECT * FROM users u, (SELECT na|) s")
        .assert_lacks("name", CompletionKind::Column);
}

#[test]
fn cte_and_subquery_alias_columns_are_visible() {
    let fixture = Fixture::default();
    fixture
        .complete("WITH active(user_id) AS (SELECT id FROM users) SELECT active.user_| FROM active")
        .assert_has("user_id", CompletionKind::Column);
    fixture
        .complete("SELECT s.fo| FROM (SELECT id FROM users) s(foo)")
        .assert_has("foo", CompletionKind::Column);
}

#[test]
fn reserved_and_mixed_case_columns_are_quoted_on_insert() {
    let fixture = Fixture::default();
    assert_eq!(
        fixture
            .complete("SELECT se| FROM users")
            .item("select", CompletionKind::Column)
            .insert_text,
        "\"select\""
    );
    assert_eq!(
        fixture
            .complete("SELECT p.Dis| FROM \"UserProfile\" p")
            .item("DisplayName", CompletionKind::Column)
            .insert_text,
        "\"DisplayName\""
    );
}
