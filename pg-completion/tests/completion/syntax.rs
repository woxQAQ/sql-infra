use pg_completion::{CompletionError, CompletionKind};

use super::support::Fixture;

#[test]
fn statement_keywords_are_prefix_filtered() {
    Fixture::default()
        .complete_without_catalog("SEL|")
        .assert_has("SELECT", CompletionKind::Keyword)
        .assert_lacks("INSERT", CompletionKind::Keyword)
        .assert_replaces("SEL")
        .assert_incomplete(false);
}

#[test]
fn syntax_only_results_are_marked_incomplete_when_names_need_a_catalog() {
    Fixture::default()
        .complete_without_catalog("SELECT | FROM users")
        .assert_has("NULL", CompletionKind::Keyword)
        .assert_lacks("name", CompletionKind::Column)
        .assert_incomplete(true);

    Fixture::default()
        .complete("SELECT | FROM users")
        .assert_incomplete(false);
}

#[test]
fn completion_works_at_document_boundaries_and_after_statements() {
    let fixture = Fixture::default();
    fixture
        .complete_without_catalog("|")
        .assert_has("SELECT", CompletionKind::Keyword)
        .assert_replaces("");
    fixture
        .complete_without_catalog("SELECT 1; SEL|")
        .assert_has("SELECT", CompletionKind::Keyword)
        .assert_replaces("SEL");
    fixture
        .complete("SELECT '中', |")
        .assert_has("count", CompletionKind::Function)
        .assert_replaces("");
}

#[test]
fn expression_slots_reach_the_completion_resolver_through_nested_parsers() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT | FROM users",
        "SELECT value + | FROM users",
        "SELECT count(|) FROM users",
        "WITH x AS (SELECT |",
        "SELECT (SELECT |",
        "SELECT 1 IN (SELECT |",
        "SELECT 1 = ANY (SELECT |",
        "SELECT JSON_ARRAY(SELECT |",
        "CREATE RULE r AS ON UPDATE TO users DO (SELECT |",
        "SELECT sum(id) OVER (PARTITION BY | FROM users",
        "SELECT json_arrayagg(id ORDER BY | FROM users",
        "SELECT * FROM JSON_TABLE(|",
        "SELECT * FROM ROWS FROM (|",
        "SELECT * FROM XMLTABLE(|",
        "CREATE STATISTICS s ON |",
        "CALL |",
        "UPDATE users SET name[|",
    ] {
        fixture
            .complete(marked)
            .assert_has("count", CompletionKind::Function);
    }
}

#[test]
fn replacement_covers_the_whole_identifier_when_cursor_is_in_the_middle() {
    Fixture::default()
        .complete("SELECT * FROM us|ers")
        .assert_has("users", CompletionKind::Table)
        .assert_replaces("users");
}

#[test]
fn strings_and_comments_suppress_completion() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT 'na|' FROM users",
        "SELECT 1 -- na|\nFROM users",
        "SELECT /* na| */ 1",
        "SELECT $$na|$$",
    ] {
        fixture.complete(marked).assert_empty();
    }
}

#[test]
fn cursor_validation_errors_cross_the_public_interface() {
    let fixture = Fixture::default();
    let sql = "SELECT 中";
    assert!(matches!(
        fixture.error_without_catalog(sql, "SELECT ".len() + 1),
        CompletionError::Syntax(pg_parser::CompletionError::CursorNotCharBoundary { .. })
    ));
    assert!(matches!(
        fixture.error_without_catalog(sql, sql.len() + 1),
        CompletionError::Syntax(pg_parser::CompletionError::CursorOutOfBounds { .. })
    ));
}
