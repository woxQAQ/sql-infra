use pg_completion::{CatalogObjectKind, CompletionKind};

use super::support::Fixture;

#[test]
fn query_clause_expression_slots_have_visible_scope() {
    let fixture = Fixture::default();
    fixture
        .complete("SELECT | FROM users")
        .assert_visible_value_expression();

    for marked in [
        "SELECT id, | FROM users",
        "SELECT * FROM users WHERE |",
        "SELECT * FROM users GROUP BY |",
        "SELECT * FROM users HAVING |",
        "SELECT * FROM users ORDER BY |",
        "SELECT * FROM users LIMIT |",
        "SELECT * FROM users OFFSET |",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn advanced_query_expression_slots_have_visible_scope() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT DISTINCT ON (|) id FROM users",
        "SELECT * FROM users u JOIN orders o ON |",
        "SELECT * FROM users GROUP BY ROLLUP(|)",
        "SELECT count(id) FROM users WINDOW w AS (PARTITION BY |)",
        "SELECT * FROM users FETCH FIRST | ROWS ONLY",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn values_rows_complete_values_without_statement_starters() {
    let fixture = Fixture::default();
    for marked in ["VALUES (|)", "VALUES (1, |)", "VALUES (1), (|)"] {
        fixture
            .complete(marked)
            .assert_required_value_expression()
            .assert_lacks_kind(CompletionKind::Column)
            .assert_lacks("SELECT", CompletionKind::Keyword)
            .assert_lacks("CREATE", CompletionKind::Keyword);
    }
}

#[test]
fn dml_value_slots_include_target_relation_columns() {
    let fixture = Fixture::default();
    for marked in [
        "INSERT INTO users(name) VALUES ('x') RETURNING |",
        "INSERT INTO users(name) VALUES ('x') ON CONFLICT (id) DO UPDATE SET name = |",
        "UPDATE users SET name = |",
        "UPDATE users SET name = 'x' WHERE |",
        "UPDATE users SET name = 'x' RETURNING |",
        "DELETE FROM users WHERE |",
        "DELETE FROM users RETURNING |",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn default_is_only_offered_in_dml_assignment_value_slots() {
    let fixture = Fixture::default();
    for marked in [
        "INSERT INTO users(name) VALUES (|)",
        "UPDATE users SET name = |",
        "INSERT INTO users(name) VALUES ('x') ON CONFLICT (id) DO UPDATE SET name = |",
        "WITH recent AS (SELECT id FROM orders) UPDATE users SET name = |",
    ] {
        fixture
            .complete(marked)
            .assert_has("DEFAULT", CompletionKind::Keyword);
    }
    for marked in [
        "SELECT | FROM users",
        "UPDATE users SET name = 'x' WHERE |",
        "UPDATE users SET name = 'x' RETURNING |",
    ] {
        fixture
            .complete(marked)
            .assert_lacks("DEFAULT", CompletionKind::Keyword);
    }
}

#[test]
fn relation_owned_ddl_and_utility_expressions_include_relation_columns() {
    let fixture = Fixture::default();
    for marked in [
        "CREATE INDEX users_expr ON users ((|))",
        "CREATE INDEX users_partial ON users (id) WHERE |",
        "CREATE POLICY users_policy ON users USING (|)",
        "CREATE POLICY users_policy ON users WITH CHECK (|)",
        "CREATE PUBLICATION users_publication FOR TABLE users WHERE (|)",
        "COPY users FROM STDIN WHERE |",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn merge_conditions_assignments_and_returning_see_target_and_source_scope() {
    let fixture = Fixture::default();
    for marked in [
        "MERGE INTO users u USING orders o ON | WHEN MATCHED THEN DO NOTHING",
        "MERGE INTO users u USING orders o ON u.id = o.user_id WHEN MATCHED AND | THEN DO NOTHING",
        "MERGE INTO users u USING orders o ON u.id = o.user_id WHEN MATCHED THEN UPDATE SET name = |",
        "MERGE INTO users u USING orders o ON u.id = o.user_id WHEN MATCHED THEN DO NOTHING RETURNING |",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression()
            .assert_has("u", CompletionKind::Alias)
            .assert_has("o", CompletionKind::Alias);
    }
}

#[test]
fn update_from_and_delete_using_expose_all_relations() {
    let fixture = Fixture::default();
    fixture
        .complete("UPDATE users u SET name = | FROM orders o")
        .assert_required_visible_value_expression()
        .assert_has("amount", CompletionKind::Column)
        .assert_has("u", CompletionKind::Alias)
        .assert_has("o", CompletionKind::Alias);
    fixture
        .complete("DELETE FROM users u USING orders o WHERE |")
        .assert_required_visible_value_expression()
        .assert_has("amount", CompletionKind::Column)
        .assert_has("u", CompletionKind::Alias)
        .assert_has("o", CompletionKind::Alias);
}

#[test]
fn with_dml_statements_expose_cte_and_target_columns() {
    Fixture::default()
        .complete(
            "WITH recent(order_id, user_id, total) AS \
             (SELECT id, user_id, amount FROM orders) \
             UPDATE users u SET name = | FROM recent r",
        )
        .assert_required_visible_value_expression()
        .assert_has("total", CompletionKind::Column)
        .assert_has("u", CompletionKind::Alias)
        .assert_has("r", CompletionKind::Alias);
}

#[test]
fn insert_select_switches_from_source_scope_to_target_returning_scope() {
    let fixture = Fixture::default();
    fixture
        .complete("INSERT INTO users(name) SELECT | FROM orders o")
        .assert_value_expression()
        .assert_has("amount", CompletionKind::Column)
        .assert_has("o", CompletionKind::Alias)
        .assert_lacks("name", CompletionKind::Column)
        .assert_lacks("DEFAULT", CompletionKind::Keyword);

    fixture
        .complete("INSERT INTO users(name) SELECT amount::text FROM orders o RETURNING |")
        .assert_required_visible_value_expression()
        .assert_lacks("amount", CompletionKind::Column)
        .assert_lacks("o", CompletionKind::Alias);
}

#[test]
fn remaining_statement_expression_slots_reach_value_completion() {
    let mut fixture = Fixture::default();
    fixture.catalog.add_relation(
        "public",
        "events",
        CatalogObjectKind::Table,
        [("occurred_at".into(), "timestamp".into())],
    );

    for marked in [
        "CREATE TABLE t (value integer DEFAULT |)",
        "CREATE TABLE t (value integer CHECK (|))",
        "CREATE TABLE t (value integer GENERATED ALWAYS AS (|) STORED)",
        "CREATE DOMAIN positive_integer AS integer DEFAULT |",
        "CREATE DOMAIN positive_integer AS integer CHECK (|)",
        "ALTER DOMAIN positive_integer SET DEFAULT |",
        "ALTER DOMAIN positive_integer ADD CHECK (|)",
        "CREATE TRIGGER users_trigger BEFORE UPDATE ON users FOR EACH ROW WHEN (|) EXECUTE FUNCTION calculate_total()",
        "CREATE STATISTICS s ON | FROM users",
        "CALL calculate_total(|)",
        "EXECUTE prepared_statement(|)",
        "SELECT * FROM users TABLESAMPLE system(|)",
        "CREATE TABLE events_2026 PARTITION OF events FOR VALUES FROM (|) TO ('2027-01-01')",
    ] {
        fixture
            .complete(marked)
            .assert_has("count", CompletionKind::Function)
            .assert_has("NULL", CompletionKind::Keyword)
            .assert_lacks("SELECT", CompletionKind::Keyword)
            .assert_lacks("CREATE", CompletionKind::Keyword);
    }
}

#[test]
fn restricted_expression_slots_do_not_offer_disallowed_not_prefix() {
    let fixture = Fixture::default();
    for marked in [
        "CREATE TABLE t (value integer DEFAULT |)",
        "SELECT * FROM XMLTABLE('/x' PASSING |)",
    ] {
        fixture
            .complete(marked)
            .assert_has("count", CompletionKind::Function)
            .assert_has("NULL", CompletionKind::Keyword)
            .assert_lacks("NOT", CompletionKind::Keyword);
    }
}
