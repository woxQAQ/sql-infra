use super::*;

#[test]
fn select_stmt_builds_graph_table_pattern_and_elements() {
    let sql = "select * from graph_table(social match (person is person_label)-[edge is knows]->(friend is person_label) where person.active = true columns (person.id as person_id, friend.id as friend_id)) as graph_rows";
    let stmt = parse_node!(sql, SelectStmt);
    let table = expect_node!(&stmt.from_clause[0], RangeGraphTable);
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
    let path = expect_node!(&pattern.path_pattern_list[0], AArrayExpr);
    assert_eq!(path.elements.len(), 3);
    assert!(
        path.elements
            .iter()
            .all(|element| matches!(element, Node::GraphElementPattern(_)))
    );
    let vertex = expect_node!(&path.elements[0], GraphElementPattern);
    assert_eq!(vertex.location as usize, sql.find("(person").unwrap());
    let edge = expect_node!(&path.elements[1], GraphElementPattern);
    assert_eq!(edge.location as usize, sql.find("-[edge").unwrap());

    let nested_stmt = parse_node!(
        "select * from graph_table(social match ((person is person_label | employee)-[edge]->(friend)){1,2} columns (person.id as person_id))",
        SelectStmt
    );
    let nested_table = expect_node!(&nested_stmt.from_clause[0], RangeGraphTable);
    let nested_pattern = nested_table
        .graph_pattern
        .as_ref()
        .expect("nested GraphPattern");
    let nested_path = expect_node!(&nested_pattern.path_pattern_list[0], AArrayExpr);
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
    let abbreviated = parse_node!(abbreviated_sql, SelectStmt);
    let table = expect_node!(&abbreviated.from_clause[0], RangeGraphTable);
    let pattern = table.graph_pattern.as_ref().expect("GraphPattern");
    let expected = [
        (GraphElementPatternKind::EdgePatternRight, 2, 2, "->{2}"),
        (GraphElementPatternKind::EdgePatternLeft, 0, 3, "<-{,3}"),
        (GraphElementPatternKind::EdgePatternAny, 4, 4, "-{4}"),
    ];
    for (path, (kind, lower, upper, needle)) in pattern.path_pattern_list.iter().zip(expected) {
        let path = expect_node!(path, AArrayExpr);
        let edge = expect_node!(&path.elements[1], GraphElementPattern);
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
