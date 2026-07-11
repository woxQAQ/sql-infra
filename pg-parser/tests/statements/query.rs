use pg_parser::{
    AExprKind, Alias, BoolExprType, BoolTestType, CteCycleClause, CteMaterialize, CteSearchClause,
    FRAMEOPTION_BETWEEN, FRAMEOPTION_EXCLUDE_TIES, FRAMEOPTION_ROWS,
    FRAMEOPTION_START_OFFSET_PRECEDING, GraphElementPatternKind, GroupingSetKind, JoinType,
    JsonBehavior, JsonBehaviorType, JsonEncoding, JsonExprOp, JsonFormatType, JsonQuotes,
    JsonReturning, JsonTableColumnType, JsonTablePathSpec, JsonValueType, JsonWrapper,
    LockClauseStrength, LockWaitPolicy, MinMaxOp, Node, NullTestType, SetOperation, ValUnion,
    WithClause, XmlExprOp,
};

use super::common::parse_statement;

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

fn set_shape(stmt: &pg_parser::SelectStmt) -> String {
    if let (Some(left), Some(right)) = (&stmt.larg, &stmt.rarg) {
        format!("{:?}({},{})", stmt.op, set_shape(left), set_shape(right))
    } else {
        "leaf".to_owned()
    }
}

#[test]
fn select_stmt_populates_query_clauses() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select distinct a, b + 1 as next_b from app.items where active = true group by a, b having count(*) > 0 order by a desc nulls last limit 10 offset 2",
    ) else {
        panic!("expected SelectStmt");
    };

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
    let Node::SelectStmt(stmt) =
        parse_statement("select id into temporary table app.snapshot from app.items")
    else {
        panic!("expected SelectStmt");
    };
    let into = stmt.into_clause.as_deref().expect("INTO clause");
    let relation = into.rel.as_deref().expect("INTO relation");
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("snapshot"));
    assert_eq!(relation.relpersistence, b't');
    assert!(
        matches!(stmt.from_clause.first(), Some(Node::RangeVar(range)) if range.relpersistence == b'p')
    );

    let Node::SelectStmt(stmt) = parse_statement("select 1 into unlogged snapshot") else {
        panic!("expected SelectStmt");
    };
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
    let Node::SelectStmt(stmt) = parse_statement("select 1 limit all offset 2 rows") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.limit_count.as_deref(),
        Some(Node::AConst(value)) if value.isnull
    ));
    assert!(stmt.limit_offset.is_some());

    let Node::SelectStmt(stmt) = parse_statement("select 1 fetch first row only") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.limit_count.as_deref(),
        Some(Node::AConst(value))
            if matches!(value.val, ValUnion::Integer(ref integer) if integer.ival == 1)
    ));
    assert_eq!(stmt.limit_option, pg_parser::LimitOption::Count);

    let Node::SelectStmt(stmt) = parse_statement("select 1 order by 1 fetch next 5 rows with ties")
    else {
        panic!("expected SelectStmt");
    };
    assert!(stmt.limit_count.is_some());
    assert_eq!(stmt.limit_option, pg_parser::LimitOption::WithTies);

    let Node::SelectStmt(offset_then_limit) = parse_statement("select 1 offset 2 rows limit 5")
    else {
        panic!("expected SelectStmt");
    };
    assert!(offset_then_limit.limit_offset.is_some());
    assert!(offset_then_limit.limit_count.is_some());

    let Node::SelectStmt(offset_then_fetch) =
        parse_statement("select 1 offset 2 rows fetch next 3 rows only")
    else {
        panic!("expected SelectStmt");
    };
    assert!(offset_then_fetch.limit_offset.is_some());
    assert!(offset_then_fetch.limit_count.is_some());

    let Node::SelectStmt(locking_then_limit) =
        parse_statement("select * from items for update of items nowait offset 2 limit 5")
    else {
        panic!("expected SelectStmt");
    };
    assert_eq!(locking_then_limit.locking_clause.len(), 1);
    assert!(locking_then_limit.limit_offset.is_some());
    assert!(locking_then_limit.limit_count.is_some());

    let Node::SelectStmt(read_only_then_limit) =
        parse_statement("select * from items for read only limit 5")
    else {
        panic!("expected SelectStmt");
    };
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
        let Node::SelectStmt(stmt) = parse_statement(sql) else {
            panic!("expected SelectStmt for {sql}");
        };
        assert!(
            stmt.limit_count.is_some() || stmt.limit_offset.is_some(),
            "{sql}"
        );
    }
}

#[test]
fn select_without_targets_is_allowed_only_by_the_plain_select_production() {
    let Node::SelectStmt(empty) = parse_statement("select") else {
        panic!("expected empty SelectStmt");
    };
    assert!(empty.target_list.is_empty());
    assert!(empty.from_clause.is_empty());

    let Node::SelectStmt(all) = parse_statement("select all") else {
        panic!("expected SELECT ALL SelectStmt");
    };
    assert!(all.target_list.is_empty());

    let Node::SelectStmt(with) = parse_statement("with x as (select) select") else {
        panic!("expected WITH SelectStmt");
    };
    assert!(with.with_clause.is_some());
    assert!(with.target_list.is_empty());

    let Node::SelectStmt(union) = parse_statement("select union all select") else {
        panic!("expected set-operation SelectStmt");
    };
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

    let Node::SelectStmt(into) = parse_statement("select into temporary empty_table") else {
        panic!("expected SELECT INTO SelectStmt");
    };
    assert!(into.target_list.is_empty());
    assert!(into.into_clause.is_some());

    let Node::SelectStmt(filtered) = parse_statement("select where true") else {
        panic!("expected filtered empty-target SelectStmt");
    };
    assert!(filtered.target_list.is_empty());
    assert!(filtered.where_clause.is_some());

    let Node::SelectStmt(grouped) = parse_statement("select group by 1 having true window w as ()")
    else {
        panic!("expected grouped empty-target SelectStmt");
    };
    assert!(grouped.target_list.is_empty());
    assert_eq!(grouped.group_clause.len(), 1);
    assert!(grouped.having_clause.is_some());
    assert_eq!(grouped.window_clause.len(), 1);

    let Node::SelectStmt(ordered) = parse_statement("select order by 1 limit 1 for update") else {
        panic!("expected ordered empty-target SelectStmt");
    };
    assert!(ordered.target_list.is_empty());
    assert_eq!(ordered.sort_clause.len(), 1);
    assert!(ordered.limit_count.is_some());
    assert_eq!(ordered.locking_clause.len(), 1);

    let Node::SelectStmt(stmt) = parse_statement("select from items") else {
        panic!("expected SelectStmt");
    };
    assert!(stmt.target_list.is_empty());
    assert_eq!(stmt.from_clause.len(), 1);
}

#[test]
fn select_core_reference_and_target_locations_follow_the_expression_start() {
    let sql = "select app.f(t.value) as result, t.other label, * from t";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
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
    let Some(Node::FuncCall(function)) = function_target.val.as_deref() else {
        panic!("expected FuncCall");
    };
    assert_eq!(function.location as usize, sql.find("app.f").unwrap());
    let [Node::ColumnRef(argument)] = function.args.as_slice() else {
        panic!("expected ColumnRef argument");
    };
    assert_eq!(argument.location as usize, sql.find("t.value").unwrap());

    assert_eq!(
        column_target.location as usize,
        sql.find("t.other").unwrap()
    );
    let Some(Node::ColumnRef(column)) = column_target.val.as_deref() else {
        panic!("expected ColumnRef target");
    };
    assert_eq!(column.location as usize, sql.find("t.other").unwrap());
    assert_eq!(star_target.location as usize, sql.find('*').unwrap());
    let Some(Node::ColumnRef(star)) = star_target.val.as_deref() else {
        panic!("expected star ColumnRef");
    };
    assert_eq!(star.location as usize, sql.find('*').unwrap());
}

#[test]
fn select_cast_collation_and_operator_locations_follow_grammar_tokens() {
    let sql = "select value::setof app.kind collate app.c, cast(value as numeric(4, 2)), a + b";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let target_value = |index: usize| {
        let Node::ResTarget(target) = &stmt.target_list[index] else {
            panic!("expected ResTarget");
        };
        target.val.as_deref().expect("target value")
    };

    let Node::CollateClause(collation) = target_value(0) else {
        panic!("expected CollateClause");
    };
    assert_eq!(collation.location as usize, sql.find("collate").unwrap());
    let Some(Node::TypeCast(postfix)) = collation.arg.as_deref() else {
        panic!("expected postfix TypeCast");
    };
    assert_eq!(postfix.location as usize, sql.find("::").unwrap());
    let postfix_type = postfix.type_name.as_deref().expect("postfix TypeName");
    assert!(postfix_type.setof);
    assert_eq!(
        postfix_type.location as usize,
        sql.find("app.kind").unwrap()
    );

    let Node::TypeCast(cast) = target_value(1) else {
        panic!("expected CAST TypeCast");
    };
    assert_eq!(cast.location as usize, sql.find("cast").unwrap());
    assert_eq!(
        cast.type_name.as_deref().expect("CAST TypeName").location as usize,
        sql.find("numeric").unwrap()
    );

    let Node::AExpr(operator) = target_value(2) else {
        panic!("expected operator AExpr");
    };
    assert_eq!(operator.location as usize, sql.rfind('+').unwrap());
}

#[test]
fn select_order_by_preserves_using_operator_direction_and_nulls() {
    let sql =
        "select a, b from t order by a using < nulls first, b desc nulls last, f(direction) asc";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.sort_clause.len(), 3);
    let Node::SortBy(using) = &stmt.sort_clause[0] else {
        panic!("expected SortBy");
    };
    assert_eq!(using.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(using.sortby_nulls, pg_parser::SortByNulls::First);
    assert!(matches!(
        using.use_op.as_slice(),
        [Node::String(value)] if value.sval.as_deref() == Some("<")
    ));
    assert_eq!(using.location as usize, sql.find("< nulls").unwrap());
    let Node::SortBy(desc) = &stmt.sort_clause[1] else {
        panic!("expected SortBy");
    };
    assert_eq!(desc.sortby_dir, pg_parser::SortByDir::Desc);
    assert_eq!(desc.sortby_nulls, pg_parser::SortByNulls::Last);
    assert_eq!(desc.location, -1);
    let Node::SortBy(function) = &stmt.sort_clause[2] else {
        panic!("expected SortBy");
    };
    assert_eq!(function.sortby_dir, pg_parser::SortByDir::Asc);
    assert_eq!(function.location, -1);
    assert!(matches!(function.node.as_deref(), Some(Node::FuncCall(_))));
}

#[test]
fn select_stmt_builds_grouping_sets_rollup_cube_and_group_by_all() {
    let sql = "select a, b, c, d from t group by distinct grouping sets ((a, b), rollup(c, d), cube(a, c), ())";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
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
    let Node::GroupingSet(rollup) = &sets.content[1] else {
        panic!("expected ROLLUP GroupingSet");
    };
    assert_eq!(rollup.kind, GroupingSetKind::Rollup);
    assert_eq!(rollup.content.len(), 2);
    assert_eq!(rollup.location as usize, sql.find("rollup").unwrap());
    let Node::GroupingSet(cube) = &sets.content[2] else {
        panic!("expected CUBE GroupingSet");
    };
    assert_eq!(cube.kind, GroupingSetKind::Cube);
    assert_eq!(cube.content.len(), 2);
    assert_eq!(cube.location as usize, sql.find("cube").unwrap());
    let Node::GroupingSet(empty) = &sets.content[3] else {
        panic!("expected empty GroupingSet");
    };
    assert_eq!(empty.kind, GroupingSetKind::Empty);
    assert!(empty.content.is_empty());
    assert_eq!(empty.location as usize, sql.rfind("()").unwrap());

    let Node::SelectStmt(stmt) = parse_statement("select count(*) from t group by all") else {
        panic!("expected SelectStmt");
    };
    assert!(stmt.group_by_all);
    assert!(stmt.group_clause.is_empty());
}

#[test]
fn select_stmt_respects_set_operation_precedence_and_associativity() {
    let Node::SelectStmt(except) = parse_statement("select 1 except select 2 except select 3")
    else {
        panic!("expected SelectStmt");
    };
    assert_eq!(
        set_shape(&except),
        "Except(Except(leaf,leaf),leaf)",
        "EXCEPT must be left associative"
    );

    let Node::SelectStmt(intersect) = parse_statement("select 1 intersect select 2 union select 3")
    else {
        panic!("expected SelectStmt");
    };
    assert_eq!(intersect.op, SetOperation::Union);
    assert_eq!(
        set_shape(&intersect),
        "Union(Intersect(leaf,leaf),leaf)",
        "INTERSECT must bind more tightly than UNION"
    );
}

#[test]
fn select_stmt_supports_parenthesized_top_level_and_set_operands() {
    let Node::SelectStmt(stmt) =
        parse_statement("(select 1 order by 1 limit 1) union all (select 2 order by 1 limit 1)")
    else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.op, SetOperation::Union);
    assert!(stmt.all);
    let left = stmt.larg.as_deref().expect("left operand");
    assert_eq!(left.sort_clause.len(), 1);
    assert!(left.limit_count.is_some());
    let right = stmt.rarg.as_deref().expect("right operand");
    assert_eq!(right.sort_clause.len(), 1);
    assert!(right.limit_count.is_some());

    let Node::SelectStmt(stmt) = parse_statement("select 1 union (select 2 order by 1)") else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.op, SetOperation::Union);
    assert_eq!(
        stmt.rarg
            .as_deref()
            .expect("right operand")
            .sort_clause
            .len(),
        1
    );

    for sql in [
        "with x as (select 1) (select * from x)",
        "with x as (select 1) ((select * from x)) order by 1",
    ] {
        let Node::SelectStmt(stmt) = parse_statement(sql) else {
            panic!("expected WITH parenthesized SelectStmt for {sql}");
        };
        assert!(stmt.with_clause.is_some(), "{sql}");
        assert!(!stmt.target_list.is_empty(), "{sql}");
    }
}

#[test]
fn select_stmt_parses_cte_aliases_and_materialization() {
    let sql = "with recursive source(id, parent_id) as not materialized (select id, parent_id from tree) select id from source";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let with: Box<WithClause> = stmt.with_clause.expect("WITH clause");
    assert!(with.recursive);
    assert_eq!(with.location, sql.find("with").unwrap() as i32);
    let Node::CommonTableExpr(cte) = &with.ctes[0] else {
        panic!("expected CommonTableExpr");
    };
    assert_eq!(cte.ctename.as_deref(), Some("source"));
    assert_eq!(cte.location, sql.find("source").unwrap() as i32);
    assert_eq!(cte.aliascolnames.len(), 2);
    assert_eq!(cte.ctematerialized, CteMaterialize::Never);
    assert!(cte.ctequery.is_some());

    let Node::SelectStmt(quoted) =
        parse_statement("with \"select\"(\"from\") as (select 1) select * from \"select\"")
    else {
        panic!("expected SelectStmt");
    };
    let quoted_with = quoted.with_clause.expect("WithClause");
    let [Node::CommonTableExpr(cte)] = quoted_with.ctes.as_slice() else {
        panic!("expected CommonTableExpr");
    };
    assert_eq!(cte.ctename.as_deref(), Some("select"));
    assert!(matches!(
        cte.aliascolnames.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("from")
    ));

    for name in ["time", "ordinality"] {
        let sql = format!("with {name} as (select 1) select * from {name}");
        let Node::SelectStmt(stmt) = parse_statement(&sql) else {
            panic!("expected lookahead-keyword CTE SelectStmt for {name}");
        };
        let with = stmt.with_clause.expect("WithClause");
        let [Node::CommonTableExpr(cte)] = with.ctes.as_slice() else {
            panic!("expected CommonTableExpr for {name}");
        };
        assert_eq!(cte.ctename.as_deref(), Some(name));
    }
}

#[test]
fn select_stmt_populates_cte_search_and_cycle_clauses() {
    let sql = "with recursive walk(id) as (values (1) union all select id + 1 from walk where id < 3) search breadth first by id set visit_order cycle id set is_cycle to text 'yes' default text 'no' using visit_path select * from walk";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let with = stmt.with_clause.expect("WITH clause");
    let Node::CommonTableExpr(cte) = &with.ctes[0] else {
        panic!("expected CommonTableExpr");
    };
    let search: &CteSearchClause = cte.search_clause.as_deref().expect("SEARCH clause");
    assert!(search.search_breadth_first);
    assert_eq!(search.location, sql.find("search").unwrap() as i32);
    assert_eq!(search.search_col_list.len(), 1);
    assert_eq!(search.search_seq_column.as_deref(), Some("visit_order"));

    let cycle: &CteCycleClause = cte.cycle_clause.as_deref().expect("CYCLE clause");
    assert_eq!(cycle.location, sql.find("cycle").unwrap() as i32);
    assert_eq!(cycle.cycle_col_list.len(), 1);
    assert_eq!(cycle.cycle_mark_column.as_deref(), Some("is_cycle"));
    assert!(matches!(
        cycle.cycle_mark_value.as_deref(),
        Some(Node::TypeCast(_))
    ));
    assert!(matches!(
        cycle.cycle_mark_default.as_deref(),
        Some(Node::TypeCast(_))
    ));
    assert_eq!(cycle.cycle_path_column.as_deref(), Some("visit_path"));

    let Node::SelectStmt(stmt) = parse_statement(
        "with recursive walk(id) as (values (1)) search depth first by id set visit_order cycle id set is_cycle using visit_path select * from walk",
    ) else {
        panic!("expected SelectStmt");
    };
    let with = stmt.with_clause.expect("WITH clause");
    let Node::CommonTableExpr(cte) = &with.ctes[0] else {
        panic!("expected CommonTableExpr");
    };
    assert!(
        !cte.search_clause
            .as_deref()
            .expect("SEARCH clause")
            .search_breadth_first
    );
    let cycle = cte.cycle_clause.as_deref().expect("CYCLE clause");
    assert!(matches!(
        cycle.cycle_mark_value.as_deref(),
        Some(Node::AConst(value)) if matches!(value.val, ValUnion::Boolean(ref boolean) if boolean.boolval)
    ));
    assert!(matches!(
        cycle.cycle_mark_default.as_deref(),
        Some(Node::AConst(value)) if matches!(value.val, ValUnion::Boolean(ref boolean) if !boolean.boolval)
    ));
}

#[test]
fn select_stmt_parses_values_and_table_forms() {
    let Node::SelectStmt(values) = parse_statement("values (1, 'a'), (2, 'b')") else {
        panic!("expected SelectStmt");
    };
    assert_eq!(values.values_lists.len(), 2);

    let Node::SelectStmt(table) = parse_statement("table public.items") else {
        panic!("expected SelectStmt");
    };
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

    let Node::SelectStmt(only) = parse_statement("table only (public.items)") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        only.from_clause.as_slice(),
        [Node::RangeVar(range)] if !range.inh && range.alias.is_none()
    ));

    let Node::SelectStmt(explicit_inheritance) =
        parse_statement("select i.id from public.items * as i")
    else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        explicit_inheritance.from_clause.as_slice(),
        [Node::RangeVar(range)]
            if range.inh
                && matches!(range.alias.as_deref(), Some(alias) if alias.aliasname.as_deref() == Some("i"))
    ));

    let Node::SelectStmt(only_with_alias) =
        parse_statement("select i.id from only (public.items) as i")
    else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        only_with_alias.from_clause.as_slice(),
        [Node::RangeVar(range)] if !range.inh && range.alias.is_some()
    ));
}

#[test]
fn select_stmt_builds_raw_case_null_boolean_and_default_expression_nodes() {
    let Node::SelectStmt(stmt) =
        parse_statement("select case when a is null then default else 0 end, flag is not true")
    else {
        panic!("expected SelectStmt");
    };
    let Node::ResTarget(case_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(case_value) = &case_target.val else {
        panic!("expected CaseExpr");
    };
    let Node::CaseExpr(case) = case_value.as_ref() else {
        panic!("expected CaseExpr");
    };
    let Node::CaseWhen(when) = &case.args[0] else {
        panic!("expected CaseWhen");
    };
    assert!(matches!(
        when.expr.as_deref(),
        Some(Node::NullTest(test))
            if test.nulltesttype == NullTestType::Null && !test.argisrow
    ));
    assert!(matches!(
        when.result.as_deref(),
        Some(Node::SetToDefault(_))
    ));
    assert!(matches!(case.defresult.as_deref(), Some(Node::AConst(_))));

    let Node::SelectStmt(without_else) = parse_statement("select case value when 1 then 'one' end")
    else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        without_else.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::CaseExpr(case)) if case.arg.is_some() && case.defresult.is_none())
    ));

    let Node::ResTarget(boolean_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        boolean_target.val.as_deref(),
        Some(Node::BooleanTest(test)) if test.booltesttype == BoolTestType::NotTrue
    ));
}

#[test]
fn select_stmt_builds_named_args_grouping_and_sql_value_functions() {
    let sql = "select f(first => 1, second := 2), grouping(category), current_timestamp(3)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::ResTarget(function_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(Node::FuncCall(function)) = function_target.val.as_deref() else {
        panic!("expected FuncCall");
    };
    assert_eq!(function.args.len(), 2);
    assert!(
        function
            .args
            .iter()
            .all(|arg| matches!(arg, Node::NamedArgExpr(_)))
    );
    assert!(
        function
            .args
            .iter()
            .all(|arg| matches!(arg, Node::NamedArgExpr(named) if named.argnumber == -1))
    );
    let [Node::NamedArgExpr(first), Node::NamedArgExpr(second)] = function.args.as_slice() else {
        panic!("expected NamedArgExpr arguments");
    };
    assert_eq!(first.location as usize, sql.find("first").unwrap());
    assert_eq!(second.location as usize, sql.find("second").unwrap());

    let Node::ResTarget(grouping_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        grouping_target.val.as_deref(),
        Some(Node::GroupingFunc(_))
    ));

    let Node::ResTarget(timestamp_target) = &stmt.target_list[2] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        timestamp_target.val.as_deref(),
        Some(Node::SqlValueFunction(value))
            if value.op == pg_parser::SqlValueFunctionOp::CurrentTimestampN
                && value.typmod == 3
    ));
}

#[test]
fn select_stmt_builds_range_table_sample() {
    let sql = "select * from items tablesample system(10) repeatable(42)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeTableSample(sample) = &stmt.from_clause[0] else {
        panic!("expected RangeTableSample");
    };
    assert!(sample.relation.is_some());
    assert_eq!(sample.method.len(), 1);
    assert_eq!(sample.args.len(), 1);
    assert!(sample.repeatable.is_some());
    assert_eq!(sample.location as usize, sql.find("system").unwrap());
}

#[test]
fn select_stmt_builds_rows_from_function_pairs_and_column_definitions() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select * from rows from (f(1) as (a int, b text collate c), g(2)) with ordinality as rf",
    ) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeFunction(range) = &stmt.from_clause[0] else {
        panic!("expected RangeFunction");
    };
    assert!(range.is_rowsfrom);
    assert!(range.ordinality);
    assert_eq!(range.functions.len(), 2);
    assert_eq!(
        range
            .alias
            .as_deref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("rf")
    );
    let Node::AArrayExpr(first_pair) = &range.functions[0] else {
        panic!("expected function/coldef pair");
    };
    assert!(matches!(first_pair.elements[0], Node::FuncCall(_)));
    let Node::AArrayExpr(coldefs) = &first_pair.elements[1] else {
        panic!("expected column definition list");
    };
    assert_eq!(coldefs.elements.len(), 2);
    assert!(matches!(
        coldefs.elements[1],
        Node::ColumnDef(ref column) if column.coll_clause.is_some()
    ));
    let Node::AArrayExpr(second_pair) = &range.functions[1] else {
        panic!("expected function/coldef pair");
    };
    assert!(matches!(
        second_pair.elements[1],
        Node::AArrayExpr(ref coldefs) if coldefs.elements.is_empty()
    ));
}

#[test]
fn select_range_function_preserves_function_pairs_alias_columns_and_coldefs() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select * from f() as named(a, b), g() as typed(x int), h() as (y text), q(distinct a order by b) as agg",
    ) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.from_clause.len(), 4);
    let ranges = stmt
        .from_clause
        .iter()
        .map(|item| match item {
            Node::RangeFunction(range) => range,
            other => panic!("expected RangeFunction, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(!ranges[0].is_rowsfrom);
    assert!(matches!(
        ranges[0].functions.as_slice(),
        [Node::AArrayExpr(pair)] if pair.elements.len() == 2
    ));
    let named = ranges[0].alias.as_deref().expect("named alias");
    assert_eq!(named.aliasname.as_deref(), Some("named"));
    assert_eq!(named.colnames.len(), 2);
    assert!(ranges[0].coldeflist.is_empty());
    assert_eq!(ranges[1].coldeflist.len(), 1);
    assert_eq!(
        ranges[1]
            .alias
            .as_deref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("typed")
    );
    assert!(ranges[2].alias.is_none());
    assert_eq!(ranges[2].coldeflist.len(), 1);
    let Node::AArrayExpr(aggregate_pair) = &ranges[3].functions[0] else {
        panic!("expected function pair");
    };
    assert!(matches!(
        aggregate_pair.elements[0],
        Node::FuncCall(ref call) if call.agg_distinct && call.agg_order.len() == 1
    ));

    let Node::SelectStmt(quoted) = parse_statement("select * from f() as \"select\"(\"from\")")
    else {
        panic!("expected SelectStmt");
    };
    let [Node::RangeFunction(range)] = quoted.from_clause.as_slice() else {
        panic!("expected RangeFunction");
    };
    let alias = range.alias.as_deref().expect("Alias");
    assert_eq!(alias.aliasname.as_deref(), Some("select"));
    assert!(matches!(
        alias.colnames.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("from")
    ));

    let Node::SelectStmt(lateral) = parse_statement("select * from lateral f() as lf") else {
        panic!("expected lateral RangeFunction SelectStmt");
    };
    let [Node::RangeFunction(range)] = lateral.from_clause.as_slice() else {
        panic!("expected lateral RangeFunction");
    };
    assert!(range.lateral);

    let Node::SelectStmt(keyword_alias) = parse_statement("select abort.value from f() abort")
    else {
        panic!("expected keyword-aliased RangeFunction SelectStmt");
    };
    let [Node::RangeFunction(range)] = keyword_alias.from_clause.as_slice() else {
        panic!("expected keyword-aliased RangeFunction");
    };
    assert_eq!(
        range
            .alias
            .as_deref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("abort")
    );

    let Node::SelectStmt(keyword_aliases) =
        parse_statement("select value.a from items value(a), other abort")
    else {
        panic!("expected keyword aliases SelectStmt");
    };
    let [Node::RangeVar(first), Node::RangeVar(second)] = keyword_aliases.from_clause.as_slice()
    else {
        panic!("expected aliased RangeVars");
    };
    let first_alias = first.alias.as_deref().expect("VALUE alias");
    assert_eq!(first_alias.aliasname.as_deref(), Some("value"));
    assert_eq!(first_alias.colnames.len(), 1);
    assert_eq!(
        second
            .alias
            .as_deref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("abort")
    );
}

#[test]
fn select_stmt_builds_xml_expression_and_serialize_nodes() {
    let sql = "select xmlelement(name item, xmlattributes(id as item_id), name), xmlforest(id as item_id, name), xmlserialize(content xmlparse(content '<a/>' preserve whitespace) as text indent)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::ResTarget(element_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(Node::XmlExpr(element)) = element_target.val.as_deref() else {
        panic!("expected XmlExpr");
    };
    assert_eq!(element.name.as_deref(), Some("item"));
    assert_eq!(element.named_args.len(), 1);
    let [Node::ResTarget(attribute)] = element.named_args.as_slice() else {
        panic!("expected XML attribute ResTarget");
    };
    assert_eq!(
        attribute.location as usize,
        sql.find("id as item_id").unwrap()
    );
    assert!(element.arg_names.is_empty());
    assert_eq!(element.args.len(), 1);
    assert_eq!(element.node_tag, 0);
    assert_eq!(element.typmod, 0);

    let Node::ResTarget(forest_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    let Some(Node::XmlExpr(forest)) = forest_target.val.as_deref() else {
        panic!("expected XmlExpr");
    };
    assert_eq!(forest.named_args.len(), 2);
    let [Node::ResTarget(id), Node::ResTarget(name)] = forest.named_args.as_slice() else {
        panic!("expected XMLFOREST ResTarget nodes");
    };
    assert_eq!(id.location as usize, sql.rfind("id as item_id").unwrap());
    assert_eq!(
        name.location as usize,
        sql.find("name), xmlserialize").unwrap()
    );
    assert!(forest.arg_names.is_empty());

    let Node::ResTarget(serialize_target) = &stmt.target_list[2] else {
        panic!("expected ResTarget");
    };
    let Some(Node::XmlSerialize(serialize)) = serialize_target.val.as_deref() else {
        panic!("expected XmlSerialize");
    };
    assert!(serialize.indent);
    assert!(matches!(serialize.expr.as_deref(), Some(Node::XmlExpr(_))));
    assert!(serialize.type_name.is_some());
    assert_eq!(
        serialize.location as usize,
        sql.find("xmlserialize").unwrap()
    );

    let Node::SelectStmt(reserved_label) =
        parse_statement("select xmlelement(name select), xmlforest(id as from)")
    else {
        panic!("expected reserved XML label SelectStmt");
    };
    assert!(matches!(
        reserved_label.target_list.as_slice(),
        [Node::ResTarget(element), Node::ResTarget(forest)]
            if matches!(element.val.as_deref(), Some(Node::XmlExpr(expr)) if expr.name.as_deref() == Some("select"))
                && matches!(forest.val.as_deref(), Some(Node::XmlExpr(expr))
                    if matches!(expr.named_args.as_slice(), [Node::ResTarget(target)] if target.name.as_deref() == Some("from")))
    ));
}

#[test]
fn select_xmlroot_always_preserves_the_raw_standalone_argument() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select xmlroot(doc, version '1.0'), xmlroot(doc, version '1.0', standalone yes), xmlroot(doc, version '1.0', standalone no), xmlroot(doc, version '1.0', standalone no value)",
    ) else {
        panic!("expected SelectStmt");
    };
    let standalone_values = stmt
        .target_list
        .iter()
        .map(|target| {
            let Node::ResTarget(target) = target else {
                panic!("expected ResTarget");
            };
            let Some(Node::XmlExpr(expression)) = target.val.as_deref() else {
                panic!("expected XmlExpr");
            };
            assert_eq!(expression.op, XmlExprOp::Xmlroot);
            assert_eq!(expression.args.len(), 3);
            let Node::AConst(value) = &expression.args[2] else {
                panic!("expected standalone AConst");
            };
            assert_eq!(value.location, -1);
            let ValUnion::Integer(value) = &value.val else {
                panic!("expected standalone integer");
            };
            value.ival
        })
        .collect::<Vec<_>>();
    assert_eq!(standalone_values, [3, 0, 1, 2]);
}

#[test]
fn select_stmt_builds_xmltable_range_and_column_nodes() {
    let sql = "select * from xmltable(xmlnamespaces('urn:items' as item_ns), '/items/item' passing document_xml columns ord for ordinality, id int path '@id' not null, name text default 'unknown' path 'name') as item_rows";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeTableFunc(table) = &stmt.from_clause[0] else {
        panic!("expected RangeTableFunc");
    };
    assert!(table.rowexpr.is_some());
    assert!(table.docexpr.is_some());
    assert!(!table.lateral);
    assert_eq!(table.location, sql.find("xmltable").unwrap() as i32);
    assert_eq!(table.namespaces.len(), 1);
    assert_eq!(table.columns.len(), 3);
    assert_eq!(
        table
            .alias
            .as_ref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("item_rows")
    );

    let Node::RangeTableFuncCol(ordinality) = &table.columns[0] else {
        panic!("expected RangeTableFuncCol");
    };
    assert!(ordinality.for_ordinality);
    assert_eq!(ordinality.colname.as_deref(), Some("ord"));
    assert_eq!(
        ordinality.location,
        sql.find("ord for ordinality").unwrap() as i32
    );

    let Node::RangeTableFuncCol(id) = &table.columns[1] else {
        panic!("expected RangeTableFuncCol");
    };
    assert!(id.type_name.is_some());
    assert!(id.colexpr.is_some());
    assert!(id.is_not_null);
    assert_eq!(id.location as usize, sql.find("id int path").unwrap());

    let Node::RangeTableFuncCol(name) = &table.columns[2] else {
        panic!("expected RangeTableFuncCol");
    };
    assert!(name.coldefexpr.is_some());
    assert!(name.colexpr.is_some());
    assert_eq!(name.location as usize, sql.find("name text").unwrap());

    for passing in [
        "passing doc",
        "passing doc by ref",
        "passing by value doc",
        "passing by ref doc by value",
    ] {
        let sql = format!("select * from xmltable('/x' {passing} columns id int)");
        let Node::SelectStmt(stmt) = parse_statement(&sql) else {
            panic!("expected SelectStmt for {sql}");
        };
        assert!(matches!(
            stmt.from_clause.as_slice(),
            [Node::RangeTableFunc(table)]
                if table.docexpr.is_some() && table.columns.len() == 1
        ));
    }

    let sql = "select * from xmltable(xmlnamespaces(1 is distinct from 2 as cmp, default (true and false)), '/x' passing doc columns compared boolean path 1 = 1, grouped boolean default (true and false), nullable text path null)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeTableFunc(table) = &stmt.from_clause[0] else {
        panic!("expected RangeTableFunc");
    };
    assert_eq!(table.namespaces.len(), 2);
    assert!(matches!(
        table.namespaces.as_slice(),
        [Node::ResTarget(named), Node::ResTarget(default)]
            if named.name.as_deref() == Some("cmp")
                && matches!(named.val.as_deref(), Some(Node::AExpr(expr)) if expr.kind == AExprKind::Distinct)
                && default.name.is_none()
                && matches!(default.val.as_deref(), Some(Node::BoolExpr(_)))
    ));
    let [Node::ResTarget(named), Node::ResTarget(default)] = table.namespaces.as_slice() else {
        panic!("expected namespace ResTarget nodes");
    };
    assert_eq!(named.location as usize, sql.find("1 is distinct").unwrap());
    assert_eq!(
        default.location as usize,
        sql.find("default (true").unwrap()
    );
    assert!(matches!(
        table.columns.as_slice(),
        [Node::RangeTableFuncCol(compared), Node::RangeTableFuncCol(grouped), Node::RangeTableFuncCol(nullable)]
            if matches!(compared.colexpr.as_deref(), Some(Node::AExpr(_)))
                && matches!(grouped.coldefexpr.as_deref(), Some(Node::BoolExpr(_)))
                && matches!(nullable.colexpr.as_deref(), Some(Node::AConst(value)) if value.isnull)
    ));

    let Node::SelectStmt(parenthesized) = parse_statement(
        "select * from xmltable(('/x' || '/item') passing (doc_a || doc_b) columns id int)",
    ) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeTableFunc(table) = &parenthesized.from_clause[0] else {
        panic!("expected RangeTableFunc");
    };
    assert!(matches!(table.rowexpr.as_deref(), Some(Node::AExpr(_))));
    assert!(matches!(table.docexpr.as_deref(), Some(Node::AExpr(_))));

    let sql = "select * from lateral xmltable('/x' passing doc columns id int) as xt";
    let Node::SelectStmt(lateral) = parse_statement(sql) else {
        panic!("expected lateral XMLTABLE SelectStmt");
    };
    let [Node::RangeTableFunc(table)] = lateral.from_clause.as_slice() else {
        panic!("expected lateral RangeTableFunc");
    };
    assert!(table.lateral);
    assert_eq!(table.location, sql.find("xmltable").unwrap() as i32);
}

#[test]
fn select_stmt_builds_graph_table_pattern_and_elements() {
    let sql = "select * from graph_table(social match (person is person_label)-[edge is knows]->(friend is person_label) where person.active = true columns (person.id as person_id, friend.id as friend_id)) as graph_rows";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::RangeGraphTable(table) = &stmt.from_clause[0] else {
        panic!("expected RangeGraphTable");
    };
    assert!(table.graph_name.is_some());
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.location as usize, sql.find("graph_table").unwrap());
    assert_eq!(
        table
            .alias
            .as_ref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("graph_rows")
    );
    let pattern = table.graph_pattern.as_ref().expect("GraphPattern");
    assert_eq!(pattern.path_pattern_list.len(), 1);
    assert!(pattern.where_clause.is_some());
    let Node::AArrayExpr(path) = &pattern.path_pattern_list[0] else {
        panic!("expected graph path list");
    };
    assert_eq!(path.elements.len(), 3);
    assert!(
        path.elements
            .iter()
            .all(|element| matches!(element, Node::GraphElementPattern(_)))
    );
    let Node::GraphElementPattern(vertex) = &path.elements[0] else {
        panic!("expected vertex GraphElementPattern");
    };
    assert_eq!(vertex.location as usize, sql.find("(person").unwrap());
    let Node::GraphElementPattern(edge) = &path.elements[1] else {
        panic!("expected edge GraphElementPattern");
    };
    assert_eq!(edge.location as usize, sql.find("-[edge").unwrap());

    let Node::SelectStmt(nested_stmt) = parse_statement(
        "select * from graph_table(social match ((person is person_label | employee)-[edge]->(friend)){1,2} columns (person.id as person_id))",
    ) else {
        panic!("expected nested graph SelectStmt");
    };
    let Node::RangeGraphTable(nested_table) = &nested_stmt.from_clause[0] else {
        panic!("expected nested RangeGraphTable");
    };
    let nested_pattern = nested_table
        .graph_pattern
        .as_ref()
        .expect("nested GraphPattern");
    let Node::AArrayExpr(nested_path) = &nested_pattern.path_pattern_list[0] else {
        panic!("expected nested graph path list");
    };
    let [Node::GraphElementPattern(parenthesized)] = nested_path.elements.as_slice() else {
        panic!("expected one parenthesized graph element");
    };
    assert_eq!(parenthesized.kind, GraphElementPatternKind::ParenExpr);
    assert_eq!(parenthesized.subexpr.len(), 3);
    assert!(matches!(
        parenthesized.subexpr.first(),
        Some(Node::GraphElementPattern(vertex))
            if vertex.variable.as_deref() == Some("person")
                && matches!(
                vertex.labelexpr.as_deref(),
                Some(Node::BoolExpr(disjunction))
                    if disjunction.boolop == BoolExprType::OrExpr
                        && disjunction.args.len() == 2
            )
    ));
    assert!(matches!(
        parenthesized.quantifier.as_slice(),
        [Node::Integer(lower), Node::Integer(upper)]
            if lower.ival == 1 && upper.ival == 2
    ));

    let abbreviated_sql = "select * from graph_table(social match
        (a)->{2}(b),
        (c)<-{,3}(d),
        (e)-{4}(f)
        columns (a.id))";
    let Node::SelectStmt(abbreviated) = parse_statement(abbreviated_sql) else {
        panic!("expected abbreviated-edge SelectStmt");
    };
    let Node::RangeGraphTable(table) = &abbreviated.from_clause[0] else {
        panic!("expected abbreviated-edge RangeGraphTable");
    };
    let pattern = table.graph_pattern.as_ref().expect("GraphPattern");
    let expected = [
        (GraphElementPatternKind::EdgePatternRight, 2, 2, "->{2}"),
        (GraphElementPatternKind::EdgePatternLeft, 0, 3, "<-{,3}"),
        (GraphElementPatternKind::EdgePatternAny, 4, 4, "-{4}"),
    ];
    for (path, (kind, lower, upper, needle)) in pattern.path_pattern_list.iter().zip(expected) {
        let Node::AArrayExpr(path) = path else {
            panic!("expected abbreviated graph path");
        };
        let Node::GraphElementPattern(edge) = &path.elements[1] else {
            panic!("expected abbreviated edge GraphElementPattern");
        };
        assert_eq!(edge.kind, kind);
        assert!(matches!(
            edge.quantifier.as_slice(),
            [Node::Integer(actual_lower), Node::Integer(actual_upper)]
                if actual_lower.ival == lower && actual_upper.ival == upper
        ));
        assert_eq!(
            edge.location as usize,
            abbreviated_sql.find(needle).unwrap()
        );
    }
}

#[test]
fn select_stmt_builds_sql_json_constructor_and_aggregate_nodes() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select json(doc format json encoding utf8 with unique keys), json_scalar(42), json_serialize(doc format json returning text format json), json_object('id' value id absent on null with unique keys returning jsonb), json_array(id, name null on null returning jsonb), json_array(select id from items), json_objectagg(name value id absent on null with unique keys returning jsonb), json_arrayagg(id order by name absent on null returning jsonb), merge_action() from items",
    ) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.target_list.len(), 9);

    let Node::ResTarget(parse_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonParseExpr(parse)) = parse_target.val.as_deref() else {
        panic!("expected JsonParseExpr");
    };
    assert!(parse.unique_keys);
    let value = parse.expr.as_ref().expect("JsonValueExpr");
    assert!(matches!(
        value.raw_expr.as_deref(),
        Some(Node::ColumnRef(_))
    ));
    let format = value.format.as_ref().expect("JsonFormat");
    assert_eq!(format.format_type, JsonFormatType::Json);
    assert_eq!(format.encoding, JsonEncoding::Utf8);

    let Node::ResTarget(scalar_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        scalar_target.val.as_deref(),
        Some(Node::JsonScalarExpr(_))
    ));

    let Node::ResTarget(serialize_target) = &stmt.target_list[2] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonSerializeExpr(serialize)) = serialize_target.val.as_deref() else {
        panic!("expected JsonSerializeExpr");
    };
    let output = serialize.output.as_ref().expect("JsonOutput");
    assert!(output.type_name.is_some());
    let returning: &JsonReturning = output.returning.as_deref().expect("JsonReturning");
    assert!(returning.format.is_some());
    assert_eq!(
        output
            .returning
            .as_ref()
            .and_then(|returning| returning.format.as_ref())
            .map(|format| format.format_type),
        Some(JsonFormatType::Json)
    );

    let Node::ResTarget(object_target) = &stmt.target_list[3] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonObjectConstructor(object)) = object_target.val.as_deref() else {
        panic!("expected JsonObjectConstructor");
    };
    assert!(object.absent_on_null);
    assert!(object.unique);
    assert!(object.output.is_some());
    assert!(matches!(
        object.exprs.first(),
        Some(Node::JsonKeyValue(pair))
            if matches!(pair.key.as_deref(), Some(Node::AConst(_)))
    ));

    let Node::ResTarget(array_target) = &stmt.target_list[4] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonArrayConstructor(array)) = array_target.val.as_deref() else {
        panic!("expected JsonArrayConstructor");
    };
    assert_eq!(array.exprs.len(), 2);
    assert!(!array.absent_on_null);
    assert!(
        array
            .exprs
            .iter()
            .all(|expr| matches!(expr, Node::JsonValueExpr(_)))
    );

    let Node::ResTarget(query_array_target) = &stmt.target_list[5] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonArrayQueryConstructor(query_array)) = query_array_target.val.as_deref()
    else {
        panic!("expected JsonArrayQueryConstructor");
    };
    assert!(matches!(
        query_array.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));
    assert!(matches!(
        query_array.format.as_deref(),
        Some(format)
            if format.format_type == JsonFormatType::Default && format.location == -1
    ));

    let Node::ResTarget(object_agg_target) = &stmt.target_list[6] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonObjectAgg(object_agg)) = object_agg_target.val.as_deref() else {
        panic!("expected JsonObjectAgg");
    };
    assert!(object_agg.constructor.is_some());
    assert!(object_agg.arg.is_some());

    let Node::ResTarget(array_agg_target) = &stmt.target_list[7] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonArrayAgg(array_agg)) = array_agg_target.val.as_deref() else {
        panic!("expected JsonArrayAgg");
    };
    assert_eq!(
        array_agg
            .constructor
            .as_ref()
            .expect("JsonAggConstructor")
            .agg_order
            .len(),
        1
    );

    let Node::ResTarget(merge_action_target) = &stmt.target_list[8] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        merge_action_target.val.as_deref(),
        Some(Node::MergeSupportFunc(function))
            if function.msftype == 25 && function.msfcollid == 0
    ));
}

#[test]
fn select_legacy_json_object_builds_a_system_function_call() {
    let Node::SelectStmt(stmt) =
        parse_statement("select json_object(key_array => array['id'], value_array := array[42])")
    else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::FuncCall(call))
                if call.funcformat == pg_parser::CoercionForm::ExplicitCall
                    && call.args.len() == 2
                    && call.args.iter().all(|argument| matches!(argument, Node::NamedArgExpr(_)))
                    && matches!(call.funcname.as_slice(), [Node::String(schema), Node::String(name)]
                        if schema.sval.as_deref() == Some("pg_catalog")
                            && name.sval.as_deref() == Some("json_object")))
    ));
}

#[test]
fn select_json_value_expr_default_format_uses_synthetic_location() {
    let Node::SelectStmt(stmt) = parse_statement("select json_array(value returning jsonb)") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::JsonArrayConstructor(constructor))
                if matches!(constructor.exprs.as_slice(), [Node::JsonValueExpr(value)]
                    if matches!(value.format.as_deref(), Some(format)
                        if format.format_type == JsonFormatType::Default
                            && format.encoding == JsonEncoding::Default
                            && format.location == -1))
                    && matches!(constructor.output.as_deref(), Some(output)
                        if matches!(output.returning.as_deref(), Some(returning)
                            if matches!(returning.format.as_deref(), Some(format)
                                if format.format_type == JsonFormatType::Default
                                    && format.encoding == JsonEncoding::Default
                                    && format.location == -1))))
    ));
}

#[test]
fn select_json_array_query_preserves_format_and_returning_clauses() {
    let sql = "select json_array(select id from items format json returning jsonb format json)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::JsonArrayQueryConstructor(constructor))
                if constructor.query.is_some()
                    && constructor.absent_on_null
                    && matches!(constructor.format.as_deref(), Some(format)
                        if format.format_type == JsonFormatType::Json
                            && format.location as usize == sql.find("format json").unwrap())
                    && matches!(constructor.output.as_deref(), Some(output)
                        if output.type_name.is_some()
                            && matches!(output.returning.as_deref(), Some(returning)
                                if matches!(returning.format.as_deref(), Some(format)
                                    if format.format_type == JsonFormatType::Json))))
    ));
}

#[test]
fn select_stmt_builds_sql_json_function_arguments_and_behaviors() {
    let sql = "select json_query(doc format json, '$.item' passing threshold as select returning text format json with conditional array wrapper keep quotes on scalar string null on empty error on error), json_exists(doc, '$.item' passing threshold as threshold true on error), json_value(doc, '$.item' returning int default 0 on empty error on error)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };

    let Node::ResTarget(query_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonFuncExpr(query)) = query_target.val.as_deref() else {
        panic!("expected JsonFuncExpr");
    };
    assert_eq!(query.op, JsonExprOp::QueryOp);
    assert!(query.context_item.is_some());
    assert!(matches!(query.pathspec.as_deref(), Some(Node::AConst(_))));
    assert_eq!(query.location, 7);
    assert_eq!(query.passing.len(), 1);
    assert!(matches!(
        query.passing[0],
        Node::JsonArgument(ref argument) if argument.name.as_deref() == Some("select")
    ));
    assert_eq!(query.wrapper, JsonWrapper::Conditional);
    assert_eq!(query.quotes, JsonQuotes::Keep);
    let on_empty: &JsonBehavior = query.on_empty.as_deref().expect("ON EMPTY behavior");
    assert_eq!(on_empty.btype, JsonBehaviorType::Null);
    assert_eq!(on_empty.location, sql.find("null on empty").unwrap() as i32);
    assert_eq!(
        query.on_empty.as_ref().map(|behavior| behavior.btype),
        Some(JsonBehaviorType::Null)
    );
    assert_eq!(
        query.on_error.as_ref().map(|behavior| behavior.btype),
        Some(JsonBehaviorType::Error)
    );

    let Node::ResTarget(exists_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonFuncExpr(exists)) = exists_target.val.as_deref() else {
        panic!("expected JsonFuncExpr");
    };
    assert_eq!(exists.op, JsonExprOp::ExistsOp);
    assert!(exists.output.is_none());
    assert_eq!(
        exists.on_error.as_ref().map(|behavior| behavior.btype),
        Some(JsonBehaviorType::True)
    );

    let Node::ResTarget(value_target) = &stmt.target_list[2] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonFuncExpr(value)) = value_target.val.as_deref() else {
        panic!("expected JsonFuncExpr");
    };
    assert_eq!(value.op, JsonExprOp::ValueOp);
    assert_eq!(
        value.on_empty.as_ref().map(|behavior| behavior.btype),
        Some(JsonBehaviorType::Default)
    );
    assert!(matches!(
        value
            .on_empty
            .as_deref()
            .and_then(|behavior| behavior.expr.as_deref()),
        Some(Node::AConst(_))
    ));
}

#[test]
fn select_stmt_builds_json_table_and_all_column_kinds() {
    let sql = "select * from lateral json_table(doc format json, '$.items[*]' as root passing 7 as threshold columns (ord for ordinality, id int path '$.id', payload jsonb format json path '$' with conditional array wrapper keep quotes null on empty error on error, present boolean exists path '$.id' false on error, nested path '$.children[*]' as child columns (child_id int path '$.id'))) as rows";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let Node::JsonTable(table) = &stmt.from_clause[0] else {
        panic!("expected JsonTable");
    };
    assert!(table.lateral);
    assert!(table.context_item.is_some());
    assert_eq!(table.passing.len(), 1);
    assert_eq!(table.columns.len(), 5);
    let pathspec: &JsonTablePathSpec = table.pathspec.as_ref().expect("root path spec");
    assert!(pathspec.string.is_some());
    assert_eq!(
        table
            .pathspec
            .as_ref()
            .and_then(|path| path.name.as_deref()),
        Some("root")
    );
    assert_eq!(pathspec.name_location as usize, sql.find("root").unwrap());
    assert_eq!(
        table
            .alias
            .as_ref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("rows")
    );
    let alias: &Alias = table.alias.as_deref().expect("JSON_TABLE alias");
    assert_eq!(alias.aliasname.as_deref(), Some("rows"));

    let expected = [
        JsonTableColumnType::ForOrdinality,
        JsonTableColumnType::Regular,
        JsonTableColumnType::Formatted,
        JsonTableColumnType::Exists,
        JsonTableColumnType::Nested,
    ];
    for (column, expected_type) in table.columns.iter().zip(expected) {
        let Node::JsonTableColumn(column) = column else {
            panic!("expected JsonTableColumn");
        };
        assert_eq!(column.coltype, expected_type);
    }

    let Node::JsonTableColumn(formatted) = &table.columns[2] else {
        panic!("expected formatted JsonTableColumn");
    };
    assert_eq!(formatted.wrapper, JsonWrapper::Conditional);
    assert_eq!(formatted.quotes, JsonQuotes::Keep);
    assert!(formatted.on_empty.is_some());
    assert!(formatted.on_error.is_some());

    let Node::JsonTableColumn(nested) = &table.columns[4] else {
        panic!("expected nested JsonTableColumn");
    };
    assert_eq!(nested.columns.len(), 1);
}

#[test]
fn select_stmt_builds_strict_join_and_locking_nodes() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select a.id from app.a a left join app.b b using (id) as matched natural join app.c c for no key update of app.a, app.b skip locked",
    ) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.from_clause.len(), 1);
    let Node::JoinExpr(outer) = &stmt.from_clause[0] else {
        panic!("expected outer JoinExpr");
    };
    assert!(outer.is_natural);
    assert!(matches!(outer.rarg.as_deref(), Some(Node::RangeVar(_))));
    let Some(Node::JoinExpr(inner)) = outer.larg.as_deref() else {
        panic!("expected inner JoinExpr");
    };
    assert_eq!(inner.using_clause.len(), 1);
    assert!(matches!(inner.rarg.as_deref(), Some(Node::RangeVar(_))));
    assert_eq!(
        inner
            .join_using_alias
            .as_deref()
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("matched")
    );

    assert_eq!(stmt.locking_clause.len(), 1);
    let Node::LockingClause(lock) = &stmt.locking_clause[0] else {
        panic!("expected LockingClause");
    };
    assert_eq!(lock.strength, LockClauseStrength::Fornokeyupdate);
    assert_eq!(lock.wait_policy, LockWaitPolicy::Skip);
    assert_eq!(lock.locked_rels.len(), 2);
    assert!(
        lock.locked_rels
            .iter()
            .all(|rel| matches!(rel, Node::RangeVar(_)))
    );

    let Node::SelectStmt(multiple) =
        parse_statement("select * from app.a, app.b for update of app.a for share of app.b")
    else {
        panic!("expected SelectStmt");
    };
    assert_eq!(multiple.locking_clause.len(), 2);

    let Node::SelectStmt(on_chain) = parse_statement(
        "select * from app.a a join app.b b on a.id = b.id left join app.c c on b.id = c.id",
    ) else {
        panic!("expected chained ON-join SelectStmt");
    };
    let [Node::JoinExpr(outer)] = on_chain.from_clause.as_slice() else {
        panic!("expected outer chained JoinExpr");
    };
    assert!(outer.quals.is_some());
    assert!(matches!(outer.larg.as_deref(), Some(Node::JoinExpr(inner)) if inner.quals.is_some()));
}

#[test]
fn select_stmt_populates_window_frame_bounds_and_exclusion() {
    let sql = "select v from measurements window w as (partition by sensor order by measured_at rows between 2 preceding and current row exclude ties)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.window_clause.len(), 1);
    let Node::WindowDef(window) = &stmt.window_clause[0] else {
        panic!("expected WindowDef");
    };
    assert_eq!(window.name.as_deref(), Some("w"));
    assert_eq!(window.partition_clause.len(), 1);
    assert_eq!(window.order_clause.len(), 1);
    assert_ne!(window.frame_options & FRAMEOPTION_ROWS, 0);
    assert_ne!(window.frame_options & FRAMEOPTION_BETWEEN, 0);
    assert_ne!(window.frame_options & FRAMEOPTION_START_OFFSET_PRECEDING, 0);
    assert_ne!(window.frame_options & FRAMEOPTION_EXCLUDE_TIES, 0);
    assert!(window.start_offset.is_some());
    assert!(window.end_offset.is_none());
    assert_eq!(window.location as usize, sql.find("(partition").unwrap());

    let Node::SelectStmt(quoted) =
        parse_statement("select count(*) over \"select\" window \"select\" as ()")
    else {
        panic!("expected quoted-window SelectStmt");
    };
    let Node::ResTarget(target) = &quoted.target_list[0] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        target.val.as_deref(),
        Some(Node::FuncCall(call))
            if call.over.as_deref().and_then(|window| window.name.as_deref()) == Some("select")
    ));
    let Some(Node::FuncCall(call)) = target.val.as_deref() else {
        panic!("expected FuncCall");
    };
    let over = call.over.as_deref().expect("OVER WindowDef");
    assert_eq!(
        over.location as usize,
        "select count(*) over \"select\" window \"select\" as ()"
            .find("\"select\"")
            .unwrap()
    );
    assert!(matches!(
        quoted.window_clause.as_slice(),
        [Node::WindowDef(window)] if window.name.as_deref() == Some("select")
    ));

    let Node::SelectStmt(inherited) = parse_statement(
        "select sum(v) over (derived rows unbounded preceding)
         from measurements
         window base as (partition by sensor), derived as (base order by measured_at)",
    ) else {
        panic!("expected inherited-window SelectStmt");
    };
    assert!(matches!(
        inherited.window_clause.as_slice(),
        [Node::WindowDef(base), Node::WindowDef(derived)]
            if base.name.as_deref() == Some("base")
                && base.refname.is_none()
                && derived.name.as_deref() == Some("derived")
                && derived.refname.as_deref() == Some("base")
    ));
    let Node::ResTarget(inherited_target) = &inherited.target_list[0] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        inherited_target.val.as_deref(),
        Some(Node::FuncCall(call))
            if call.over.as_deref().and_then(|window| window.refname.as_deref()) == Some("derived")
    ));
}

#[test]
fn select_exposes_core_raw_expression_and_range_node_shapes() {
    let sql = "select *, $1, coalesce(a, 0), greatest(a, b), row(a, b), (select 1), a collate c, a::int from generate_series(1, 2) g, (select 1) s where a = 1 and b = 2 order by a desc nulls last";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.target_list.len(), 8);
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::ColumnRef(column) if matches!(column.fields.as_slice(), [Node::AStar(_)])
    ));
    assert!(matches!(
        values[1],
        Node::ParamRef(param) if param.number == 1 && param.location == 10
    ));
    assert!(matches!(values[2], Node::CoalesceExpr(expr) if expr.args.len() == 2));
    assert!(matches!(
        values[3],
        Node::MinMaxExpr(expr)
            if expr.op == MinMaxOp::Greatest
                && expr.args.len() == 2
                && expr.location == sql.find("greatest").unwrap() as i32
    ));
    assert!(matches!(values[4], Node::RowExpr(expr) if expr.args.len() == 2));
    assert!(matches!(
        values[5],
        Node::SubLink(link) if link.subselect.is_some() && link.sub_link_id == 0
    ));
    assert!(matches!(
        values[6],
        Node::CollateClause(clause) if matches!(clause.arg.as_deref(), Some(Node::ColumnRef(_)))
    ));
    assert!(matches!(values[7], Node::TypeCast(cast) if cast.type_name.is_some()));

    assert_eq!(stmt.from_clause.len(), 2);
    assert!(matches!(stmt.from_clause[0], Node::RangeFunction(_)));
    assert!(matches!(
        &stmt.from_clause[1],
        Node::RangeSubselect(range)
            if matches!(range.subquery.as_deref(), Some(Node::SelectStmt(_)))
    ));
    assert!(matches!(
        stmt.where_clause.as_deref(),
        Some(Node::BoolExpr(_))
    ));
    assert!(matches!(stmt.sort_clause[0], Node::SortBy(_)));

    let Node::SelectStmt(nested_lateral) =
        parse_statement("select * from lateral ((select 1)) as nested(value)")
    else {
        panic!("expected nested lateral subquery SelectStmt");
    };
    let [Node::RangeSubselect(range)] = nested_lateral.from_clause.as_slice() else {
        panic!("expected RangeSubselect");
    };
    assert!(range.lateral);
    let alias = range.alias.as_deref().expect("nested subquery alias");
    assert_eq!(alias.aliasname.as_deref(), Some("nested"));
    assert_eq!(alias.colnames.len(), 1);

    let Node::SelectStmt(flat) = parse_statement("select 1 where a and b and c or d or e") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        flat.where_clause.as_deref(),
        Some(Node::BoolExpr(disjunction))
            if disjunction.boolop == BoolExprType::OrExpr
                && disjunction.args.len() == 3
                && matches!(
                    disjunction.args.first(),
                    Some(Node::BoolExpr(conjunction))
                        if conjunction.boolop == BoolExprType::AndExpr
                            && conjunction.args.len() == 3
                )
    ));

    let Node::SelectStmt(qualified) = parse_statement("select app.select") else {
        panic!("expected qualified-name SelectStmt");
    };
    assert!(matches!(
        qualified.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(
                target.val.as_deref(),
                Some(Node::ColumnRef(column)) if column.fields.len() == 2
            )
    ));

    let Node::SelectStmt(aliases) = parse_statement("select 1 answer, 2 as select") else {
        panic!("expected aliased SelectStmt");
    };
    assert!(matches!(
        aliases.target_list.as_slice(),
        [Node::ResTarget(bare), Node::ResTarget(explicit)]
            if bare.name.as_deref() == Some("answer")
                && explicit.name.as_deref() == Some("select")
    ));
}

#[test]
fn select_join_expr_preserves_type_qualification_and_raw_defaults() {
    let Node::SelectStmt(stmt) =
        parse_statement("select * from app.a left join app.b on app.a.id = app.b.id")
    else {
        panic!("expected SelectStmt");
    };
    let [Node::JoinExpr(join)] = stmt.from_clause.as_slice() else {
        panic!("expected JoinExpr");
    };
    assert_eq!(join.jointype, JoinType::Left);
    assert!(matches!(join.quals.as_deref(), Some(Node::AExpr(_))));
    assert!(join.using_clause.is_empty());
    assert!(!join.is_natural);
    assert_eq!(join.rtindex, 0);
}

#[test]
fn select_nullif_preserves_postgresql_raw_aexpr_fields() {
    let Node::SelectStmt(stmt) = parse_statement("select nullif(left_value, right_value)") else {
        panic!("expected SelectStmt");
    };
    assert!(matches!(
        stmt.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(
                target.val.as_deref(),
                Some(Node::AExpr(expression))
                    if expression.kind == AExprKind::Nullif
                        && matches!(
                            expression.name.as_slice(),
                            [Node::String(name)] if name.sval.as_deref() == Some("=")
                        )
                        && matches!(expression.lexpr.as_deref(), Some(Node::ColumnRef(_)))
                        && matches!(expression.rexpr.as_deref(), Some(Node::ColumnRef(_)))
            )
    ));
}

#[test]
fn select_builds_array_indices_slices_and_expression_indirection() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select items[1], items[2:4], items[:], items[1:2][3].field, (row(a, b)).field, $1[1]",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();

    let Node::AIndirection(single) = values[0] else {
        panic!("expected AIndirection");
    };
    assert!(matches!(single.arg.as_deref(), Some(Node::ColumnRef(_))));
    assert!(matches!(
        single.indirection.as_slice(),
        [Node::AIndices(index)] if !index.is_slice && index.lidx.is_none() && index.uidx.is_some()
    ));

    let Node::AIndirection(slice) = values[1] else {
        panic!("expected AIndirection");
    };
    assert!(matches!(
        slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice && index.lidx.is_some() && index.uidx.is_some()
    ));

    let Node::AIndirection(open_slice) = values[2] else {
        panic!("expected AIndirection");
    };
    assert!(matches!(
        open_slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice && index.lidx.is_none() && index.uidx.is_none()
    ));

    let Node::AIndirection(chained) = values[3] else {
        panic!("expected AIndirection");
    };
    assert_eq!(chained.indirection.len(), 3);
    assert!(matches!(chained.indirection[0], Node::AIndices(_)));
    assert!(matches!(chained.indirection[1], Node::AIndices(_)));
    assert!(matches!(chained.indirection[2], Node::String(_)));

    assert!(matches!(
        values[4],
        Node::AIndirection(indirection)
            if matches!(indirection.arg.as_deref(), Some(Node::RowExpr(_)))
    ));
    assert!(matches!(
        values[5],
        Node::AIndirection(indirection)
            if matches!(indirection.arg.as_deref(), Some(Node::ParamRef(_)))
    ));
}

#[test]
fn select_literal_tokens_build_the_correct_a_const_value_variants() {
    let Node::SelectStmt(stmt) = parse_statement("select 1, 1.5, 'text', B'101', X'ff'") else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::AConst(value)) => &value.val,
                other => panic!("expected AConst, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(values[0], ValUnion::Integer(_)));
    assert!(matches!(values[1], ValUnion::Float(_)));
    assert!(matches!(values[2], ValUnion::String(_)));
    assert!(matches!(
        values[3],
        ValUnion::BitString(value) if value.bsval.as_deref() == Some("b101")
    ));
    assert!(matches!(
        values[4],
        ValUnion::BitString(value) if value.bsval.as_deref() == Some("xff")
    ));

    let Node::SelectStmt(signed) = parse_statement("select -42, -1.5, +42") else {
        panic!("expected signed SelectStmt");
    };
    assert!(matches!(
        signed.target_list.as_slice(),
        [Node::ResTarget(integer), Node::ResTarget(float), Node::ResTarget(positive)]
            if matches!(
                integer.val.as_deref(),
                Some(Node::AConst(value))
                    if matches!(&value.val, ValUnion::Integer(number) if number.ival == -42)
            )
                && matches!(
                    float.val.as_deref(),
                    Some(Node::AConst(value))
                        if matches!(&value.val, ValUnion::Float(number) if number.fval.as_deref() == Some("-1.5"))
                )
                && matches!(positive.val.as_deref(), Some(Node::AExpr(_)))
    ));
}

#[test]
fn select_typed_literals_build_raw_type_cast_nodes() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select date '2026-07-10', numeric(5, 2) '12.34', interval '2' day,
                pg_catalog.text 'hello', char 'abc', bit '101', char(3) 'abc', bit(3) '101'",
    ) else {
        panic!("expected SelectStmt");
    };
    assert_eq!(stmt.target_list.len(), 8);
    let mut type_names = Vec::new();
    for target in &stmt.target_list {
        let Node::ResTarget(target) = target else {
            panic!("expected ResTarget");
        };
        let Some(Node::TypeCast(cast)) = target.val.as_deref() else {
            panic!("expected typed literal TypeCast");
        };
        type_names.push(cast.type_name.as_deref().expect("typed literal type"));
        assert!(matches!(cast.arg.as_deref(), Some(Node::AConst(_))));
    }
    assert!(type_names[4].typmods.is_empty());
    assert!(type_names[5].typmods.is_empty());
    assert_eq!(type_names[6].typmods.len(), 1);
    assert_eq!(type_names[7].typmods.len(), 1);
}

#[test]
fn select_casts_preserve_type_modifiers_arrays_and_time_zone() {
    let Node::SelectStmt(stmt) =
        parse_statement("select amount::numeric(10, 2)[], created_at::timestamp(3) with time zone")
    else {
        panic!("expected SelectStmt");
    };
    let casts = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::TypeCast(cast)) => cast,
                other => panic!("expected TypeCast, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    let numeric = casts[0].type_name.as_deref().expect("numeric type");
    assert_eq!(numeric.typmods.len(), 2);
    assert_eq!(numeric.array_bounds.len(), 1);
    let timestamp = casts[1].type_name.as_deref().expect("timestamp type");
    assert_eq!(timestamp.typmods.len(), 1);
    assert!(timestamp.names.iter().any(
        |name| matches!(name, Node::String(value) if value.sval.as_deref() == Some("timestamptz"))
    ));
}

#[test]
fn select_builds_xml_document_and_json_is_predicates() {
    let sql = "select xmlcol is document, xmlcol is not document, doc is json, doc is json array, doc is json object with unique keys, doc is not json scalar";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::XmlExpr(expr) if expr.op == XmlExprOp::Document && expr.args.len() == 1
    ));
    assert!(matches!(
        values[1],
        Node::BoolExpr(expr)
            if expr.boolop == pg_parser::BoolExprType::NotExpr
                && matches!(expr.args.first(), Some(Node::XmlExpr(document)) if document.op == XmlExprOp::Document)
    ));
    assert!(matches!(
        values[2],
        Node::JsonIsPredicate(predicate)
            if predicate.item_type == JsonValueType::Any
                && predicate.format.is_some()
                && predicate.location as usize == sql.find("doc is json").expect("JSON predicate")
    ));
    assert!(matches!(
        values[3],
        Node::JsonIsPredicate(predicate) if predicate.item_type == JsonValueType::Array
    ));
    assert!(matches!(
        values[4],
        Node::JsonIsPredicate(predicate)
            if predicate.item_type == JsonValueType::Object && predicate.unique_keys
    ));
    assert!(matches!(
        values[5],
        Node::BoolExpr(expr)
            if expr.boolop == pg_parser::BoolExprType::NotExpr
                && expr.location as usize == sql.find("doc is not json").expect("negated JSON predicate")
                && matches!(expr.args.first(), Some(Node::JsonIsPredicate(predicate))
                    if predicate.item_type == JsonValueType::Scalar
                        && predicate.location == expr.location)
    ));
}

#[test]
fn select_in_subqueries_build_sublinks_and_scalar_lists_build_aexpr() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select id in (select id from other), id not in (select id from other), id in (1, 2), id not in (1, 2)",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AnySublink
                && link.testexpr.is_some()
                && link.subselect.is_some()
                && link.oper_name.is_empty()
    ));
    assert!(matches!(
        values[1],
        Node::BoolExpr(expr)
            if expr.boolop == pg_parser::BoolExprType::NotExpr
                && matches!(expr.args.first(), Some(Node::SubLink(link)) if link.sub_link_type == pg_parser::SubLinkType::AnySublink)
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(expr)
            if expr.kind == pg_parser::AExprKind::In
                && matches!(expr.name.as_slice(), [Node::String(value)] if value.sval.as_deref() == Some("="))
                && expr.rexpr_list_start >= 0
                && expr.rexpr_list_end >= expr.rexpr_list_start
    ));
    assert!(matches!(
        values[3],
        Node::AExpr(expr)
            if expr.kind == pg_parser::AExprKind::In
                && matches!(expr.name.as_slice(), [Node::String(value)] if value.sval.as_deref() == Some("<>"))
    ));
}

#[test]
fn select_scalar_exists_array_and_quantified_sublinks_preserve_raw_fields() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select (select array[10, 20])[1], exists(select 1), array(select id from items), id = any(select id from items), id <> all(select id from items), id = any(array[1, 2])",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        values[0],
        Node::AIndirection(indirection)
            if matches!(indirection.arg.as_deref(), Some(Node::SubLink(link))
                if link.sub_link_type == pg_parser::SubLinkType::ExprSublink
                    && link.testexpr.is_none()
                    && link.oper_name.is_empty()
                    && link.subselect.is_some())
                && matches!(indirection.indirection.as_slice(), [Node::AIndices(_)])
    ));
    assert!(matches!(
        values[1],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::ExistsSublink
                && link.testexpr.is_none()
                && link.oper_name.is_empty()
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[2],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::ArraySublink
                && link.testexpr.is_none()
                && link.oper_name.is_empty()
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[3],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AnySublink
                && link.testexpr.is_some()
                && link.oper_name.len() == 1
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[4],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AllSublink
                && link.testexpr.is_some()
                && link.oper_name.len() == 1
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[5],
        Node::AExpr(expr)
            if expr.kind == pg_parser::AExprKind::OpAny
                && expr.lexpr.is_some()
                && matches!(expr.rexpr.as_deref(), Some(Node::AArrayExpr(_)))
    ));
}

#[test]
fn select_builds_at_time_zone_and_explicit_operator_precedence() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select ts at time zone 'UTC', ts at local, 2 ^ 3 * 4, doc -> 'key', flags | mask",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax
                && call.args.len() == 2
    ));
    assert!(matches!(
        values[1],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax
                && call.args.len() == 1
                && call.location == -1
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(product)
            if matches!(product.lexpr.as_deref(), Some(Node::AExpr(power))
                if matches!(power.name.as_slice(), [Node::String(value)] if value.sval.as_deref() == Some("^")))
    ));
    assert!(matches!(
        values[3],
        Node::AExpr(expression)
            if matches!(expression.name.as_slice(), [Node::String(name)]
                if name.sval.as_deref() == Some("->"))
    ));
    assert!(matches!(
        values[4],
        Node::AExpr(expression)
            if matches!(expression.name.as_slice(), [Node::String(name)]
                if name.sval.as_deref() == Some("|"))
    ));
}

#[test]
fn select_qualified_prefix_and_quantified_operators_follow_postgresql_precedence() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select 1 + 2 ## 3,
                1 operator(pg_catalog.+) 2 * 3,
                @-@ value,
                -ts at time zone 'UTC',
                -value::int,
                name like any(array['a', 'b']),
                id operator(pg_catalog.=) any(select id from other)",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        values[0],
        Node::AExpr(custom)
            if matches!(custom.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("##"))
                && matches!(custom.lexpr.as_deref(), Some(Node::AExpr(add))
                    if matches!(add.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("+")))
    ));
    assert!(matches!(
        values[1],
        Node::AExpr(explicit)
            if explicit.name.len() == 2
                && matches!(explicit.rexpr.as_deref(), Some(Node::AExpr(product))
                    if matches!(product.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("*")))
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(prefix)
            if prefix.lexpr.is_none()
                && matches!(prefix.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("@-@"))
    ));
    assert!(matches!(
        values[3],
        Node::FuncCall(timezone)
            if matches!(timezone.args.get(1), Some(Node::AExpr(prefix)) if prefix.lexpr.is_none())
    ));
    assert!(matches!(
        values[4],
        Node::AExpr(prefix)
            if prefix.lexpr.is_none()
                && matches!(prefix.rexpr.as_deref(), Some(Node::TypeCast(_)))
    ));
    assert!(matches!(
        values[5],
        Node::AExpr(quantified)
            if quantified.kind == pg_parser::AExprKind::OpAny
                && matches!(quantified.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("~~"))
    ));
    assert!(matches!(
        values[6],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AnySublink
                && link.oper_name.len() == 2
                && matches!(link.subselect.as_deref(), Some(Node::SelectStmt(_)))
    ));

    let Node::SelectStmt(predicates) = parse_statement(
        "select a = b in (1, 2), a in (1, 2) = true, a is distinct from b = c, value not between symmetric low and high",
    ) else {
        panic!("expected predicate precedence SelectStmt");
    };
    let values = predicates
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::AExpr(comparison)
            if comparison.kind == pg_parser::AExprKind::Op
                && matches!(comparison.rexpr.as_deref(), Some(Node::AExpr(predicate))
                    if predicate.kind == pg_parser::AExprKind::In)
    ));
    assert!(matches!(
        values[1],
        Node::AExpr(comparison)
            if comparison.kind == pg_parser::AExprKind::Op
                && matches!(comparison.lexpr.as_deref(), Some(Node::AExpr(predicate))
                    if predicate.kind == pg_parser::AExprKind::In)
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(distinct)
            if distinct.kind == pg_parser::AExprKind::Distinct
                && matches!(distinct.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("="))
                && matches!(distinct.rexpr.as_deref(), Some(Node::AExpr(comparison))
                    if comparison.kind == pg_parser::AExprKind::Op)
    ));
    assert!(matches!(
        values[3],
        Node::AExpr(between)
            if between.kind == pg_parser::AExprKind::NotBetweenSym
                && matches!(between.name.as_slice(), [Node::String(name)] if name.sval.as_deref() == Some("NOT BETWEEN SYMMETRIC"))
    ));
}

#[test]
fn select_like_ilike_and_similar_preserve_raw_operators_and_escape_calls() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select name like 'a%', name not like 'a!%' escape '!', name ilike 'a%', name similar to '(a|b)%' escape '!'",
    ) else {
        panic!("expected SelectStmt");
    };
    let expressions = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::AExpr(expression)) => expression,
                other => panic!("expected AExpr, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    let operator = |expression: &pg_parser::AExpr| {
        expression.name.first().and_then(|name| match name {
            Node::String(value) => value.sval.clone(),
            _ => None,
        })
    };
    assert_eq!(operator(expressions[0]).as_deref(), Some("~~"));
    assert_eq!(operator(expressions[1]).as_deref(), Some("!~~"));
    assert!(
        matches!(expressions[1].rexpr.as_deref(), Some(Node::FuncCall(call)) if call.args.len() == 2)
    );
    assert_eq!(operator(expressions[2]).as_deref(), Some("~~*"));
    assert_eq!(operator(expressions[3]).as_deref(), Some("~"));
    assert!(
        matches!(expressions[3].rexpr.as_deref(), Some(Node::FuncCall(call)) if call.args.len() == 2)
    );
}

#[test]
fn select_is_normalized_builds_sql_syntax_function_calls() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select value is normalized, value is nfc normalized, value is not nfkd normalized",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax && call.args.len() == 1
    ));
    assert!(matches!(
        values[1],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax && call.args.len() == 2
    ));
    assert!(matches!(
        values[2],
        Node::BoolExpr(expr)
            if expr.boolop == pg_parser::BoolExprType::NotExpr
                && matches!(expr.args.first(), Some(Node::FuncCall(call)) if call.args.len() == 2)
    ));
}

#[test]
fn select_function_calls_populate_aggregate_filter_nulls_and_window_fields() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select f(a, variadic rest order by key desc nulls last) filter (where active) ignore nulls over (partition by grp order by key rows between 1 preceding and current row), percentile_cont(0.5) within group (order by score) respect nulls over win, count(*)",
    ) else {
        panic!("expected SelectStmt");
    };
    let calls = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::FuncCall(call)) => call,
                other => panic!("expected FuncCall, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();

    let ordered = calls[0];
    assert_eq!(ordered.args.len(), 2);
    assert!(ordered.func_variadic);
    assert_eq!(ordered.agg_order.len(), 1);
    assert!(ordered.agg_filter.is_some());
    assert_eq!(ordered.ignore_nulls, 1);
    let over = ordered.over.as_deref().expect("OVER clause");
    assert_eq!(over.partition_clause.len(), 1);
    assert_eq!(over.order_clause.len(), 1);
    assert_ne!(over.frame_options & FRAMEOPTION_BETWEEN, 0);

    let within = calls[1];
    assert!(within.agg_within_group);
    assert_eq!(within.agg_order.len(), 1);
    assert_eq!(within.ignore_nulls, 2);
    assert_eq!(
        within.over.as_deref().and_then(|over| over.name.as_deref()),
        Some("win")
    );

    let star = calls[2];
    assert!(star.agg_star);
    assert!(star.args.is_empty());
}

#[test]
fn select_function_order_by_using_preserves_qualified_operators_and_locations() {
    let sql = "select array_agg(value order by key using operator(pg_catalog.<) nulls first), percentile_cont(0.5) within group (order by score using ->)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let calls = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::FuncCall(call)) => call,
                other => panic!("expected FuncCall, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    let Node::SortBy(qualified) = &calls[0].agg_order[0] else {
        panic!("expected qualified SortBy");
    };
    assert_eq!(qualified.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(qualified.location as usize, sql.find("operator").unwrap());
    assert!(matches!(
        qualified.use_op.as_slice(),
        [Node::String(schema), Node::String(operator)]
            if schema.sval.as_deref() == Some("pg_catalog")
                && operator.sval.as_deref() == Some("<")
    ));

    let Node::SortBy(arrow) = &calls[1].agg_order[0] else {
        panic!("expected arrow SortBy");
    };
    assert_eq!(arrow.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(arrow.location as usize, sql.rfind("->").unwrap());
    assert!(matches!(
        arrow.use_op.as_slice(),
        [Node::String(operator)] if operator.sval.as_deref() == Some("->")
    ));
}

#[test]
fn select_json_aggregates_populate_filter_order_and_window_fields() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select json_objectagg(k value v) filter (where active) over (partition by grp), json_arrayagg(v order by key) filter (where active) over win",
    ) else {
        panic!("expected SelectStmt");
    };
    let Node::ResTarget(object_target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonObjectAgg(object)) = object_target.val.as_deref() else {
        panic!("expected JsonObjectAgg");
    };
    let object_constructor = object.constructor.as_deref().expect("constructor");
    assert!(object_constructor.agg_filter.is_some());
    assert_eq!(
        object_constructor
            .over
            .as_deref()
            .map(|over| over.partition_clause.len()),
        Some(1)
    );

    let Node::ResTarget(array_target) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonArrayAgg(array)) = array_target.val.as_deref() else {
        panic!("expected JsonArrayAgg");
    };
    let array_constructor = array.constructor.as_deref().expect("constructor");
    assert_eq!(array_constructor.agg_order.len(), 1);
    assert!(array_constructor.agg_filter.is_some());
    assert_eq!(
        array_constructor
            .over
            .as_deref()
            .and_then(|over| over.name.as_deref()),
        Some("win")
    );
}

#[test]
fn select_json_arrayagg_preserves_complete_sort_by_nodes() {
    let sql = "select json_arrayagg(value order by name desc nulls last, id using operator(pg_catalog.<) absent on null returning jsonb)";
    let Node::SelectStmt(stmt) = parse_statement(sql) else {
        panic!("expected SelectStmt");
    };
    let [Node::ResTarget(target)] = stmt.target_list.as_slice() else {
        panic!("expected ResTarget");
    };
    let Some(Node::JsonArrayAgg(aggregate)) = target.val.as_deref() else {
        panic!("expected JsonArrayAgg");
    };
    assert!(aggregate.absent_on_null);
    let constructor = aggregate.constructor.as_deref().expect("constructor");
    assert_eq!(constructor.agg_order.len(), 2);
    let Node::SortBy(descending) = &constructor.agg_order[0] else {
        panic!("expected SortBy");
    };
    assert_eq!(descending.sortby_dir, pg_parser::SortByDir::Desc);
    assert_eq!(descending.sortby_nulls, pg_parser::SortByNulls::Last);
    assert_eq!(descending.location, -1);
    let Node::SortBy(using) = &constructor.agg_order[1] else {
        panic!("expected SortBy");
    };
    assert_eq!(using.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(using.location as usize, sql.find("operator").unwrap());
    assert!(matches!(
        using.use_op.as_slice(),
        [Node::String(schema), Node::String(operator)]
            if schema.sval.as_deref() == Some("pg_catalog")
                && operator.sval.as_deref() == Some("<")
    ));
}

#[test]
fn select_quantified_comparisons_distinguish_subqueries_and_array_expressions() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select id = any(select id from other), id <> all(select id from other), id = any(array[1, 2]), id > some(array[1, 2])",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AnySublink
                && link.testexpr.is_some()
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[1],
        Node::SubLink(link)
            if link.sub_link_type == pg_parser::SubLinkType::AllSublink
                && link.testexpr.is_some()
                && link.subselect.is_some()
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(expression) if expression.kind == pg_parser::AExprKind::OpAny
    ));
    assert!(matches!(
        values[3],
        Node::AExpr(expression) if expression.kind == pg_parser::AExprKind::OpAny
    ));
}

#[test]
fn select_overlaps_builds_sql_syntax_function_call() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select (start_at, end_at) overlaps (other_start, other_end), row(start_at, end_at) overlaps row(other_start, other_end)",
    ) else {
        panic!("expected SelectStmt");
    };
    for target in &stmt.target_list {
        let Node::ResTarget(target) = target else {
            panic!("expected ResTarget");
        };
        let Some(Node::FuncCall(call)) = target.val.as_deref() else {
            panic!("expected FuncCall");
        };
        assert_eq!(call.args.len(), 4);
        assert_eq!(call.funcformat, pg_parser::CoercionForm::SqlSyntax);
    }
}

#[test]
fn select_array_expressions_preserve_nested_shape_and_list_locations() {
    let Node::SelectStmt(stmt) =
        parse_statement("select array[], array[1, 2], array[[1, 2], [3, 4]]")
    else {
        panic!("expected SelectStmt");
    };
    let arrays = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::AArrayExpr(array)) => array,
                other => panic!("expected AArrayExpr, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(arrays[0].elements.is_empty());
    assert_eq!(arrays[1].elements.len(), 2);
    assert!(arrays[1].list_start >= arrays[1].location);
    assert!(arrays[1].list_end >= arrays[1].list_start);
    assert_eq!(arrays[2].elements.len(), 2);
    assert!(
        arrays[2]
            .elements
            .iter()
            .all(|element| matches!(element, Node::AArrayExpr(inner) if inner.elements.len() == 2))
    );
}

#[test]
fn select_special_sql_functions_build_cast_extract_normalize_and_user_nodes() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select cast(value as numeric(10, 2)[]), treat(value as text), extract(year from ts), normalize(value), normalize(value, nfc), user, system_user, collation for (value)",
    ) else {
        panic!("expected SelectStmt");
    };
    let values = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::TypeCast(cast)
            if cast.type_name.as_ref().is_some_and(|name| name.typmods.len() == 2 && name.array_bounds.len() == 1)
    ));
    assert!(matches!(values[1], Node::FuncCall(call) if call.args.len() == 1));
    assert!(matches!(
        values[2],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax && call.args.len() == 2
    ));
    assert!(matches!(values[3], Node::FuncCall(call) if call.args.len() == 1));
    assert!(matches!(values[4], Node::FuncCall(call) if call.args.len() == 2));
    assert!(matches!(
        values[5],
        Node::SqlValueFunction(function) if function.op == pg_parser::SqlValueFunctionOp::User
    ));
    assert!(matches!(
        values[6],
        Node::FuncCall(call) if call.funcformat == pg_parser::CoercionForm::SqlSyntax
    ));
    assert!(matches!(
        values[7],
        Node::FuncCall(call)
            if call.funcformat == pg_parser::CoercionForm::SqlSyntax && call.args.len() == 1
    ));

    let Node::SelectStmt(extension) = parse_statement("select extract('epoch' from ts)") else {
        panic!("expected EXTRACT extension SelectStmt");
    };
    assert!(matches!(
        extension.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::FuncCall(call)) if call.args.len() == 2)
    ));
}

#[test]
fn select_position_overlay_and_substring_preserve_sql_syntax_argument_rewrites() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select position('a' in text_value), overlay(text_value placing 'x' from 2 for 1), overlay(text_value, 'x', 2), substring(text_value from 2 for 3), substring(text_value for 3), substring(text_value similar pattern escape esc), substring(text_value, 2, 3)",
    ) else {
        panic!("expected SelectStmt");
    };
    let calls = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::FuncCall(call)) => call,
                other => panic!("expected FuncCall, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(calls[0].args.len(), 2);
    assert!(matches!(calls[0].args[0], Node::ColumnRef(_)));
    assert!(matches!(calls[0].args[1], Node::AConst(_)));
    assert_eq!(calls[1].args.len(), 4);
    assert_eq!(calls[1].funcformat, pg_parser::CoercionForm::SqlSyntax);
    assert_eq!(calls[2].funcformat, pg_parser::CoercionForm::ExplicitCall);
    assert_eq!(calls[3].args.len(), 3);
    assert_eq!(calls[4].args.len(), 3);
    assert!(matches!(calls[4].args[2], Node::TypeCast(_)));
    assert_eq!(calls[5].args.len(), 3);
    assert_eq!(calls[6].funcformat, pg_parser::CoercionForm::ExplicitCall);

    let Node::SelectStmt(restricted) = parse_statement(
        "select position(a = b in c = d), position(a is distinct from b in c is document), x between y = z and q, position((a and b) in c)",
    ) else {
        panic!("expected restricted-expression SelectStmt");
    };
    let values = restricted
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => target.val.as_deref().expect("target value"),
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Node::FuncCall(call)
            if call.args.len() == 2
                && call.args.iter().all(|arg| matches!(arg, Node::AExpr(_)))
    ));
    assert!(matches!(
        values[1],
        Node::FuncCall(call)
            if matches!(call.args.as_slice(), [Node::XmlExpr(_), Node::AExpr(distinct)]
                if distinct.kind == pg_parser::AExprKind::Distinct)
    ));
    assert!(matches!(
        values[2],
        Node::AExpr(between)
            if between.kind == pg_parser::AExprKind::Between
                && matches!(between.rexpr.as_deref(), Some(Node::AArrayExpr(bounds))
                    if matches!(bounds.elements.first(), Some(Node::AExpr(comparison))
                        if comparison.kind == pg_parser::AExprKind::Op))
    ));
    assert!(matches!(
        values[3],
        Node::FuncCall(call)
            if matches!(call.args.get(1), Some(Node::BoolExpr(_)))
    ));
}

#[test]
fn select_overlay_and_substring_plain_calls_preserve_named_arguments() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select overlay(source => text_value, replacement := 'x', position_arg => 2), substring(source => text_value, start_arg := 2, count_arg => 3)",
    ) else {
        panic!("expected SelectStmt");
    };
    for target in &stmt.target_list {
        let Node::ResTarget(target) = target else {
            panic!("expected ResTarget");
        };
        let Some(Node::FuncCall(call)) = target.val.as_deref() else {
            panic!("expected FuncCall");
        };
        assert_eq!(call.funcformat, pg_parser::CoercionForm::ExplicitCall);
        assert_eq!(call.args.len(), 3);
        assert!(
            call.args
                .iter()
                .all(|argument| matches!(argument, Node::NamedArgExpr(_)))
        );
    }
}

#[test]
fn select_trim_and_xmlexists_preserve_sql_syntax_rewrites() {
    let Node::SelectStmt(stmt) = parse_statement(
        "select trim(both 'x' from value), trim(leading from value), trim(trailing 'x' from value), trim(value), xmlexists('/a' passing doc), xmlexists('/a' passing by ref doc by value), xmlexists(('/' || 'a') passing (doc_a || doc_b))",
    ) else {
        panic!("expected SelectStmt");
    };
    let calls = stmt
        .target_list
        .iter()
        .map(|target| match target {
            Node::ResTarget(target) => match target.val.as_deref() {
                Some(Node::FuncCall(call)) => call,
                other => panic!("expected FuncCall, got {other:?}"),
            },
            other => panic!("expected ResTarget, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(calls[0].args.len(), 2);
    assert!(matches!(calls[0].args[0], Node::ColumnRef(_)));
    assert!(matches!(calls[0].args[1], Node::AConst(_)));
    assert_eq!(calls[1].args.len(), 1);
    assert_eq!(calls[2].args.len(), 2);
    assert_eq!(calls[3].args.len(), 1);
    assert_eq!(calls[4].args.len(), 2);
    assert_eq!(calls[5].args.len(), 2);
    assert!(
        calls
            .iter()
            .all(|call| call.funcformat == pg_parser::CoercionForm::SqlSyntax)
    );
}
