use super::*;

#[test]
fn select_stmt_builds_range_table_sample() {
    let sql = "select * from items tablesample system(10) repeatable(42)";
    let stmt = parse_node!(sql, SelectStmt);
    let sample = expect_node!(&stmt.from_clause[0], RangeTableSample);
    assert!(sample.relation.is_some());
    assert_eq!(sample.method.len(), 1);
    assert_eq!(sample.args.len(), 1);
    assert!(sample.repeatable.is_some());
    assert_eq!(sample.location as usize, sql.find("system").unwrap());
}

#[test]
fn select_stmt_builds_rows_from_function_pairs_and_column_definitions() {
    let stmt = parse_node!(
        "select * from rows from (f(1) as (a int, b text collate c), g(2)) with ordinality as rf",
        SelectStmt
    );
    let range = expect_node!(&stmt.from_clause[0], RangeFunction);
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
    let first_pair = expect_node!(&range.functions[0], AArrayExpr);
    assert!(matches!(first_pair.elements[0], Node::FuncCall(_)));
    let coldefs = expect_node!(&first_pair.elements[1], AArrayExpr);
    assert_eq!(coldefs.elements.len(), 2);
    assert!(matches!(
        coldefs.elements[1],
        Node::ColumnDef(ref column) if column.coll_clause.is_some()
    ));
    let second_pair = expect_node!(&range.functions[1], AArrayExpr);
    assert!(matches!(
        second_pair.elements[1],
        Node::AArrayExpr(ref coldefs) if coldefs.elements.is_empty()
    ));
}

#[test]
fn select_range_function_preserves_function_pairs_alias_columns_and_coldefs() {
    let stmt = parse_node!(
        "select * from f() as named(a, b), g() as typed(x int), h() as (y text), q(distinct a order by b) as agg",
        SelectStmt
    );
    assert_eq!(stmt.from_clause.len(), 4);
    let ranges = stmt
        .from_clause
        .iter()
        .map(|item| expect_node!(item, RangeFunction))
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
    let aggregate_pair = expect_node!(&ranges[3].functions[0], AArrayExpr);
    assert!(matches!(
        aggregate_pair.elements[0],
        Node::FuncCall(ref call) if call.agg_distinct && call.agg_order.len() == 1
    ));

    let quoted = parse_node!("select * from f() as \"select\"(\"from\")", SelectStmt);
    let [Node::RangeFunction(range)] = quoted.from_clause.as_slice() else {
        panic!("expected RangeFunction");
    };
    let alias = range.alias.as_deref().expect("Alias");
    assert_eq!(alias.aliasname.as_deref(), Some("select"));
    assert!(matches!(
        alias.colnames.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("from")
    ));

    let lateral = parse_node!("select * from lateral f() as lf", SelectStmt);
    let [Node::RangeFunction(range)] = lateral.from_clause.as_slice() else {
        panic!("expected lateral RangeFunction");
    };
    assert!(range.lateral);

    let keyword_alias = parse_node!("select abort.value from f() abort", SelectStmt);
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

    let keyword_aliases = parse_node!(
        "select value.a from items value(a), other abort",
        SelectStmt
    );
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
fn select_stmt_builds_strict_join_and_locking_nodes() {
    let stmt = parse_node!(
        "select a.id from app.a a left join app.b b using (id) as matched natural join app.c c for no key update of app.a, app.b skip locked",
        SelectStmt
    );
    assert_eq!(stmt.from_clause.len(), 1);
    let outer = expect_node!(&stmt.from_clause[0], JoinExpr);
    assert!(outer.is_natural);
    assert!(matches!(outer.rarg.as_deref(), Some(Node::RangeVar(_))));
    let inner = expect_node!(outer.larg.as_deref(), Some(JoinExpr));
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
    let lock = expect_node!(&stmt.locking_clause[0], LockingClause);
    assert_eq!(lock.strength, LockClauseStrength::Fornokeyupdate);
    assert_eq!(lock.wait_policy, LockWaitPolicy::Skip);
    assert_eq!(lock.locked_rels.len(), 2);
    assert!(
        lock.locked_rels
            .iter()
            .all(|rel| matches!(rel, Node::RangeVar(_)))
    );

    let multiple = parse_node!(
        "select * from app.a, app.b for update of app.a for share of app.b",
        SelectStmt
    );
    assert_eq!(multiple.locking_clause.len(), 2);

    let on_chain = parse_node!(
        "select * from app.a a join app.b b on a.id = b.id left join app.c c on b.id = c.id",
        SelectStmt
    );
    let [Node::JoinExpr(outer)] = on_chain.from_clause.as_slice() else {
        panic!("expected outer chained JoinExpr");
    };
    assert!(outer.quals.is_some());
    assert!(matches!(outer.larg.as_deref(), Some(Node::JoinExpr(inner)) if inner.quals.is_some()));
}

#[test]
fn select_exposes_core_raw_expression_and_range_node_shapes() {
    let sql = "select *, $1, coalesce(a, 0), greatest(a, b), row(a, b), (select 1), a collate c, a::int from generate_series(1, 2) g, (select 1) s where a = 1 and b = 2 order by a desc nulls last";
    let stmt = parse_node!(sql, SelectStmt);
    assert_eq!(stmt.target_list.len(), 8);
    let values = stmt
        .target_list
        .iter()
        .map(|target| {
            expect_node!(target, ResTarget)
                .val
                .as_deref()
                .expect("target value")
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

    let nested_lateral = parse_node!(
        "select * from lateral ((select 1)) as nested(value)",
        SelectStmt
    );
    let [Node::RangeSubselect(range)] = nested_lateral.from_clause.as_slice() else {
        panic!("expected RangeSubselect");
    };
    assert!(range.lateral);
    let alias = range.alias.as_deref().expect("nested subquery alias");
    assert_eq!(alias.aliasname.as_deref(), Some("nested"));
    assert_eq!(alias.colnames.len(), 1);

    let flat = parse_node!("select 1 where a and b and c or d or e", SelectStmt);
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

    let qualified = parse_node!("select app.select", SelectStmt);
    assert!(matches!(
        qualified.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(
                target.val.as_deref(),
                Some(Node::ColumnRef(column)) if column.fields.len() == 2
            )
    ));

    let aliases = parse_node!("select 1 answer, 2 as select", SelectStmt);
    assert!(matches!(
        aliases.target_list.as_slice(),
        [Node::ResTarget(bare), Node::ResTarget(explicit)]
            if bare.name.as_deref() == Some("answer")
                && explicit.name.as_deref() == Some("select")
    ));
}

#[test]
fn select_join_expr_preserves_type_qualification_and_raw_defaults() {
    let stmt = parse_node!(
        "select * from app.a left join app.b on app.a.id = app.b.id",
        SelectStmt
    );
    let [Node::JoinExpr(join)] = stmt.from_clause.as_slice() else {
        panic!("expected JoinExpr");
    };
    assert_eq!(join.jointype, JoinType::Left);
    assert!(matches!(join.quals.as_deref(), Some(Node::AExpr(_))));
    assert!(join.using_clause.is_empty());
    assert!(!join.is_natural);
    assert_eq!(join.rtindex, 0);
}
