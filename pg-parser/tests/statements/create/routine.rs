use super::*;

#[test]
fn create_function_stmt_populates_parameters_return_type_and_options() {
    let sql = "create or replace function app.compute(a int, in label text default 'x', out total bigint) returns int language sql immutable strict security definer parallel safe cost 10 rows 1 as 'select 1'";
    let stmt = parse_node!(sql, CreateFunctionStmt);
    assert!(stmt.replace);
    assert!(!stmt.is_procedure);
    assert_eq!(stmt.funcname.len(), 2);
    assert_eq!(stmt.parameters.len(), 3);
    assert!(stmt.return_type.is_some());
    assert_eq!(stmt.options.len(), 8);

    let first = expect_node!(&stmt.parameters[0], FunctionParameter);
    assert_eq!(first.name.as_deref(), Some("a"));
    assert_eq!(first.mode, FunctionParameterMode::Default);
    assert_eq!(first.location as usize, sql.find("a int").unwrap());

    let with_default = expect_node!(&stmt.parameters[1], FunctionParameter);
    assert_eq!(with_default.name.as_deref(), Some("label"));
    assert_eq!(with_default.mode, FunctionParameterMode::In);
    assert!(with_default.defexpr.is_some());
    assert_eq!(
        with_default.location as usize,
        sql.find("in label").unwrap()
    );

    let output = expect_node!(&stmt.parameters[2], FunctionParameter);
    assert_eq!(output.name.as_deref(), Some("total"));
    assert_eq!(output.mode, FunctionParameterMode::Out);
    assert_eq!(output.location as usize, sql.find("out total").unwrap());
}

#[test]
fn create_procedure_uses_procedure_specific_options() {
    let stmt = parse_node!(
        "create or replace procedure app.refresh(in target text, inout changed integer)
         language sql security definer set search_path to app
         begin atomic
           insert into audit_log values (target);
         end",
        CreateFunctionStmt
    );
    assert!(stmt.is_procedure);
    assert!(stmt.replace);
    assert_eq!(stmt.funcname.len(), 2);
    assert_eq!(stmt.parameters.len(), 2);
    assert!(stmt.return_type.is_none());
    assert_eq!(stmt.options.len(), 3);
    assert!(stmt.sql_body.is_some());
}

#[test]
fn create_function_stmt_can_store_a_sql_return_body() {
    let stmt = parse_node!(
        "create function increment(a int) returns int return a + 1",
        CreateFunctionStmt
    );
    assert!(matches!(
        stmt.sql_body.as_deref(),
        Some(Node::ReturnStmt(_))
    ));
}

#[test]
fn create_function_returns_table_merges_output_columns_and_builds_return_type() {
    let sql = "create function app.expand(prefix text) returns table (id bigint, label text) language sql as 'select 1, prefix'";
    let stmt = parse_node!(sql, CreateFunctionStmt);
    assert_eq!(stmt.parameters.len(), 3);
    for (index, expected_name) in [(1, "id"), (2, "label")] {
        let column = expect_node!(&stmt.parameters[index], FunctionParameter);
        assert_eq!(column.name.as_deref(), Some(expected_name));
        assert_eq!(column.mode, FunctionParameterMode::Table);
        assert!(column.arg_type.is_some());
        assert_eq!(column.location as usize, sql.find(expected_name).unwrap());
    }
    let return_type = stmt.return_type.as_deref().expect("table return TypeName");
    assert!(return_type.setof);
    assert!(matches!(
        return_type.names.as_slice(),
        [Node::String(catalog), Node::String(name)]
            if catalog.sval.as_deref() == Some("pg_catalog")
                && name.sval.as_deref() == Some("record")
    ));

    let single = parse_node!(
        "create function app.amount() returns table (value numeric(12, 2)) language sql as 'select 1'",
        CreateFunctionStmt
    );
    let return_type = single
        .return_type
        .as_deref()
        .expect("single table return type");
    assert!(return_type.setof);
    assert_eq!(return_type.typmods.len(), 2);
    assert!(matches!(
        return_type.names.last(),
        Some(Node::String(name)) if name.sval.as_deref() == Some("numeric")
    ));

    let quoted = parse_node!(
        "create function app.quoted_table() returns table (\"select\" integer) language sql as 'select 1'",
        CreateFunctionStmt
    );
    assert!(matches!(
        quoted.parameters.as_slice(),
        [Node::FunctionParameter(column)] if column.name.as_deref() == Some("select")
    ));
}

#[test]
fn create_function_populates_all_common_and_create_only_options() {
    let stmt = parse_node!(
        "create function app.optioned(a integer) returns integer
         called on null input returns null on null input
         external security invoker leakproof not leakproof
         transform for type integer, for type text window
         set work_mem to '4MB' reset search_path
         language sql as 'select a'",
        CreateFunctionStmt
    );
    let option = |name: &str, occurrence: usize| {
        stmt.options
            .iter()
            .filter_map(|node| match node {
                Node::DefElem(option) if option.defname.as_deref() == Some(name) => Some(option),
                _ => None,
            })
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {name} option {occurrence}"))
    };
    assert!(matches!(
        option("strict", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if !value.boolval
    ));
    assert!(matches!(
        option("strict", 1).arg.as_deref(),
        Some(Node::Boolean(value)) if value.boolval
    ));
    assert!(matches!(
        option("security", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if !value.boolval
    ));
    let transforms = expect_node!(option("transform", 0).arg.as_deref(), Some(AArrayExpr));
    assert_eq!(transforms.elements.len(), 2);
    assert!(
        transforms
            .elements
            .iter()
            .all(|node| matches!(node, Node::TypeName(_)))
    );
    assert!(matches!(
        option("window", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if value.boolval
    ));
    let set = expect_node!(option("set", 0).arg.as_deref(), Some(VariableSetStmt));
    assert_eq!(set.kind, VariableSetKind::SetValue);
    assert_eq!(set.name.as_deref(), Some("work_mem"));
    let reset = expect_node!(option("set", 1).arg.as_deref(), Some(VariableSetStmt));
    assert_eq!(reset.kind, VariableSetKind::Reset);
    assert_eq!(reset.name.as_deref(), Some("search_path"));
}

#[test]
fn create_function_begin_atomic_preserves_nested_statement_list() {
    let stmt = parse_node!(
        "create function app.atomic_body(a integer) returns integer
         begin atomic
           insert into audit_log values (a);
           return a + 1;
         end",
        CreateFunctionStmt
    );
    let outer = expect_node!(stmt.sql_body.as_deref(), Some(AArrayExpr));
    let [Node::AArrayExpr(inner)] = outer.elements.as_slice() else {
        panic!("expected nested routine statement list");
    };
    assert!(matches!(
        inner.elements.as_slice(),
        [Node::InsertStmt(_), Node::ReturnStmt(_)]
    ));
    let return_stmt = expect_node!(&inner.elements[1], ReturnStmt);
    assert!(matches!(
        return_stmt.returnval.as_deref(),
        Some(Node::AExpr(_))
    ));
}

#[test]
fn create_function_preserves_in_out_variadic_defaults_and_percent_type() {
    let sql = "create function app.typed(
             in out first app.source.value%type,
             second inout text,
             variadic rest integer[],
             fallback integer = 42
         ) returns setof app.source.value%type language sql as 'select first'";
    let stmt = parse_node!(sql, CreateFunctionStmt);
    let parameter = |index: usize| expect_node!(&stmt.parameters[index], FunctionParameter);
    assert_eq!(parameter(0).name.as_deref(), Some("first"));
    assert_eq!(parameter(0).mode, FunctionParameterMode::Inout);
    assert!(
        parameter(0)
            .arg_type
            .as_deref()
            .expect("first type")
            .pct_type
    );
    assert_eq!(parameter(1).mode, FunctionParameterMode::Inout);
    assert_eq!(parameter(2).mode, FunctionParameterMode::Variadic);
    assert_eq!(
        parameter(2)
            .arg_type
            .as_deref()
            .expect("variadic type")
            .array_bounds
            .len(),
        1
    );
    assert!(parameter(3).defexpr.is_some());

    let return_type = stmt.return_type.as_deref().expect("return type");
    assert!(return_type.pct_type);
    assert!(return_type.setof);
    assert_eq!(return_type.names.len(), 3);
    assert_eq!(
        return_type.location as usize,
        sql.rfind("app.source.value%type").unwrap()
    );

    let alternatives_sql = "create function app.parameter_alternatives(
        in named integer,
        named_mode inout text,
        plain bigint,
        in integer,
        text
    ) returns integer language sql as 'select 1'";
    let alternatives = parse_node!(alternatives_sql, CreateFunctionStmt);
    let parameter = |index: usize| expect_node!(&alternatives.parameters[index], FunctionParameter);
    assert_eq!(parameter(0).name.as_deref(), Some("named"));
    assert_eq!(parameter(0).mode, FunctionParameterMode::In);
    assert_eq!(parameter(1).name.as_deref(), Some("named_mode"));
    assert_eq!(parameter(1).mode, FunctionParameterMode::Inout);
    assert_eq!(parameter(2).name.as_deref(), Some("plain"));
    assert_eq!(parameter(2).mode, FunctionParameterMode::Default);
    assert!(parameter(3).name.is_none());
    assert_eq!(parameter(3).mode, FunctionParameterMode::In);
    assert!(parameter(4).name.is_none());
    assert_eq!(parameter(4).mode, FunctionParameterMode::Default);
    for (index, needle) in [
        (0, "in named"),
        (1, "named_mode"),
        (2, "plain bigint"),
        (3, "in integer"),
    ] {
        assert_eq!(
            parameter(index).location as usize,
            alternatives_sql.find(needle).unwrap(),
            "parameter alternative {index}"
        );
    }
    assert_eq!(
        parameter(4).location as usize,
        alternatives_sql.rfind("text").unwrap()
    );
}

#[test]
fn create_function_preserves_special_set_and_reset_options() {
    let sql = "create function app.configured() returns integer
         set time zone 'UTC'
         set schema 'app'
         set names 'UTF8'
         set role app_user
         set session authorization default
         set xml option document
         set transaction snapshot '00000003-0000001B-1'
         reset time zone
         reset transaction isolation level
         reset session authorization
         language sql as 'select 1'";
    let stmt = parse_node!(sql, CreateFunctionStmt);
    let settings = stmt
        .options
        .iter()
        .filter_map(|node| match node {
            Node::DefElem(option) if option.defname.as_deref() == Some("set") => {
                Some(expect_node!(option.arg.as_deref(), Some(VariableSetStmt)))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(settings.len(), 10);
    assert_eq!(settings[0].name.as_deref(), Some("timezone"));
    assert!(settings[0].jumble_args);
    assert_eq!(settings[1].name.as_deref(), Some("search_path"));
    assert_eq!(settings[2].name.as_deref(), Some("client_encoding"));
    assert_eq!(settings[3].name.as_deref(), Some("role"));
    assert_eq!(settings[4].kind, VariableSetKind::SetDefault);
    assert_eq!(settings[4].name.as_deref(), Some("session_authorization"));
    assert_eq!(settings[5].name.as_deref(), Some("xmloption"));
    assert_eq!(settings[6].kind, VariableSetKind::SetMulti);
    assert_eq!(settings[6].name.as_deref(), Some("TRANSACTION SNAPSHOT"));
    assert_eq!(settings[7].name.as_deref(), Some("timezone"));
    assert_eq!(settings[8].name.as_deref(), Some("transaction_isolation"));
    assert_eq!(settings[9].name.as_deref(), Some("session_authorization"));
    assert_eq!(settings[0].location, -1);
    assert_eq!(settings[1].location, sql.find("'app'").unwrap() as i32);
    assert_eq!(settings[2].location, sql.find("'UTF8'").unwrap() as i32);
    assert_eq!(settings[3].location, sql.find("app_user").unwrap() as i32);
    assert_eq!(settings[4].location, -1);
    assert_eq!(settings[5].location, -1);
    assert_eq!(
        settings[6].location,
        sql.find("'00000003-0000001B-1'").unwrap() as i32
    );
    assert!(
        settings[7..]
            .iter()
            .all(|setting| setting.kind == VariableSetKind::Reset)
    );
    assert!(settings[7..].iter().all(|setting| setting.location == -1));
}

#[test]
fn create_ordered_aggregate_preserves_direct_count_and_variadic_normalization() {
    let aggregate = parse_node!(
        "create aggregate app.collect(
             variadic direct_values integer[]
             order by variadic ordered_values integer[]
         ) (sfunc = app.collect_state, stype = bigint)",
        DefineStmt
    );
    let [Node::AArrayExpr(parameters), Node::Integer(direct_count)] = aggregate.args.as_slice()
    else {
        panic!("expected aggregate argument pair");
    };
    assert_eq!(direct_count.ival, 1);
    assert_eq!(parameters.elements.len(), 1);
    let parameter = expect_node!(&parameters.elements[0], FunctionParameter);
    assert_eq!(parameter.mode, FunctionParameterMode::Variadic);
    assert_eq!(parameter.name.as_deref(), Some("direct_values"));

    let ordered_only = parse_node!(
        "create aggregate app.rank_value(order by value integer)
         (sfunc = app.rank_state, stype = bigint)",
        DefineStmt
    );
    assert!(matches!(
        ordered_only.args.as_slice(),
        [Node::AArrayExpr(parameters), Node::Integer(count)]
            if parameters.elements.len() == 1 && count.ival == 0
    ));
}
