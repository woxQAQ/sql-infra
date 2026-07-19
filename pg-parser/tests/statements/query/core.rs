use super::*;

#[test]
fn raw_select_statement_locations_follow_postgresql_semicolon_rules() {
    let sql = "  select 1; \n select 2";
    let statements = pg_parser::parse(sql).expect("parse statements");
    assert_eq!(statements.len(), 2);
    let first: &pg_parser::RawStmt = &statements[0];
    assert!(matches!(first.stmt.as_deref(), Some(Node::SelectStmt(_))));
    let first_start = sql.find("select 1").unwrap();
    let second_start = sql.find("select 2").unwrap();
    assert_eq!(first.stmt_location as usize, first_start);
    assert_eq!(
        first.stmt_len as usize,
        sql.find(';').unwrap() - first_start
    );
    assert_eq!(statements[1].stmt_location as usize, second_start);
    assert_eq!(statements[1].stmt_len, 0);

    let terminated = pg_parser::parse("select 1;").expect("parse terminated statement");
    assert_eq!(terminated[0].stmt_location, 0);
    assert_eq!(terminated[0].stmt_len, 8);
}

#[test]
fn select_stmt_populates_query_clauses() {
    let stmt = parse_node!(
        "select distinct a, b + 1 as next_b from app.items where active = true group by a, b having count(*) > 0 order by a desc nulls last limit 10 offset 2",
        SelectStmt
    );

    assert!(!stmt.distinct_clause.is_empty());
    assert_eq!(stmt.target_list.len(), 2);
    assert_eq!(stmt.from_clause.len(), 1);
    assert!(stmt.where_clause.is_some());
    assert_eq!(stmt.group_clause.len(), 2);
    assert!(stmt.having_clause.is_some());
    assert_eq!(stmt.sort_clause.len(), 1);
    assert!(stmt.limit_count.is_some());
    assert!(stmt.limit_offset.is_some());
}

#[test]
fn select_stmt_populates_into_clause_relation_and_persistence() {
    let stmt = parse_node!(
        "select id into temporary table app.snapshot from app.items",
        SelectStmt
    );
    let into = stmt.into_clause.as_deref().expect("INTO clause");
    let relation = into.rel.as_deref().expect("INTO relation");
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("snapshot"));
    assert_eq!(relation.relpersistence, b't');
    assert!(
        matches!(stmt.from_clause.first(), Some(Node::RangeVar(range)) if range.relpersistence == b'p')
    );

    let stmt = parse_node!("select 1 into unlogged snapshot", SelectStmt);
    assert_eq!(
        stmt.into_clause
            .as_deref()
            .and_then(|into| into.rel.as_deref())
            .map(|relation| relation.relpersistence),
        Some(b'u')
    );
}

#[test]
fn select_limit_offset_and_fetch_follow_exact_grammar_forms() {
    let stmt = parse_node!("select 1 limit all offset 2 rows", SelectStmt);
    assert!(matches!(
        stmt.limit_count.as_deref(),
        Some(Node::AConst(value)) if value.isnull
    ));
    assert!(stmt.limit_offset.is_some());

    let stmt = parse_node!("select 1 fetch first row only", SelectStmt);
    assert!(matches!(
        stmt.limit_count.as_deref(),
        Some(Node::AConst(value))
            if matches!(value.val, ValUnion::Integer(ref integer) if integer.ival == 1)
    ));
    assert_eq!(stmt.limit_option, pg_parser::LimitOption::Count);

    let stmt = parse_node!(
        "select 1 order by 1 fetch next 5 rows with ties",
        SelectStmt
    );
    assert!(stmt.limit_count.is_some());
    assert_eq!(stmt.limit_option, pg_parser::LimitOption::WithTies);

    let offset_then_limit = parse_node!("select 1 offset 2 rows limit 5", SelectStmt);
    assert!(offset_then_limit.limit_offset.is_some());
    assert!(offset_then_limit.limit_count.is_some());

    let offset_then_fetch =
        parse_node!("select 1 offset 2 rows fetch next 3 rows only", SelectStmt);
    assert!(offset_then_fetch.limit_offset.is_some());
    assert!(offset_then_fetch.limit_count.is_some());

    let locking_then_limit = parse_node!(
        "select * from items for update of items nowait offset 2 limit 5",
        SelectStmt
    );
    assert_eq!(locking_then_limit.locking_clause.len(), 1);
    assert!(locking_then_limit.limit_offset.is_some());
    assert!(locking_then_limit.limit_count.is_some());

    let read_only_then_limit = parse_node!("select * from items for read only limit 5", SelectStmt);
    assert!(read_only_then_limit.locking_clause.is_empty());
    assert!(read_only_then_limit.limit_count.is_some());

    for sql in [
        "select 1 fetch first +2 rows only",
        "select 1 fetch next -2.5 rows only",
        "select 1 offset +2 rows",
        "select 1 offset -2.5 rows",
        "select 1 fetch first (a + b) rows only",
        "select 1 offset (a + b) rows",
    ] {
        let stmt = parse_node!(sql, SelectStmt);
        assert!(
            stmt.limit_count.is_some() || stmt.limit_offset.is_some(),
            "{sql}"
        );
    }
}

#[test]
fn select_without_targets_is_allowed_only_by_the_plain_select_production() {
    let empty = parse_node!("select", SelectStmt);
    assert!(empty.target_list.is_empty());
    assert!(empty.from_clause.is_empty());

    let all = parse_node!("select all", SelectStmt);
    assert!(all.target_list.is_empty());

    let union = parse_node!("select union all select", SelectStmt);
    assert_eq!(union.op, SetOperation::Union);
    assert!(union.all);
    assert!(
        union
            .larg
            .as_deref()
            .is_some_and(|side| side.target_list.is_empty())
    );
    assert!(
        union
            .rarg
            .as_deref()
            .is_some_and(|side| side.target_list.is_empty())
    );

    let into = parse_node!("select into temporary empty_table", SelectStmt);
    assert!(into.target_list.is_empty());
    assert!(into.into_clause.is_some());

    let filtered = parse_node!("select where true", SelectStmt);
    assert!(filtered.target_list.is_empty());
    assert!(filtered.where_clause.is_some());

    let grouped = parse_node!("select group by 1 having true window w as ()", SelectStmt);
    assert!(grouped.target_list.is_empty());
    assert_eq!(grouped.group_clause.len(), 1);
    assert!(grouped.having_clause.is_some());
    assert_eq!(grouped.window_clause.len(), 1);

    let ordered = parse_node!("select order by 1 limit 1 for update", SelectStmt);
    assert!(ordered.target_list.is_empty());
    assert_eq!(ordered.sort_clause.len(), 1);
    assert!(ordered.limit_count.is_some());
    assert_eq!(ordered.locking_clause.len(), 1);

    let stmt = parse_node!("select from items", SelectStmt);
    assert!(stmt.target_list.is_empty());
    assert_eq!(stmt.from_clause.len(), 1);
}

#[test]
fn select_core_reference_and_target_locations_follow_the_expression_start() {
    let sql = "select app.f(t.value) as result, t.other label, * from t";
    let stmt = parse_node!(sql, SelectStmt);
    let [
        Node::ResTarget(function_target),
        Node::ResTarget(column_target),
        Node::ResTarget(star_target),
    ] = stmt.target_list.as_slice()
    else {
        panic!("expected three ResTarget nodes");
    };
    assert_eq!(
        function_target.location as usize,
        sql.find("app.f").unwrap()
    );
    let function = expect_node!(function_target.val.as_deref(), Some(FuncCall));
    assert_eq!(function.location as usize, sql.find("app.f").unwrap());
    let [Node::ColumnRef(argument)] = function.args.as_slice() else {
        panic!("expected ColumnRef argument");
    };
    assert_eq!(argument.location as usize, sql.find("t.value").unwrap());

    assert_eq!(
        column_target.location as usize,
        sql.find("t.other").unwrap()
    );
    let column = expect_node!(column_target.val.as_deref(), Some(ColumnRef));
    assert_eq!(column.location as usize, sql.find("t.other").unwrap());
    assert_eq!(star_target.location as usize, sql.find('*').unwrap());
    let star = expect_node!(star_target.val.as_deref(), Some(ColumnRef));
    assert_eq!(star.location as usize, sql.find('*').unwrap());
}

#[test]
fn select_order_by_preserves_using_operator_direction_and_nulls() {
    let sql =
        "select a, b from t order by a using < nulls first, b desc nulls last, f(direction) asc";
    let stmt = parse_node!(sql, SelectStmt);
    assert_eq!(stmt.sort_clause.len(), 3);
    let using = expect_node!(&stmt.sort_clause[0], SortBy);
    assert_eq!(using.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(using.sortby_nulls, pg_parser::SortByNulls::First);
    assert!(matches!(
        using.use_op.as_slice(),
        [Node::String(value)] if value.sval.as_deref() == Some("<")
    ));
    assert_eq!(using.location as usize, sql.find("< nulls").unwrap());
    let desc = expect_node!(&stmt.sort_clause[1], SortBy);
    assert_eq!(desc.sortby_dir, pg_parser::SortByDir::Desc);
    assert_eq!(desc.sortby_nulls, pg_parser::SortByNulls::Last);
    assert_eq!(desc.location, -1);
    let function = expect_node!(&stmt.sort_clause[2], SortBy);
    assert_eq!(function.sortby_dir, pg_parser::SortByDir::Asc);
    assert_eq!(function.location, -1);
    assert!(matches!(function.node.as_deref(), Some(Node::FuncCall(_))));
}

#[test]
fn select_stmt_builds_grouping_sets_rollup_cube_and_group_by_all() {
    let sql = "select a, b, c, d from t group by distinct grouping sets ((a, b), rollup(c, d), cube(a, c), ())";
    let stmt = parse_node!(sql, SelectStmt);
    assert!(stmt.group_distinct);
    let [Node::GroupingSet(sets)] = stmt.group_clause.as_slice() else {
        panic!("expected top-level GroupingSet");
    };
    assert_eq!(sets.kind, GroupingSetKind::Sets);
    assert_eq!(sets.location as usize, sql.find("grouping sets").unwrap());
    assert_eq!(sets.content.len(), 4);
    assert!(matches!(
        sets.content[0],
        Node::RowExpr(ref row) if row.row_format == pg_parser::CoercionForm::ImplicitCast
    ));
    let rollup = expect_node!(&sets.content[1], GroupingSet);
    assert_eq!(rollup.kind, GroupingSetKind::Rollup);
    assert_eq!(rollup.content.len(), 2);
    assert_eq!(rollup.location as usize, sql.find("rollup").unwrap());
    let cube = expect_node!(&sets.content[2], GroupingSet);
    assert_eq!(cube.kind, GroupingSetKind::Cube);
    assert_eq!(cube.content.len(), 2);
    assert_eq!(cube.location as usize, sql.find("cube").unwrap());
    let empty = expect_node!(&sets.content[3], GroupingSet);
    assert_eq!(empty.kind, GroupingSetKind::Empty);
    assert!(empty.content.is_empty());
    assert_eq!(empty.location as usize, sql.rfind("()").unwrap());

    let stmt = parse_node!("select count(*) from t group by all", SelectStmt);
    assert!(stmt.group_by_all);
    assert!(stmt.group_clause.is_empty());
}

#[test]
fn select_stmt_respects_set_operation_precedence_and_associativity() {
    let except = parse_node!("select 1 except select 2 except select 3", SelectStmt);
    assert_eq!(
        set_shape(&except),
        "Except(Except(leaf,leaf),leaf)",
        "EXCEPT must be left associative"
    );

    let intersect = parse_node!("select 1 intersect select 2 union select 3", SelectStmt);
    assert_eq!(intersect.op, SetOperation::Union);
    assert_eq!(
        set_shape(&intersect),
        "Union(Intersect(leaf,leaf),leaf)",
        "INTERSECT must bind more tightly than UNION"
    );
}

#[test]
fn select_stmt_supports_parenthesized_top_level_and_set_operands() {
    let stmt = parse_node!(
        "(select 1 order by 1 limit 1) union all (select 2 order by 1 limit 1)",
        SelectStmt
    );
    assert_eq!(stmt.op, SetOperation::Union);
    assert!(stmt.all);
    let left = stmt.larg.as_deref().expect("left operand");
    assert_eq!(left.sort_clause.len(), 1);
    assert!(left.limit_count.is_some());
    let right = stmt.rarg.as_deref().expect("right operand");
    assert_eq!(right.sort_clause.len(), 1);
    assert!(right.limit_count.is_some());

    let stmt = parse_node!("select 1 union (select 2 order by 1)", SelectStmt);
    assert_eq!(stmt.op, SetOperation::Union);
    assert_eq!(
        stmt.rarg
            .as_deref()
            .expect("right operand")
            .sort_clause
            .len(),
        1
    );
}

#[test]
fn select_stmt_parses_values_and_table_forms() {
    let values = parse_node!("values (1, 'a'), (2, 'b')", SelectStmt);
    assert_eq!(values.values_lists.len(), 2);

    let table = parse_node!("table public.items", SelectStmt);
    assert_eq!(table.from_clause.len(), 1);
    let [Node::ResTarget(target)] = table.target_list.as_slice() else {
        panic!("TABLE must build an implicit SELECT * target");
    };
    assert!(matches!(
        target.val.as_deref(),
        Some(Node::ColumnRef(column))
            if matches!(column.fields.as_slice(), [Node::AStar(_)])
                && column.location == -1
    ));

    let only = parse_node!("table only (public.items)", SelectStmt);
    assert!(matches!(
        only.from_clause.as_slice(),
        [Node::RangeVar(range)] if !range.inh && range.alias.is_none()
    ));

    let explicit_inheritance = parse_node!("select i.id from public.items * as i", SelectStmt);
    assert!(matches!(
        explicit_inheritance.from_clause.as_slice(),
        [Node::RangeVar(range)]
            if range.inh
                && matches!(range.alias.as_deref(), Some(alias) if alias.aliasname.as_deref() == Some("i"))
    ));

    let only_with_alias = parse_node!("select i.id from only (public.items) as i", SelectStmt);
    assert!(matches!(
        only_with_alias.from_clause.as_slice(),
        [Node::RangeVar(range)] if !range.inh && range.alias.is_some()
    ));
}

#[test]
fn tooling_statement_ranges_are_complete_with_or_without_semicolons() {
    let sql = "  select 1; \n select 中文  ";
    let statements = pg_parser::parse_with_ranges(sql).expect("parse statements with ranges");
    assert_eq!(statements.len(), 2);

    let first_start = sql.find("select 1").unwrap();
    let semicolon = sql.find(';').unwrap();
    assert_eq!(usize::from(statements[0].range.syntax.start()), first_start);
    assert_eq!(usize::from(statements[0].range.syntax.end()), semicolon);
    assert_eq!(
        statements[0].range.terminator,
        Some(pg_parser::TextRange::new(
            pg_parser::TextSize::try_from(semicolon).unwrap(),
            pg_parser::TextSize::try_from(semicolon + 1).unwrap(),
        ))
    );

    let second_start = sql.find("select 中文").unwrap();
    assert_eq!(
        usize::from(statements[1].range.syntax.start()),
        second_start
    );
    assert_eq!(usize::from(statements[1].range.syntax.end()), sql.len());
    assert_eq!(statements[1].range.terminator, None);
    assert_eq!(statements[1].raw.stmt_len, 0);
}

#[test]
fn parser_errors_use_the_unexpected_token_range() {
    let sql = "select 1 trailing extra";
    let error = pg_parser::parse(sql).unwrap_err();
    let start = sql.find("trailing").unwrap();
    assert_eq!(usize::from(error.range.start()), start);
    assert_eq!(usize::from(error.range.end()), start + "trailing".len());
}
