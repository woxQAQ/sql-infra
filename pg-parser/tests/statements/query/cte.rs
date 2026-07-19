use super::*;

#[test]
fn cte_accepts_empty_plain_selects() {
    let with = parse_node!("with x as (select) select", SelectStmt);
    assert!(with.with_clause.is_some());
    assert!(with.target_list.is_empty());
}

#[test]
fn cte_attaches_to_parenthesized_top_level_selects() {
    for sql in [
        "with x as (select 1) (select * from x)",
        "with x as (select 1) ((select * from x)) order by 1",
    ] {
        let stmt = parse_node!(sql, SelectStmt);
        assert!(stmt.with_clause.is_some(), "{sql}");
        assert!(!stmt.target_list.is_empty(), "{sql}");
    }
}

#[test]
fn select_stmt_parses_cte_aliases_and_materialization() {
    let sql = "with recursive source(id, parent_id) as not materialized (select id, parent_id from tree) select id from source";
    let stmt = parse_node!(sql, SelectStmt);
    let with: Box<WithClause> = stmt.with_clause.expect("WITH clause");
    assert!(with.recursive);
    assert_eq!(with.location, sql.find("with").unwrap() as i32);
    let cte = expect_node!(&with.ctes[0], CommonTableExpr);
    assert_eq!(cte.ctename.as_deref(), Some("source"));
    assert_eq!(cte.location, sql.find("source").unwrap() as i32);
    assert_eq!(cte.aliascolnames.len(), 2);
    assert_eq!(cte.ctematerialized, CteMaterialize::Never);
    assert!(cte.ctequery.is_some());

    let quoted = parse_node!(
        "with \"select\"(\"from\") as (select 1) select * from \"select\"",
        SelectStmt
    );
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
        let stmt = parse_node!(&sql, SelectStmt);
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
    let stmt = parse_node!(sql, SelectStmt);
    let with = stmt.with_clause.expect("WITH clause");
    let cte = expect_node!(&with.ctes[0], CommonTableExpr);
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

    let stmt = parse_node!(
        "with recursive walk(id) as (values (1)) search depth first by id set visit_order cycle id set is_cycle using visit_path select * from walk",
        SelectStmt
    );
    let with = stmt.with_clause.expect("WITH clause");
    let cte = expect_node!(&with.ctes[0], CommonTableExpr);
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
