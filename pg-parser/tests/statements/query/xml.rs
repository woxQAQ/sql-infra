use super::*;

#[test]
fn select_stmt_builds_xml_expression_and_serialize_nodes() {
    let sql = "select xmlelement(name item, xmlattributes(id as item_id), name), xmlforest(id as item_id, name), xmlserialize(content xmlparse(content '<a/>' preserve whitespace) as text indent)";
    let stmt = parse_node!(sql, SelectStmt);
    let element_target = expect_node!(&stmt.target_list[0], ResTarget);
    let element = expect_node!(element_target.val.as_deref(), Some(XmlExpr));
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

    let forest_target = expect_node!(&stmt.target_list[1], ResTarget);
    let forest = expect_node!(forest_target.val.as_deref(), Some(XmlExpr));
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

    let serialize_target = expect_node!(&stmt.target_list[2], ResTarget);
    let serialize = expect_node!(serialize_target.val.as_deref(), Some(XmlSerialize));
    assert!(serialize.indent);
    assert!(matches!(serialize.expr.as_deref(), Some(Node::XmlExpr(_))));
    assert!(serialize.type_name.is_some());
    assert_eq!(
        serialize.location as usize,
        sql.find("xmlserialize").unwrap()
    );

    let reserved_label = parse_node!(
        "select xmlelement(name select), xmlforest(id as from)",
        SelectStmt
    );
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
    let stmt = parse_node!(
        "select xmlroot(doc, version '1.0'), xmlroot(doc, version '1.0', standalone yes), xmlroot(doc, version '1.0', standalone no), xmlroot(doc, version '1.0', standalone no value)",
        SelectStmt
    );
    let standalone_values = stmt
        .target_list
        .iter()
        .map(|target| {
            let target = expect_node!(target, ResTarget);
            let expression = expect_node!(target.val.as_deref(), Some(XmlExpr));
            assert_eq!(expression.op, XmlExprOp::Xmlroot);
            assert_eq!(expression.args.len(), 3);
            let value = expect_node!(&expression.args[2], AConst);
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
    let stmt = parse_node!(sql, SelectStmt);
    let table = expect_node!(&stmt.from_clause[0], RangeTableFunc);
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

    let ordinality = expect_node!(&table.columns[0], RangeTableFuncCol);
    assert!(ordinality.for_ordinality);
    assert_eq!(ordinality.colname.as_deref(), Some("ord"));
    assert_eq!(
        ordinality.location,
        sql.find("ord for ordinality").unwrap() as i32
    );

    let id = expect_node!(&table.columns[1], RangeTableFuncCol);
    assert!(id.type_name.is_some());
    assert!(id.colexpr.is_some());
    assert!(id.is_not_null);
    assert_eq!(id.location as usize, sql.find("id int path").unwrap());

    let name = expect_node!(&table.columns[2], RangeTableFuncCol);
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
        let stmt = parse_node!(&sql, SelectStmt);
        assert!(matches!(
            stmt.from_clause.as_slice(),
            [Node::RangeTableFunc(table)]
                if table.docexpr.is_some() && table.columns.len() == 1
        ));
    }

    let sql = "select * from xmltable(xmlnamespaces(1 is distinct from 2 as cmp, default (true and false)), '/x' passing doc columns compared boolean path 1 = 1, grouped boolean default (true and false), nullable text path null)";
    let stmt = parse_node!(sql, SelectStmt);
    let table = expect_node!(&stmt.from_clause[0], RangeTableFunc);
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

    let parenthesized = parse_node!(
        "select * from xmltable(('/x' || '/item') passing (doc_a || doc_b) columns id int)",
        SelectStmt
    );
    let table = expect_node!(&parenthesized.from_clause[0], RangeTableFunc);
    assert!(matches!(table.rowexpr.as_deref(), Some(Node::AExpr(_))));
    assert!(matches!(table.docexpr.as_deref(), Some(Node::AExpr(_))));

    let sql = "select * from lateral xmltable('/x' passing doc columns id int) as xt";
    let lateral = parse_node!(sql, SelectStmt);
    let [Node::RangeTableFunc(table)] = lateral.from_clause.as_slice() else {
        panic!("expected lateral RangeTableFunc");
    };
    assert!(table.lateral);
    assert_eq!(table.location, sql.find("xmltable").unwrap() as i32);
}

#[test]
fn select_builds_xml_document_and_json_is_predicates() {
    let sql = "select xmlcol is document, xmlcol is not document, doc is json, doc is json array, doc is json object with unique keys, doc is not json scalar";
    let stmt = parse_node!(sql, SelectStmt);
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
fn select_trim_and_xmlexists_preserve_sql_syntax_rewrites() {
    let stmt = parse_node!(
        "select trim(both 'x' from value), trim(leading from value), trim(trailing 'x' from value), trim(value), xmlexists('/a' passing doc), xmlexists('/a' passing by ref doc by value), xmlexists(('/' || 'a') passing (doc_a || doc_b))",
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
