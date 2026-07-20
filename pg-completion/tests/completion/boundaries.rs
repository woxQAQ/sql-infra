use pg_completion::CompletionKind;

use super::support::Fixture;

#[test]
fn expression_completion_handles_empty_and_trailing_list_positions() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT (|) FROM users",
        "SELECT ARRAY[|] FROM users",
        "SELECT ARRAY[id, |] FROM users",
        "SELECT ROW(|) FROM users",
        "SELECT ROW(id, |) FROM users",
        "SELECT count(id, |) FROM users",
        "SELECT XMLCONCAT(|) FROM users",
        "SELECT XMLFOREST(|) FROM users",
        "SELECT JSON_ARRAY(|) FROM users",
    ] {
        fixture.complete(marked).assert_visible_value_expression();
    }
}

#[test]
fn replacement_ranges_cover_identifier_and_quoted_identifier_boundaries() {
    let fixture = Fixture::default();
    for (marked, replaced) in [
        ("SELECT * FROM |users", "users"),
        ("SELECT * FROM us|ers", "users"),
        ("SELECT * FROM users|", "users"),
        ("SELECT * FROM \"|UserProfile\"", "\"UserProfile\""),
        ("SELECT * FROM \"User|Profile\"", "\"UserProfile\""),
        (
            "SELECT p.\"Display|Name\" FROM \"UserProfile\" p",
            "\"DisplayName\"",
        ),
    ] {
        fixture.complete(marked).assert_replaces(replaced);
    }

    fixture
        .complete("SELECT * FROM audit.|active_users")
        .assert_replaces("active_users")
        .assert_has("active_users", CompletionKind::View);
}

#[test]
fn lexical_regions_and_their_boundaries_suppress_only_inside_content() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT E'na|' FROM users",
        "SELECT U&'na|' FROM users",
        "SELECT B'10|1' FROM users",
        "SELECT X'F|F' FROM users",
        "SELECT $tag$na|$tag$ FROM users",
        "SELECT /* outer /* inner | */ outer */ 1",
        "SELECT 'unterminated |",
        "SELECT /* unterminated |",
    ] {
        assert!(
            fixture.complete(marked).result.items.is_empty(),
            "{marked:?}"
        );
    }

    fixture
        .complete("SELECT 'name' | FROM users")
        .assert_has("FROM", CompletionKind::Keyword)
        .assert_lacks("count", CompletionKind::Function);
    fixture
        .complete("SELECT 1 /* comment */ + | FROM users")
        .assert_visible_value_expression();
}

#[test]
fn completion_result_contract_holds_across_representative_boundaries() {
    let fixture = Fixture::default();
    for marked in [
        "|",
        "SEL|",
        "SELECT | FROM users",
        "SELECT na| FROM users",
        "SELECT u.na| FROM users u",
        "SELECT * FROM audit.active_|",
        "SELECT 1::inte|",
        "SELECT count(|) FROM users",
        "SELECT 1; SELECT | FROM users",
        "SELECT '中', | FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_no_duplicate_items()
            .assert_replacement_contract();
    }
}
