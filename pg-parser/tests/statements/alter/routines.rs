use super::*;

#[test]
fn alter_operator_family_populates_add_and_drop_items() {
    let add = parse_node!(
        "alter operator family app.numeric_ops using btree add operator 1 <(int, int) for search, function 1 (int, int) app.compare(int, int)",
        AlterOpFamilyStmt
    );
    assert!(!add.is_drop);
    assert_eq!(add.opfamilyname.len(), 2);
    assert_eq!(add.amname.as_deref(), Some("btree"));
    assert_eq!(add.items.len(), 2);

    let drop = parse_node!(
        "alter operator family app.numeric_ops using btree drop operator 1 (int, int), function 2 (int, int)",
        AlterOpFamilyStmt
    );
    assert!(drop.is_drop);
    assert_eq!(drop.items.len(), 2);
    let operator = expect_node!(&drop.items[0], CreateOpClassItem);
    assert_eq!(operator.itemtype, 1);
    assert_eq!(operator.number, 1);
    assert_eq!(operator.class_args.len(), 2);
}

#[test]
fn alter_function_and_operator_populate_typed_actions() {
    let function = parse_node!(
        "alter function app.calculate(in value int, out result text) immutable strict security definer cost 10 rows 2 support app.calculate_support set work_mem to '4MB' parallel safe restrict",
        AlterFunctionStmt
    );
    assert_eq!(function.objtype, ObjectType::Function);
    let func = function.func.as_deref().expect("ObjectWithArgs");
    assert_eq!(func.objargs.len(), 2);
    assert_eq!(func.objfuncargs.len(), 2);
    assert!(matches!(
        func.objfuncargs.as_slice(),
        [Node::FunctionParameter(input), Node::FunctionParameter(output)]
            if input.name.as_deref() == Some("value")
                && input.mode == pg_parser::FunctionParameterMode::In
                && output.name.as_deref() == Some("result")
                && output.mode == pg_parser::FunctionParameterMode::Out
    ));
    assert_eq!(function.actions.len(), 8);
    assert_eq!(
        def(&function.actions[0]).defname.as_deref(),
        Some("volatility")
    );
    assert_eq!(def(&function.actions[1]).defname.as_deref(), Some("strict"));
    assert_eq!(
        def(&function.actions[2]).defname.as_deref(),
        Some("security")
    );
    let set_action = def(&function.actions[6]);
    let setstmt = expect_node!(set_action.arg.as_deref(), Some(VariableSetStmt));
    assert_eq!(setstmt.name.as_deref(), Some("work_mem"));
    assert_eq!(setstmt.kind, VariableSetKind::SetValue);

    let operator = parse_node!(
        "alter operator app.=(int, int) set (restrict = app.eqsel, joins = app.eqjoinsel, commutator = none)",
        AlterOperatorStmt
    );
    assert!(operator.opername.is_some());
    assert_eq!(operator.options.len(), 3);
    assert!(matches!(
        def(&operator.options[0]).arg.as_deref(),
        Some(Node::TypeName(_))
    ));
    assert!(def(&operator.options[2]).arg.is_none());

    let unary = parse_node!(
        "alter operator app.-(none, int) set (restrict = app.int4umsel)",
        AlterOperatorStmt
    );
    let signature = unary.opername.as_deref().expect("operator signature");
    assert!(matches!(
        signature.objargs.as_slice(),
        [None, Some(Node::TypeName(_))]
    ));
}
