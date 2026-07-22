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
fn join_using_honors_alias_column_lists_on_both_sides() {
    Fixture::default()
        .complete(
            "SELECT * FROM users u(user_id, user_name) \
             JOIN orders o(order_id, user_id, total) USING (user_|",
        )
        .assert_kind_labels(CompletionKind::Column, &["user_id"]);
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
    fixture
        .complete("SELECT se| FROM users")
        .assert_insert_text("select", CompletionKind::Column, "\"select\"");
    fixture
        .complete("SELECT p.Dis| FROM \"UserProfile\" p")
        .assert_insert_text("DisplayName", CompletionKind::Column, "\"DisplayName\"");
}

#[test]
fn range_aliases_are_expression_candidates_and_hide_base_relation_names() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT u| FROM users u")
        .assert_has("u", CompletionKind::Alias)
        .assert_lacks("users", CompletionKind::Alias);
    fixture
        .complete("SELECT s| FROM (SELECT id FROM users) s")
        .assert_has("s", CompletionKind::Alias);
    fixture
        .complete("SELECT f| FROM calculate_total(1) f")
        .assert_has("f", CompletionKind::Alias);
    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT u|)")
        .assert_has("u", CompletionKind::Alias);
}

#[test]
fn quoted_alias_completion_is_case_sensitive_and_quotes_insert_text() {
    Fixture::default()
        .complete("SELECT \"U|\" FROM users \"UserAlias\"")
        .assert_insert_text("UserAlias", CompletionKind::Alias, "\"UserAlias\"")
        .assert_lacks("users", CompletionKind::Alias);
}

#[test]
fn ambiguous_unqualified_columns_remain_distinguishable_by_detail() {
    Fixture::default()
        .complete("SELECT i| FROM users u JOIN orders o ON u.id = o.id")
        .assert_count("id", CompletionKind::Column, 2)
        .assert_has_detail("id", CompletionKind::Column, "u.id integer")
        .assert_has_detail("id", CompletionKind::Column, "o.id integer");
}

#[test]
fn range_tail_keywords_are_not_misclassified_as_aliases() {
    Fixture::default()
        .complete("SELECT | FROM users TABLESAMPLE system(10) REPEATABLE (1)")
        .assert_lacks("tablesample", CompletionKind::Alias)
        .assert_lacks("repeatable", CompletionKind::Alias)
        .assert_has("name", CompletionKind::Column);
}

#[test]
fn inner_frames_really_shadow_outer_ranges_and_columns() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT u.| FROM orders u)")
        .assert_has("amount", CompletionKind::Column)
        .assert_lacks("name", CompletionKind::Column);

    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT i| FROM orders o)")
        .assert_count("id", CompletionKind::Column, 1)
        .assert_has_detail("id", CompletionKind::Column, "o.id integer");

    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT na| FROM orders o)")
        .assert_has("name", CompletionKind::Column);

    fixture
        .complete("SELECT * FROM users u WHERE EXISTS (SELECT na| FROM orders u)")
        .assert_has("name", CompletionKind::Column);
}

#[test]
fn lateral_subqueries_see_only_left_hand_siblings() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT * FROM users u, LATERAL (SELECT |) s, \"UserProfile\" profile")
        .assert_has("name", CompletionKind::Column)
        .assert_lacks("DisplayName", CompletionKind::Column);

    fixture
        .complete("SELECT * FROM users u, order_lines(|) lines, \"UserProfile\" profile")
        .assert_has("name", CompletionKind::Column)
        .assert_lacks("DisplayName", CompletionKind::Column)
        .assert_lacks("line_id", CompletionKind::Column);
}

#[test]
fn row_shapes_flow_through_ctes_derived_tables_and_wildcards() {
    let fixture = Fixture::default();
    fixture
        .complete(
            "WITH active AS (SELECT id, name AS display_name FROM users) \
             SELECT active.| FROM active",
        )
        .assert_has("id", CompletionKind::Column)
        .assert_has("display_name", CompletionKind::Column)
        .assert_lacks("name", CompletionKind::Column);

    fixture
        .complete("SELECT derived.| FROM (SELECT u.* FROM users u) derived")
        .assert_has("id", CompletionKind::Column)
        .assert_has("name", CompletionKind::Column);

    fixture
        .complete(
            "SELECT derived.| FROM \
             (SELECT id::text, name::text AS display_name FROM users) derived",
        )
        .assert_has("id", CompletionKind::Column)
        .assert_has("display_name", CompletionKind::Column)
        .assert_lacks("name", CompletionKind::Column);

    fixture
        .complete("SELECT values_row.| FROM (VALUES (1, 'one')) values_row")
        .assert_has("column1", CompletionKind::Column)
        .assert_has("column2", CompletionKind::Column);
}

#[test]
fn alias_lists_rename_positionally_without_dropping_remaining_columns() {
    Fixture::default()
        .complete("SELECT u.| FROM users u(user_id)")
        .assert_has("user_id", CompletionKind::Column)
        .assert_has("name", CompletionKind::Column)
        .assert_has("select", CompletionKind::Column)
        .assert_lacks("id", CompletionKind::Column);
}

#[test]
fn table_function_row_shapes_are_available_to_range_bindings() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT lines.| FROM order_lines(1) lines")
        .assert_has("line_id", CompletionKind::Column)
        .assert_has("total", CompletionKind::Column)
        .assert_has_detail("line_id", CompletionKind::Column, "lines.line_id integer");

    fixture
        .complete("SELECT lines.| FROM order_lines(1) AS lines(item_id integer, amount numeric)")
        .assert_has("item_id", CompletionKind::Column)
        .assert_has("amount", CompletionKind::Column)
        .assert_lacks("integer", CompletionKind::Column)
        .assert_lacks("numeric", CompletionKind::Column);
}

#[test]
fn recursive_cte_row_shapes_stop_at_cycles() {
    Fixture::default()
        .complete(
            "WITH RECURSIVE nums(n) AS (SELECT n FROM nums) \
             SELECT nums.| FROM nums",
        )
        .assert_has("n", CompletionKind::Column);
}

#[test]
fn nested_ctes_shadow_outer_cte_row_shapes() {
    Fixture::default()
        .complete(
            "WITH active(user_id) AS (SELECT id FROM users) \
             SELECT * FROM (WITH active(order_id) AS (SELECT id FROM orders) \
             SELECT active.| FROM active) nested",
        )
        .assert_has("order_id", CompletionKind::Column)
        .assert_lacks("user_id", CompletionKind::Column);
}
