use super::*;

#[test]
fn select_stmt_builds_sql_json_constructor_and_aggregate_nodes() {
    let stmt = parse_node!(
        "select json(doc format json encoding utf8 with unique keys), json_scalar(42), json_serialize(doc format json returning text format json), json_object('id' value id absent on null with unique keys returning jsonb), json_array(id, name null on null returning jsonb), json_array(select id from items), json_objectagg(name value id absent on null with unique keys returning jsonb), json_arrayagg(id order by name absent on null returning jsonb), merge_action() from items",
        SelectStmt
    );
    assert_eq!(stmt.target_list.len(), 9);

    let parse_target = expect_node!(&stmt.target_list[0], ResTarget);
    let parse = expect_node!(parse_target.val.as_deref(), Some(JsonParseExpr));
    assert!(parse.unique_keys);
    let value = parse.expr.as_ref().expect("JsonValueExpr");
    assert!(matches!(
        value.raw_expr.as_deref(),
        Some(Node::ColumnRef(_))
    ));
    let format = value.format.as_ref().expect("JsonFormat");
    assert_eq!(format.format_type, JsonFormatType::Json);
    assert_eq!(format.encoding, JsonEncoding::Utf8);

    let scalar_target = expect_node!(&stmt.target_list[1], ResTarget);
    assert!(matches!(
        scalar_target.val.as_deref(),
        Some(Node::JsonScalarExpr(_))
    ));

    let serialize_target = expect_node!(&stmt.target_list[2], ResTarget);
    let serialize = expect_node!(serialize_target.val.as_deref(), Some(JsonSerializeExpr));
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

    let object_target = expect_node!(&stmt.target_list[3], ResTarget);
    let object = expect_node!(object_target.val.as_deref(), Some(JsonObjectConstructor));
    assert!(object.absent_on_null);
    assert!(object.unique);
    assert!(object.output.is_some());
    assert!(matches!(
        object.exprs.first(),
        Some(Node::JsonKeyValue(pair))
            if matches!(pair.key.as_deref(), Some(Node::AConst(_)))
    ));

    let array_target = expect_node!(&stmt.target_list[4], ResTarget);
    let array = expect_node!(array_target.val.as_deref(), Some(JsonArrayConstructor));
    assert_eq!(array.exprs.len(), 2);
    assert!(!array.absent_on_null);
    assert!(
        array
            .exprs
            .iter()
            .all(|expr| matches!(expr, Node::JsonValueExpr(_)))
    );

    let query_array_target = expect_node!(&stmt.target_list[5], ResTarget);
    let query_array = expect_node!(
        query_array_target.val.as_deref(),
        Some(JsonArrayQueryConstructor)
    );
    assert!(matches!(
        query_array.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));
    assert!(matches!(
        query_array.format.as_deref(),
        Some(format)
            if format.format_type == JsonFormatType::Default && format.location == -1
    ));

    let object_agg_target = expect_node!(&stmt.target_list[6], ResTarget);
    let object_agg = expect_node!(object_agg_target.val.as_deref(), Some(JsonObjectAgg));
    assert!(object_agg.constructor.is_some());
    assert!(object_agg.arg.is_some());

    let array_agg_target = expect_node!(&stmt.target_list[7], ResTarget);
    let array_agg = expect_node!(array_agg_target.val.as_deref(), Some(JsonArrayAgg));
    assert_eq!(
        array_agg
            .constructor
            .as_ref()
            .expect("JsonAggConstructor")
            .agg_order
            .len(),
        1
    );

    let merge_action_target = expect_node!(&stmt.target_list[8], ResTarget);
    assert!(matches!(
        merge_action_target.val.as_deref(),
        Some(Node::MergeSupportFunc(function))
            if function.msftype == 25 && function.msfcollid == 0
    ));
}

#[test]
fn select_legacy_json_object_builds_a_system_function_call() {
    let stmt = parse_node!(
        "select json_object(key_array => array['id'], value_array := array[42])",
        SelectStmt
    );
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
    let stmt = parse_node!("select json_array(value returning jsonb)", SelectStmt);
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
    let stmt = parse_node!(sql, SelectStmt);
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
    let stmt = parse_node!(sql, SelectStmt);

    let query_target = expect_node!(&stmt.target_list[0], ResTarget);
    let query = expect_node!(query_target.val.as_deref(), Some(JsonFuncExpr));
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

    let exists_target = expect_node!(&stmt.target_list[1], ResTarget);
    let exists = expect_node!(exists_target.val.as_deref(), Some(JsonFuncExpr));
    assert_eq!(exists.op, JsonExprOp::ExistsOp);
    assert!(exists.output.is_none());
    assert_eq!(
        exists.on_error.as_ref().map(|behavior| behavior.btype),
        Some(JsonBehaviorType::True)
    );

    let value_target = expect_node!(&stmt.target_list[2], ResTarget);
    let value = expect_node!(value_target.val.as_deref(), Some(JsonFuncExpr));
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
    let stmt = parse_node!(sql, SelectStmt);
    let table = expect_node!(&stmt.from_clause[0], JsonTable);
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
        let column = expect_node!(column, JsonTableColumn);
        assert_eq!(column.coltype, expected_type);
    }

    let formatted = expect_node!(&table.columns[2], JsonTableColumn);
    assert_eq!(formatted.wrapper, JsonWrapper::Conditional);
    assert_eq!(formatted.quotes, JsonQuotes::Keep);
    assert!(formatted.on_empty.is_some());
    assert!(formatted.on_error.is_some());

    let nested = expect_node!(&table.columns[4], JsonTableColumn);
    assert_eq!(nested.columns.len(), 1);
}

#[test]
fn select_json_aggregates_populate_filter_order_and_window_fields() {
    let stmt = parse_node!(
        "select json_objectagg(k value v) filter (where active) over (partition by grp), json_arrayagg(v order by key) filter (where active) over win",
        SelectStmt
    );
    let object_target = expect_node!(&stmt.target_list[0], ResTarget);
    let object = expect_node!(object_target.val.as_deref(), Some(JsonObjectAgg));
    let object_constructor = object.constructor.as_deref().expect("constructor");
    assert!(object_constructor.agg_filter.is_some());
    assert_eq!(
        object_constructor
            .over
            .as_deref()
            .map(|over| over.partition_clause.len()),
        Some(1)
    );

    let array_target = expect_node!(&stmt.target_list[1], ResTarget);
    let array = expect_node!(array_target.val.as_deref(), Some(JsonArrayAgg));
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
    let stmt = parse_node!(sql, SelectStmt);
    let [Node::ResTarget(target)] = stmt.target_list.as_slice() else {
        panic!("expected ResTarget");
    };
    let aggregate = expect_node!(target.val.as_deref(), Some(JsonArrayAgg));
    assert!(aggregate.absent_on_null);
    let constructor = aggregate.constructor.as_deref().expect("constructor");
    assert_eq!(constructor.agg_order.len(), 2);
    let descending = expect_node!(&constructor.agg_order[0], SortBy);
    assert_eq!(descending.sortby_dir, pg_parser::SortByDir::Desc);
    assert_eq!(descending.sortby_nulls, pg_parser::SortByNulls::Last);
    assert_eq!(descending.location, -1);
    let using = expect_node!(&constructor.agg_order[1], SortBy);
    assert_eq!(using.sortby_dir, pg_parser::SortByDir::Using);
    assert_eq!(using.location as usize, sql.find("operator").unwrap());
    assert!(matches!(
        using.use_op.as_slice(),
        [Node::String(schema), Node::String(operator)]
            if schema.sval.as_deref() == Some("pg_catalog")
                && operator.sval.as_deref() == Some("<")
    ));
}
