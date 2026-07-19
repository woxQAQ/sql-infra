use super::*;

#[test]
fn select_cast_collation_and_operator_locations_follow_grammar_tokens() {
    let sql = "select value::setof app.kind collate app.c, cast(value as numeric(4, 2)), a + b";
    let stmt = parse_node!(sql, SelectStmt);
    let target_value = |index: usize| {
        let target = expect_node!(&stmt.target_list[index], ResTarget);
        target.val.as_deref().expect("target value")
    };

    let collation = expect_node!(target_value(0), CollateClause);
    assert_eq!(collation.location as usize, sql.find("collate").unwrap());
    let postfix = expect_node!(collation.arg.as_deref(), Some(TypeCast));
    assert_eq!(postfix.location as usize, sql.find("::").unwrap());
    let postfix_type = postfix.type_name.as_deref().expect("postfix TypeName");
    assert!(postfix_type.setof);
    assert_eq!(
        postfix_type.location as usize,
        sql.find("app.kind").unwrap()
    );

    let cast = expect_node!(target_value(1), TypeCast);
    assert_eq!(cast.location as usize, sql.find("cast").unwrap());
    assert_eq!(
        cast.type_name.as_deref().expect("CAST TypeName").location as usize,
        sql.find("numeric").unwrap()
    );

    let operator = expect_node!(target_value(2), AExpr);
    assert_eq!(operator.location as usize, sql.rfind('+').unwrap());
}

#[test]
fn select_stmt_builds_raw_case_null_boolean_and_default_expression_nodes() {
    let stmt = parse_node!(
        "select case when a is null then default else 0 end, flag is not true",
        SelectStmt
    );
    let case_target = expect_node!(&stmt.target_list[0], ResTarget);
    let case = expect_node!(case_target.val.as_deref(), Some(CaseExpr));
    let when = expect_node!(&case.args[0], CaseWhen);
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

    let without_else = parse_node!("select case value when 1 then 'one' end", SelectStmt);
    assert!(matches!(
        without_else.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::CaseExpr(case)) if case.arg.is_some() && case.defresult.is_none())
    ));

    let boolean_target = expect_node!(&stmt.target_list[1], ResTarget);
    assert!(matches!(
        boolean_target.val.as_deref(),
        Some(Node::BooleanTest(test)) if test.booltesttype == BoolTestType::NotTrue
    ));
}

#[test]
fn select_stmt_builds_named_args_grouping_and_sql_value_functions() {
    let sql = "select f(first => 1, second := 2), grouping(category), current_timestamp(3)";
    let stmt = parse_node!(sql, SelectStmt);
    let function_target = expect_node!(&stmt.target_list[0], ResTarget);
    let function = expect_node!(function_target.val.as_deref(), Some(FuncCall));
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

    let grouping_target = expect_node!(&stmt.target_list[1], ResTarget);
    assert!(matches!(
        grouping_target.val.as_deref(),
        Some(Node::GroupingFunc(_))
    ));

    let timestamp_target = expect_node!(&stmt.target_list[2], ResTarget);
    assert!(matches!(
        timestamp_target.val.as_deref(),
        Some(Node::SqlValueFunction(value))
            if value.op == pg_parser::SqlValueFunctionOp::CurrentTimestampN
                && value.typmod == 3
    ));
}

#[test]
fn select_stmt_populates_window_frame_bounds_and_exclusion() {
    let sql = "select v from measurements window w as (partition by sensor order by measured_at rows between 2 preceding and current row exclude ties)";
    let stmt = parse_node!(sql, SelectStmt);
    assert_eq!(stmt.window_clause.len(), 1);
    let window = expect_node!(&stmt.window_clause[0], WindowDef);
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

    let quoted = parse_node!(
        "select count(*) over \"select\" window \"select\" as ()",
        SelectStmt
    );
    let target = expect_node!(&quoted.target_list[0], ResTarget);
    assert!(matches!(
        target.val.as_deref(),
        Some(Node::FuncCall(call))
            if call.over.as_deref().and_then(|window| window.name.as_deref()) == Some("select")
    ));
    let call = expect_node!(target.val.as_deref(), Some(FuncCall));
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

    let inherited = parse_node!(
        "select sum(v) over (derived rows unbounded preceding)
         from measurements
         window base as (partition by sensor), derived as (base order by measured_at)",
        SelectStmt
    );
    assert!(matches!(
        inherited.window_clause.as_slice(),
        [Node::WindowDef(base), Node::WindowDef(derived)]
            if base.name.as_deref() == Some("base")
                && base.refname.is_none()
                && derived.name.as_deref() == Some("derived")
                && derived.refname.as_deref() == Some("base")
    ));
    let inherited_target = expect_node!(&inherited.target_list[0], ResTarget);
    assert!(matches!(
        inherited_target.val.as_deref(),
        Some(Node::FuncCall(call))
            if call.over.as_deref().and_then(|window| window.refname.as_deref()) == Some("derived")
    ));
}

#[test]
fn select_nullif_preserves_postgresql_raw_aexpr_fields() {
    let stmt = parse_node!("select nullif(left_value, right_value)", SelectStmt);
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
    let stmt = parse_node!(
        "select items[1], items[2:4], items[:], items[1:2][3].field, (row(a, b)).field, $1[1]",
        SelectStmt
    );
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

    let single = expect_node!(values[0], AIndirection);
    assert!(matches!(single.arg.as_deref(), Some(Node::ColumnRef(_))));
    assert!(matches!(
        single.indirection.as_slice(),
        [Node::AIndices(index)] if !index.is_slice && index.lidx.is_none() && index.uidx.is_some()
    ));

    let slice = expect_node!(values[1], AIndirection);
    assert!(matches!(
        slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice && index.lidx.is_some() && index.uidx.is_some()
    ));

    let open_slice = expect_node!(values[2], AIndirection);
    assert!(matches!(
        open_slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice && index.lidx.is_none() && index.uidx.is_none()
    ));

    let chained = expect_node!(values[3], AIndirection);
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
    let stmt = parse_node!("select 1, 1.5, 'text', B'101', X'ff'", SelectStmt);
    let values = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            &expect_node!(target.val.as_deref(), Some(AConst)).val
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

    let signed = parse_node!("select -42, -1.5, +42", SelectStmt);
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
    let stmt = parse_node!(
        "select date '2026-07-10', numeric(5, 2) '12.34', interval '2' day,
                pg_catalog.text 'hello', char 'abc', bit '101', char(3) 'abc', bit(3) '101'",
        SelectStmt
    );
    assert_eq!(stmt.target_list.len(), 8);
    let mut type_names = Vec::new();
    for target in &stmt.target_list {
        let target = expect_node!(target, ResTarget);
        let cast = expect_node!(target.val.as_deref(), Some(TypeCast));
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
    let stmt = parse_node!(
        "select amount::numeric(10, 2)[], created_at::timestamp(3) with time zone",
        SelectStmt
    );
    let casts = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(TypeCast))
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
fn select_in_subqueries_build_sublinks_and_scalar_lists_build_aexpr() {
    let stmt = parse_node!(
        "select id in (select id from other), id not in (select id from other), id in (1, 2), id not in (1, 2)",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select (select array[10, 20])[1], exists(select 1), array(select id from items), id = any(select id from items), id <> all(select id from items), id = any(array[1, 2])",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select ts at time zone 'UTC', ts at local, 2 ^ 3 * 4, doc -> 'key', flags | mask",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select 1 + 2 ## 3,
                1 operator(pg_catalog.+) 2 * 3,
                @-@ value,
                -ts at time zone 'UTC',
                -value::int,
                name like any(array['a', 'b']),
                id operator(pg_catalog.=) any(select id from other)",
        SelectStmt
    );
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

    let predicates = parse_node!(
        "select a = b in (1, 2), a in (1, 2) = true, a is distinct from b = c, value not between symmetric low and high",
        SelectStmt
    );
    let values = predicates
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
    let stmt = parse_node!(
        "select name like 'a%', name not like 'a!%' escape '!', name ilike 'a%', name similar to '(a|b)%' escape '!'",
        SelectStmt
    );
    let expressions = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(AExpr))
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
    let stmt = parse_node!(
        "select value is normalized, value is nfc normalized, value is not nfkd normalized",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select f(a, variadic rest order by key desc nulls last) filter (where active) ignore nulls over (partition by grp order by key rows between 1 preceding and current row), percentile_cont(0.5) within group (order by score) respect nulls over win, count(*)",
        SelectStmt
    );
    let calls = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(FuncCall))
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
fn select_quantified_comparisons_distinguish_subqueries_and_array_expressions() {
    let stmt = parse_node!(
        "select id = any(select id from other), id <> all(select id from other), id = any(array[1, 2]), id > some(array[1, 2])",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select (start_at, end_at) overlaps (other_start, other_end), row(start_at, end_at) overlaps row(other_start, other_end)",
        SelectStmt
    );
    for target in &stmt.target_list {
        let target = expect_node!(target, ResTarget);
        let call = expect_node!(target.val.as_deref(), Some(FuncCall));
        assert_eq!(call.args.len(), 4);
        assert_eq!(call.funcformat, pg_parser::CoercionForm::SqlSyntax);
    }
}

#[test]
fn select_array_expressions_preserve_nested_shape_and_list_locations() {
    let stmt = parse_node!(
        "select array[], array[1, 2], array[[1, 2], [3, 4]]",
        SelectStmt
    );
    let arrays = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(AArrayExpr))
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
    let stmt = parse_node!(
        "select cast(value as numeric(10, 2)[]), treat(value as text), extract(year from ts), normalize(value), normalize(value, nfc), user, system_user, collation for (value)",
        SelectStmt
    );
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

    let extension = parse_node!("select extract('epoch' from ts)", SelectStmt);
    assert!(matches!(
        extension.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::FuncCall(call)) if call.args.len() == 2)
    ));
}

#[test]
fn select_position_overlay_and_substring_preserve_sql_syntax_argument_rewrites() {
    let stmt = parse_node!(
        "select position('a' in text_value), overlay(text_value placing 'x' from 2 for 1), overlay(text_value, 'x', 2), substring(text_value from 2 for 3), substring(text_value for 3), substring(text_value similar pattern escape esc), substring(text_value, 2, 3)",
        SelectStmt
    );
    let calls = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(FuncCall))
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

    let restricted = parse_node!(
        "select position(a = b in c = d), position(a is distinct from b in c is document), x between y = z and q, position((a and b) in c)",
        SelectStmt
    );
    let values = restricted
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
    let stmt = parse_node!(
        "select overlay(source => text_value, replacement := 'x', position_arg => 2), substring(source => text_value, start_arg := 2, count_arg => 3)",
        SelectStmt
    );
    for target in &stmt.target_list {
        let target = expect_node!(target, ResTarget);
        let call = expect_node!(target.val.as_deref(), Some(FuncCall));
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
fn select_function_order_by_using_preserves_qualified_operators_and_locations() {
    let sql = "select array_agg(value order by key using operator(pg_catalog.<) nulls first), percentile_cont(0.5) within group (order by score using ->)";
    let stmt = parse_node!(sql, SelectStmt);
    let calls = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            expect_node!(target.val.as_deref(), Some(FuncCall))
        })
        .collect::<Vec<_>>();
    let qualified = expect_node!(&calls[0].agg_order[0], SortBy);
    assert_eq!(qualified.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(qualified.location as usize, sql.find("operator").unwrap());
    assert!(matches!(
        qualified.use_op.as_slice(),
        [Node::String(schema), Node::String(operator)]
            if schema.sval.as_deref() == Some("pg_catalog")
                && operator.sval.as_deref() == Some("<")
    ));

    let arrow = expect_node!(&calls[1].agg_order[0], SortBy);
    assert_eq!(arrow.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(arrow.location as usize, sql.rfind("->").unwrap());
    assert!(matches!(
        arrow.use_op.as_slice(),
        [Node::String(operator)] if operator.sval.as_deref() == Some("->")
    ));
}
