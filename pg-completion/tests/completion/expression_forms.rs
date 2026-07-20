use pg_completion::CompletionKind;

use super::support::Fixture;

#[test]
fn expression_completion_offers_representative_starters_and_excludes_contextual_keywords() {
    let result = Fixture::default().complete("SELECT | FROM users");

    // Exercise the public completion contract across distinct expression
    // families. The form-specific tests below cover the detailed grammar;
    // this test deliberately does not mirror the implementation's token list.
    for keyword in [
        "NULL",
        "TRUE",
        "NOT",
        "EXISTS",
        "ARRAY",
        "CASE",
        "CAST",
        "EXTRACT",
        "CURRENT_DATE",
        "XMLSERIALIZE",
        "JSON_VALUE",
        "ROW",
        "COALESCE",
    ] {
        result.assert_has(keyword, CompletionKind::Keyword);
    }
    result
        .assert_lacks("SELECT", CompletionKind::Keyword)
        .assert_lacks("DEFAULT", CompletionKind::Keyword)
        .assert_lacks("MERGE_ACTION", CompletionKind::Keyword);
}

#[test]
fn operators_and_predicates_complete_their_required_operands() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT NOT | FROM users",
        "SELECT +| FROM users",
        "SELECT -| FROM users",
        "SELECT id + | FROM users",
        "SELECT id * | FROM users",
        "SELECT id = | FROM users",
        "SELECT id AND | FROM users",
        "SELECT id OR | FROM users",
        "SELECT id BETWEEN | AND 10 FROM users",
        "SELECT id BETWEEN 1 AND | FROM users",
        "SELECT id NOT BETWEEN | AND 10 FROM users",
        "SELECT id NOT BETWEEN 1 AND | FROM users",
        "SELECT id IN (|) FROM users",
        "SELECT id IN (1, |) FROM users",
        "SELECT id NOT IN (|) FROM users",
        "SELECT name LIKE | FROM users",
        "SELECT name LIKE 'n%' ESCAPE | FROM users",
        "SELECT name SIMILAR TO | FROM users",
        "SELECT id IS DISTINCT FROM | FROM users",
        "SELECT (id, id) OVERLAPS (|, id) FROM users",
        "SELECT id = ANY (ARRAY[|]) FROM users",
        "SELECT id = ANY (|) FROM users",
        "SELECT id # | FROM users",
        "SELECT id OPERATOR(pg_catalog.+) | FROM users",
        "SELECT id AT TIME ZONE | FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn case_row_array_parentheses_and_subscripts_complete_values() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT (|) FROM users",
        "SELECT ROW(|) FROM users",
        "SELECT ROW(id, |) FROM users",
        "SELECT ARRAY[|] FROM users",
        "SELECT ARRAY[id, |] FROM users",
        "SELECT ARRAY[[id], [|]] FROM users",
        "SELECT CASE | WHEN 1 THEN 2 END FROM users",
        "SELECT CASE WHEN | THEN 1 END FROM users",
        "SELECT CASE WHEN true THEN | END FROM users",
        "SELECT CASE WHEN true THEN 1 ELSE | END FROM users",
        "SELECT name[|] FROM users",
        "SELECT name[1:|] FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn function_arguments_and_decorations_complete_values() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT count(|) FROM users",
        "SELECT GROUPING(|) FROM users",
        "SELECT coalesce(id, |) FROM users",
        "SELECT greatest(id, |) FROM users",
        "SELECT least(id, |) FROM users",
        "SELECT nullif(id, |) FROM users",
        "SELECT calculate_total(value => |) FROM users",
        "SELECT calculate_total(VARIADIC |) FROM users",
        "SELECT count(DISTINCT |) FROM users",
        "SELECT count(id ORDER BY |) FROM users",
        "SELECT count(id) FILTER (WHERE |) FROM users",
        "SELECT count(id) OVER (PARTITION BY |) FROM users",
        "SELECT count(id) OVER (ORDER BY |) FROM users",
        "SELECT count(id) OVER (ROWS BETWEEN | PRECEDING AND CURRENT ROW) FROM users",
        "SELECT count(id) OVER (ROWS BETWEEN 1 PRECEDING AND | FOLLOWING) FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn sql_syntax_functions_complete_every_expression_argument() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT CAST(| AS integer) FROM users",
        "SELECT TREAT(| AS integer) FROM users",
        "SELECT COLLATION FOR (|) FROM users",
        "SELECT EXTRACT(day FROM |) FROM users",
        "SELECT NORMALIZE(|) FROM users",
        "SELECT POSITION(| IN name) FROM users",
        "SELECT POSITION('x' IN |) FROM users",
        "SELECT OVERLAY(name PLACING | FROM 1) FROM users",
        "SELECT OVERLAY(name PLACING 'x' FROM |) FROM users",
        "SELECT OVERLAY(name PLACING 'x' FROM 1 FOR |) FROM users",
        "SELECT SUBSTRING(name FROM |) FROM users",
        "SELECT SUBSTRING(name FROM 1 FOR |) FROM users",
        "SELECT XMLSERIALIZE(DOCUMENT | AS text) FROM users",
        "SELECT XMLEXISTS('/x' PASSING |) FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }

    // At this position FROM is itself a valid TRIM grammar alternative.
    fixture
        .complete("SELECT TRIM(| FROM name) FROM users")
        .assert_has("name", CompletionKind::Column)
        .assert_has("count", CompletionKind::Function)
        .assert_has("NULL", CompletionKind::Keyword)
        .assert_lacks_kind(CompletionKind::Table)
        .assert_lacks_kind(CompletionKind::Schema);
}

#[test]
fn xml_expression_arguments_complete_values() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT XMLCONCAT(|) FROM users",
        "SELECT XMLCONCAT(id, |) FROM users",
        "SELECT XMLELEMENT(NAME x, |) FROM users",
        "SELECT XMLELEMENT(NAME x, XMLATTRIBUTES(|)) FROM users",
        "SELECT XMLFOREST(|) FROM users",
        "SELECT XMLPARSE(DOCUMENT |) FROM users",
        "SELECT XMLPI(NAME x, |) FROM users",
        "SELECT XMLROOT(|, VERSION '1.0') FROM users",
        "SELECT XMLROOT(id, VERSION |) FROM users",
    ] {
        fixture
            .complete(marked)
            .assert_required_visible_value_expression();
    }
}

#[test]
fn json_expression_arguments_complete_values() {
    let fixture = Fixture::default();
    for marked in [
        "SELECT JSON_OBJECT(| VALUE 1) FROM users",
        "SELECT JSON_OBJECT('key' VALUE |) FROM users",
        "SELECT JSON_ARRAY(|) FROM users",
        "SELECT JSON_ARRAY(id, |) FROM users",
        "SELECT JSON(|) FROM users",
        "SELECT JSON_SCALAR(|) FROM users",
        "SELECT JSON_SERIALIZE(|) FROM users",
        "SELECT JSON_VALUE(|, '$') FROM users",
        "SELECT JSON_VALUE(name, |) FROM users",
        "SELECT JSON_VALUE(name, '$' PASSING | AS value) FROM users",
        "SELECT JSON_VALUE(name, '$' DEFAULT | ON EMPTY) FROM users",
        "SELECT JSON_OBJECTAGG(| VALUE name) FROM users",
        "SELECT JSON_OBJECTAGG(id VALUE |) FROM users",
        "SELECT JSON_ARRAYAGG(|) FROM users",
        "SELECT JSON_ARRAYAGG(id ORDER BY |) FROM users",
        "SELECT JSON_ARRAYAGG(id) FILTER (WHERE |) FROM users",
    ] {
        fixture.complete(marked).assert_visible_value_expression();
    }
}
