use pg_parser::{
    Node, NodeTag, parse_plpgsql_assignment, parse_plpgsql_expression, parse_type_name,
};

use super::common::expect_node;

#[test]
fn plpgsql_assignment_mode_builds_complete_raw_statement() {
    let raw = parse_plpgsql_assignment(
        "record.items[1:upper].name := distinct value from app.items where active order by value limit 1",
        2,
    )
    .expect("parse PL/pgSQL assignment");

    assert_eq!(raw.node_tag, NodeTag::RawStmt);
    assert_eq!(raw.stmt_location, 0);
    assert_eq!(raw.stmt_len, 0);
    let stmt = expect_node!(raw.stmt.as_deref(), Some(PlAssignStmt));
    assert_eq!(stmt.node_tag, NodeTag::PlAssignStmt);
    assert_eq!(stmt.name.as_deref(), Some("record"));
    assert_eq!(stmt.nnames, 2);
    assert_eq!(stmt.location, 0);
    assert_eq!(stmt.indirection.len(), 3);
    let select = stmt.val.as_deref().expect("assignment value SelectStmt");
    assert_eq!(select.node_tag, NodeTag::SelectStmt);
    assert_eq!(select.distinct_clause.len(), 1);
    assert_eq!(select.target_list.len(), 1);
    assert_eq!(select.from_clause.len(), 1);
    assert!(select.where_clause.is_some());
    assert_eq!(select.sort_clause.len(), 1);
    assert!(select.limit_count.is_some());
}

#[test]
fn plpgsql_assignment_mode_accepts_each_raw_mode_and_equals_form() {
    for (sql, nnames, expected_name) in [
        ("item = 1", 1, "item"),
        ("record.item := 2", 2, "record"),
        ("outer_record.inner.item = 3", 3, "outer_record"),
        ("$1 := 4", 1, "$1"),
    ] {
        let raw = parse_plpgsql_assignment(sql, nnames).expect(sql);
        let stmt = expect_node!(raw.stmt.as_deref(), Some(PlAssignStmt));
        assert_eq!(stmt.name.as_deref(), Some(expected_name), "{sql}");
        assert_eq!(stmt.nnames, nnames, "{sql}");
        assert!(stmt.val.is_some(), "{sql}");
    }
}

#[test]
fn plpgsql_assignment_mode_rejects_invalid_syntax_and_modes() {
    for (sql, nnames) in [
        ("item := 1", 0),
        ("item := 1", 4),
        (":= 1", 1),
        ("item 1", 1),
        ("item[] := 1", 1),
        ("item := 1; select 2", 1),
    ] {
        assert!(
            parse_plpgsql_assignment(sql, nnames).is_err(),
            "expected ParseError for {sql:?} in mode {nnames}"
        );
    }
}

#[test]
fn remaining_raw_parse_modes_parse_plpgsql_expressions_and_type_names() {
    let raw = parse_plpgsql_expression(
        "distinct value from app.items where active order by value fetch first 1 row only",
    )
    .expect("parse PL/pgSQL expression mode");
    let select = expect_node!(raw.stmt.as_deref(), Some(SelectStmt));
    assert_eq!(select.distinct_clause.len(), 1);
    assert_eq!(select.target_list.len(), 1);
    assert_eq!(select.from_clause.len(), 1);
    assert!(select.where_clause.is_some());
    assert_eq!(select.sort_clause.len(), 1);
    assert!(select.limit_count.is_some());

    let empty = parse_plpgsql_expression("").expect("empty PL/pgSQL expression production");
    assert!(matches!(
        empty.stmt.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));

    let type_name = parse_type_name("timestamp(3) with time zone[]").expect("parse type-name mode");
    assert_eq!(type_name.names.len(), 2);
    assert_eq!(type_name.typmods.len(), 1);
    assert_eq!(type_name.array_bounds.len(), 1);

    assert!(parse_plpgsql_expression("1; select 2").is_err());
    assert!(parse_type_name("int garbage").is_err());
}
