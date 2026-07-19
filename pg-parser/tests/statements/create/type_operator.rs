use super::*;

#[test]
fn create_object_names_and_option_names_follow_exact_keyword_categories() {
    let fdw = parse_node!(
        "create foreign data wrapper \"select\" options (select 'allowed-as-collabel')",
        CreateFdwStmt
    );
    assert_eq!(fdw.fdwname.as_deref(), Some("select"));
    assert!(matches!(
        fdw.options.as_slice(),
        [Node::DefElem(option)] if option.defname.as_deref() == Some("select")
    ));

    let database = parse_node!(
        "create database \"select\" with encoding = 'UTF8' \"from\" = 'custom'",
        CreatedbStmt
    );
    assert_eq!(database.dbname.as_deref(), Some("select"));
    assert!(database.options.iter().any(
        |node| matches!(node, Node::DefElem(option) if option.defname.as_deref() == Some("from"))
    ));

    let schema = parse_node!("create schema \"select\"", CreateSchemaStmt);
    assert_eq!(schema.schemaname.as_deref(), Some("select"));

    let index = parse_node!("create index \"select\" on items (id)", IndexStmt);
    assert_eq!(index.idxname.as_deref(), Some("select"));
}

#[test]
fn type_names_preserve_setof_interval_and_default_modifiers() {
    let table = parse_node!(
        "create table temporal_values (duration interval day to second(3), local_time time(2) without time zone, fixed_bits bit)",
        CreateStmt
    );
    let duration = expect_node!(&table.table_elts[0], ColumnDef);
    let duration_type = duration.type_name.as_deref().expect("duration TypeName");
    assert_eq!(duration_type.typmods.len(), 2);

    let local_time = expect_node!(&table.table_elts[1], ColumnDef);
    let local_time_type = local_time.type_name.as_deref().expect("time TypeName");
    assert!(matches!(
        local_time_type.names.last(),
        Some(Node::String(name)) if name.sval.as_deref() == Some("time")
    ));
    assert_eq!(local_time_type.typmods.len(), 1);

    let bits = expect_node!(&table.table_elts[2], ColumnDef);
    assert_eq!(
        bits.type_name
            .as_deref()
            .expect("bit TypeName")
            .typmods
            .len(),
        1
    );

    let sql =
        "create function all_items() returns setof app.item_type[] language sql as 'select null'";
    let function = parse_node!(sql, CreateFunctionStmt);
    let return_type = function.return_type.as_deref().expect("return TypeName");
    assert!(return_type.setof);
    assert_eq!(return_type.array_bounds.len(), 1);
    assert_eq!(
        return_type.location as usize,
        sql.find("app.item_type").unwrap()
    );
}

#[test]
fn create_operator_class_family_and_rule_populate_nested_nodes() {
    let opclass = parse_node!(
        "create operator class app.int_ops default for type int using btree family app.int_family as operator 1 = for search, operator 2 <(int, int) for order by app.int_family, operator 3 >(int, int), function 1 app.compare_int(int, int), function 2 (int, bigint) app.compare_mixed(int, bigint), storage bigint",
        CreateOpClassStmt
    );
    assert!(opclass.is_default);
    assert_eq!(opclass.opclassname.len(), 2);
    assert_eq!(opclass.opfamilyname.len(), 2);
    assert_eq!(opclass.amname.as_deref(), Some("btree"));
    assert!(opclass.datatype.is_some());
    assert_eq!(opclass.items.len(), 6);
    assert!(
        opclass
            .items
            .iter()
            .all(|item| matches!(item, Node::CreateOpClassItem(_)))
    );
    let search = expect_node!(&opclass.items[0], CreateOpClassItem);
    assert!(matches!(
        search.name.as_deref(),
        Some(name) if !name.args_unspecified && name.objargs.is_empty()
    ));
    let ordering = expect_node!(&opclass.items[1], CreateOpClassItem);
    assert_eq!(ordering.itemtype, 1);
    assert_eq!(ordering.number, 2);
    assert!(matches!(
        ordering.order_family.as_slice(),
        [Node::String(schema), Node::String(family)]
            if schema.sval.as_deref() == Some("app")
                && family.sval.as_deref() == Some("int_family")
    ));
    let no_purpose = expect_node!(&opclass.items[2], CreateOpClassItem);
    assert!(no_purpose.order_family.is_empty());

    let class_args = expect_node!(&opclass.items[4], CreateOpClassItem);
    assert_eq!(class_args.itemtype, 2);
    assert_eq!(class_args.class_args.len(), 2);

    let storage = expect_node!(&opclass.items[5], CreateOpClassItem);
    assert!(storage.storedtype.is_some());

    let family = parse_node!(
        "create operator family app.int_family using btree",
        CreateOpFamilyStmt
    );
    assert_eq!(family.opfamilyname.len(), 2);
    assert_eq!(family.amname.as_deref(), Some("btree"));

    let rule = parse_node!(
        "create or replace rule audit_items as on update to app.items where old.id > 0 do instead (notify audit_channel, 'updated'; update app.audit set item_id = new.id)",
        RuleStmt
    );
    assert!(rule.replace);
    assert_eq!(rule.rulename.as_deref(), Some("audit_items"));
    assert!(rule.relation.is_some());
    assert!(rule.where_clause.is_some());
    assert!(rule.instead);
    assert_eq!(rule.event, CmdType::Update);
    assert_eq!(rule.actions.len(), 2);

    let empty_actions = parse_node!(
        "create rule no_actions as on insert to app.items do ()",
        RuleStmt
    );
    assert!(!empty_actions.instead);
    assert!(empty_actions.actions.is_empty());

    let single_select = parse_node!(
        "create rule select_action as on select to app.items do also select * from app.audit",
        RuleStmt
    );
    assert_eq!(single_select.actions.len(), 1);
    assert!(matches!(single_select.actions[0], Node::SelectStmt(_)));

    let mixed_actions = parse_node!(
        "create rule mixed_actions as on insert to app.items do (;; insert into app.audit values (new.id);; delete from app.pending where id = new.id;;)",
        RuleStmt
    );
    assert!(matches!(
        mixed_actions.actions.as_slice(),
        [Node::InsertStmt(_), Node::DeleteStmt(_)]
    ));
}

#[test]
fn create_operator_definition_preserves_explicit_qualified_operator_values() {
    let operator = parse_node!(
        "create operator === (
            leftarg = integer,
            rightarg = integer,
            function = app.equal_integer,
            commutator = operator(app.===)
        )",
        DefineStmt
    );
    let commutator = operator
        .definition
        .iter()
        .find_map(|node| match node {
            Node::DefElem(def) if def.defname.as_deref() == Some("commutator") => Some(def),
            _ => None,
        })
        .expect("commutator DefElem");
    assert!(matches!(
        commutator.arg.as_deref(),
        Some(Node::AArrayExpr(names))
            if matches!(names.elements.as_slice(),
                [Node::String(schema), Node::String(operator)]
                    if schema.sval.as_deref() == Some("app")
                        && operator.sval.as_deref() == Some("===")
            )
    ));
}

#[test]
fn define_stmt_populates_aggregate_operator_type_collation_and_text_search() {
    let aggregate = parse_node!(
        "create or replace aggregate app.sum2(int) (sfunc = app.sum_state, stype = bigint, initcond = '0')",
        DefineStmt
    );
    assert_eq!(aggregate.kind, pg_parser::ObjectType::Aggregate);
    assert!(aggregate.replace);
    assert!(!aggregate.oldstyle);
    assert_eq!(aggregate.args.len(), 2);
    assert_eq!(aggregate.definition.len(), 3);

    let old_aggregate = parse_node!(
        "create aggregate app.old_sum (sfunc = app.sum_state, basetype = int, stype = bigint, initcond = '0', finalfunc = none)",
        DefineStmt
    );
    assert_eq!(old_aggregate.kind, pg_parser::ObjectType::Aggregate);
    assert!(old_aggregate.oldstyle);
    assert!(old_aggregate.args.is_empty());
    assert_eq!(old_aggregate.definition.len(), 5);
    let finalfunc = expect_node!(&old_aggregate.definition[4], DefElem);
    assert!(
        matches!(finalfunc.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("none"))
    );

    let operator = parse_node!(
        "create operator === (leftarg = int, rightarg = int, function = app.eq_int)",
        DefineStmt
    );
    assert_eq!(operator.kind, pg_parser::ObjectType::Operator);
    assert_eq!(operator.defnames.len(), 1);
    assert_eq!(operator.definition.len(), 3);
    let leftarg = expect_node!(&operator.definition[0], DefElem);
    assert!(matches!(leftarg.arg.as_deref(), Some(Node::TypeName(_))));
    let function = expect_node!(&operator.definition[2], DefElem);
    assert!(matches!(function.arg.as_deref(), Some(Node::TypeName(_))));

    let base_type = parse_node!(
        "create type app.custom_value (input = app.custom_in, output = app.custom_out)",
        DefineStmt
    );
    assert_eq!(base_type.kind, pg_parser::ObjectType::Type);
    assert_eq!(base_type.definition.len(), 2);

    let collation = parse_node!(
        "create collation if not exists app.english from pg_catalog.english",
        DefineStmt
    );
    assert_eq!(collation.kind, pg_parser::ObjectType::Collation);
    assert!(collation.if_not_exists);
    assert_eq!(collation.definition.len(), 1);

    let search = parse_node!(
        "create text search configuration app.english_search (parser = pg_catalog.default)",
        DefineStmt
    );
    assert_eq!(search.kind, pg_parser::ObjectType::Tsconfiguration);
    assert_eq!(search.definition.len(), 1);

    for (sql, expected) in [
        (
            "create text search parser app.custom_parser (start = app.parser_start)",
            pg_parser::ObjectType::Tsparser,
        ),
        (
            "create text search dictionary app.custom_dictionary (template = pg_catalog.simple)",
            pg_parser::ObjectType::Tsdictionary,
        ),
        (
            "create text search template app.custom_template (lexize = app.template_lexize)",
            pg_parser::ObjectType::Tstemplate,
        ),
    ] {
        let stmt = parse_node!(sql, DefineStmt);
        assert_eq!(stmt.kind, expected);
        assert_eq!(stmt.definition.len(), 1);
    }
}

#[test]
fn qualified_any_names_and_function_names_use_distinct_first_token_categories() {
    let domain = parse_node!("create domain app.select as integer", CreateDomainStmt);
    assert!(matches!(
        domain.domainname.as_slice(),
        [Node::String(schema), Node::String(name)]
            if schema.sval.as_deref() == Some("app") && name.sval.as_deref() == Some("select")
    ));

    let type_keyword = parse_node!(
        "create function authorization() returns integer language sql as 'select 1'",
        CreateFunctionStmt
    );
    assert!(matches!(
        type_keyword.funcname.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("authorization")
    ));

    let quoted = parse_node!(
        "create function \"select\"() returns integer language sql as 'select 1'",
        CreateFunctionStmt
    );
    assert!(matches!(
        quoted.funcname.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("select")
    ));
}
