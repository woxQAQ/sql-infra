use super::*;

#[test]
fn declare_cursor_accepts_the_grammar_valid_empty_select_query() {
    let stmt = parse_node!("declare c cursor for select", DeclareCursorStmt);
    assert!(matches!(
        stmt.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));
}

#[test]
fn prepare_and_explain_accept_the_grammar_valid_empty_select() {
    let prepare = parse_node!("prepare empty_plan as select", PrepareStmt);
    assert!(matches!(
        prepare.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));

    let explain = parse_node!("explain select", ExplainStmt);
    assert!(matches!(
        explain.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));
}

#[test]
fn cursor_statements_populate_options_direction_counts_and_names() {
    let declare = parse_node!(
        "declare item_cursor binary scroll cursor with hold for select id from app.items",
        DeclareCursorStmt
    );
    assert_eq!(declare.portalname.as_deref(), Some("item_cursor"));
    assert_eq!(
        declare.options,
        CURSOR_OPT_BINARY | CURSOR_OPT_SCROLL | CURSOR_OPT_HOLD | CURSOR_OPT_FAST_PLAN
    );
    assert!(matches!(
        declare.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    let fetch = parse_node!("fetch backward all from item_cursor", FetchStmt);
    assert_eq!(fetch.direction, FetchDirection::Backward);
    assert_eq!(fetch.how_many, i64::MAX);
    assert_eq!(fetch.direction_keyword, FetchDirectionKeywords::BackwardAll);
    assert!(!fetch.ismove);
    assert_eq!(fetch.location, -1);

    let movement = parse_node!("move absolute -2 in item_cursor", FetchStmt);
    assert_eq!(movement.direction, FetchDirection::Absolute);
    assert_eq!(movement.how_many, -2);
    assert!(movement.ismove);
    assert_eq!(movement.location, 14);

    let close = parse_node!("close all", ClosePortalStmt);
    assert!(close.portalname.is_none());

    let close = parse_node!("close item_cursor", ClosePortalStmt);
    assert_eq!(close.portalname.as_deref(), Some("item_cursor"));

    let all_modes = parse_node!(
        "declare mode_cursor no scroll insensitive asensitive cursor without hold for select 1",
        DeclareCursorStmt
    );
    assert_eq!(
        all_modes.options,
        CURSOR_OPT_NO_SCROLL
            | CURSOR_OPT_INSENSITIVE
            | CURSOR_OPT_ASENSITIVE
            | CURSOR_OPT_FAST_PLAN
    );

    for sql in [
        "declare c cursor for with source as (select 1 as id) select id from source",
        "declare c cursor for with source as (select 1 as id) (select id from source)",
        "declare c cursor for (select 1 union select 2)",
        "declare c cursor for values (1), (2)",
        "declare c cursor for table app.items",
    ] {
        let stmt = parse_node!(sql, DeclareCursorStmt);
        assert!(
            matches!(stmt.query.as_deref(), Some(Node::SelectStmt(_))),
            "{sql}"
        );
    }
}

#[test]
fn fetch_and_move_locations_follow_fetch_args_productions() {
    let cases = [
        ("fetch item_cursor", FetchDirectionKeywords::None, 1, -1),
        (
            "fetch from item_cursor",
            FetchDirectionKeywords::None,
            1,
            -1,
        ),
        (
            "fetch next from item_cursor",
            FetchDirectionKeywords::Next,
            1,
            -1,
        ),
        (
            "fetch prior in item_cursor",
            FetchDirectionKeywords::Prior,
            1,
            -1,
        ),
        (
            "fetch first item_cursor",
            FetchDirectionKeywords::First,
            1,
            -1,
        ),
        (
            "fetch last from item_cursor",
            FetchDirectionKeywords::Last,
            -1,
            -1,
        ),
        (
            "fetch all from item_cursor",
            FetchDirectionKeywords::All,
            i64::MAX,
            -1,
        ),
        (
            "fetch forward from item_cursor",
            FetchDirectionKeywords::Forward,
            1,
            -1,
        ),
        (
            "fetch forward all from item_cursor",
            FetchDirectionKeywords::ForwardAll,
            i64::MAX,
            -1,
        ),
        (
            "move backward in item_cursor",
            FetchDirectionKeywords::Backward,
            1,
            -1,
        ),
    ];
    for (sql, keyword, count, location) in cases {
        let stmt = parse_node!(sql, FetchStmt);
        assert_eq!(stmt.direction_keyword, keyword, "{sql}");
        assert_eq!(stmt.how_many, count, "{sql}");
        assert_eq!(stmt.location, location, "{sql}");
    }

    for (sql, location) in [
        ("fetch 3 from item_cursor", 6),
        ("fetch absolute -2 from item_cursor", 15),
        ("fetch relative +4 from item_cursor", 15),
        ("move forward 7 in item_cursor", 13),
        ("move backward -8 item_cursor", 14),
    ] {
        let stmt = parse_node!(sql, FetchStmt);
        assert_eq!(stmt.location, location, "{sql}");
    }

    for (sql, direction, count, keyword) in [
        (
            "fetch prior c",
            FetchDirection::Backward,
            1,
            FetchDirectionKeywords::Prior,
        ),
        (
            "fetch first c",
            FetchDirection::Absolute,
            1,
            FetchDirectionKeywords::First,
        ),
        (
            "fetch relative -3 c",
            FetchDirection::Relative,
            -3,
            FetchDirectionKeywords::Relative,
        ),
        (
            "move forward +5 c",
            FetchDirection::Forward,
            5,
            FetchDirectionKeywords::Forward,
        ),
        (
            "move backward 6 c",
            FetchDirection::Backward,
            6,
            FetchDirectionKeywords::Backward,
        ),
    ] {
        let stmt = parse_node!(sql, FetchStmt);
        assert_eq!(stmt.direction, direction, "{sql}");
        assert_eq!(stmt.how_many, count, "{sql}");
        assert_eq!(stmt.direction_keyword, keyword, "{sql}");
    }
}

#[test]
fn prepare_execute_and_deallocate_require_and_store_complete_payloads() {
    let prepare = parse_node!(
        "prepare find_order (int, text) as select * from orders where id = $1",
        PrepareStmt
    );
    assert_eq!(prepare.name.as_deref(), Some("find_order"));
    assert_eq!(prepare.argtypes.len(), 2);
    assert!(matches!(
        prepare.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    let execute = parse_node!("execute find_order(7, 'open')", ExecuteStmt);
    assert_eq!(execute.name.as_deref(), Some("find_order"));
    assert_eq!(execute.params.len(), 2);

    let named = parse_node!("deallocate prepare find_order", DeallocateStmt);
    assert_eq!(named.name.as_deref(), Some("find_order"));
    assert!(!named.isall);
    assert_eq!(named.location, 19);

    let all = parse_node!("deallocate all", DeallocateStmt);
    assert!(all.name.is_none());
    assert!(all.isall);
    assert_eq!(all.location, -1);

    let prepared_all = parse_node!("deallocate prepare all", DeallocateStmt);
    assert!(prepared_all.name.is_none());
    assert!(prepared_all.isall);
    assert_eq!(prepared_all.location, -1);

    let named = parse_node!("deallocate find_order", DeallocateStmt);
    assert_eq!(named.location, 11);

    for (sql, expected_query) in [
        ("prepare p as (select 1)", "SelectStmt"),
        ("prepare p as values (1)", "SelectStmt"),
        ("prepare p as insert into t values (1)", "InsertStmt"),
        ("prepare p as update t set id = 1", "UpdateStmt"),
        ("prepare p as delete from t", "DeleteStmt"),
        (
            "prepare p as merge into t using s on true when matched then do nothing",
            "MergeStmt",
        ),
    ] {
        let prepare = parse_node!(sql, PrepareStmt);
        let actual_query = match prepare.query.as_deref() {
            Some(Node::SelectStmt(_)) => "SelectStmt",
            Some(Node::InsertStmt(_)) => "InsertStmt",
            Some(Node::UpdateStmt(_)) => "UpdateStmt",
            Some(Node::DeleteStmt(_)) => "DeleteStmt",
            Some(Node::MergeStmt(_)) => "MergeStmt",
            other => panic!("unexpected prepared query for {sql}: {other:?}"),
        };
        assert_eq!(actual_query, expected_query, "{sql}");
    }
}
