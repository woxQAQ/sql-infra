use super::*;

#[test]
fn create_schema_populates_nested_schema_statements() {
    let schema = parse_node!(
        "create schema app
         create table items (id integer)
         create index items_idx on items (id)
         create domain positive_integer as integer check (value > 0)
         create function answer() returns integer language sql as 'select 42'
         create sequence item_seq
         create trigger items_trigger before insert on items for each row execute function answer()
         create type shell_type
         grant select on table items to public
         create view item_ids as select id from items",
        CreateSchemaStmt
    );
    assert_eq!(schema.schemaname.as_deref(), Some("app"));
    assert_eq!(
        schema.schema_elts.iter().map(Node::tag).collect::<Vec<_>>(),
        [
            NodeTag::CreateStmt,
            NodeTag::IndexStmt,
            NodeTag::CreateDomainStmt,
            NodeTag::CreateFunctionStmt,
            NodeTag::CreateSeqStmt,
            NodeTag::CreateTrigStmt,
            NodeTag::DefineStmt,
            NodeTag::GrantStmt,
            NodeTag::ViewStmt,
        ]
    );

    let authorized = parse_node!(
        "create schema analytics authorization current_user",
        CreateSchemaStmt
    );
    assert_eq!(authorized.schemaname.as_deref(), Some("analytics"));
    assert!(authorized.authrole.is_some());
    assert!(!authorized.if_not_exists);

    let authorization_only = parse_node!("create schema authorization app_owner", CreateSchemaStmt);
    assert!(authorization_only.schemaname.is_none());
    assert!(authorization_only.authrole.is_some());

    let if_not_exists = parse_node!(
        "create schema if not exists authorization app_owner",
        CreateSchemaStmt
    );
    assert!(if_not_exists.if_not_exists);
    assert!(if_not_exists.schemaname.is_none());
    assert!(if_not_exists.authrole.is_some());
}

#[test]
fn create_role_sequence_domain_and_type_forms_populate_options() {
    let role = parse_node!(
        "create role app_user with login nosuperuser inherit connection limit 5 valid until 'infinity'",
        CreateRoleStmt
    );
    assert_eq!(role.role.as_deref(), Some("app_user"));
    assert_eq!(role.stmt_type, pg_parser::RoleStmtType::Role);
    assert_eq!(role.options.len(), 5);

    let user = parse_node!("create user app_login", CreateRoleStmt);
    assert_eq!(user.stmt_type, pg_parser::RoleStmtType::User);

    let group = parse_node!("create group app_group", CreateRoleStmt);
    assert_eq!(group.stmt_type, pg_parser::RoleStmtType::Group);

    let quoted_role = parse_node!(r#"create role "select""#, CreateRoleStmt);
    assert_eq!(quoted_role.role.as_deref(), Some("select"));

    for role_name in ["PUBLIC", "NONE"] {
        let sql = format!(r#"create role "{role_name}""#);
        let role = parse_node!(&sql, CreateRoleStmt);
        assert_eq!(role.role.as_deref(), Some(role_name));
    }

    for (sql, defname) in [
        ("create role r1 admin alice", "adminmembers"),
        ("create role r2 role alice, bob", "rolemembers"),
        ("create role r3 user alice", "rolemembers"),
        ("create role r4 in role parent", "addroleto"),
        ("create role r5 in group parent", "addroleto"),
    ] {
        let role = parse_node!(sql, CreateRoleStmt);
        let option = expect_node!(&role.options[0], DefElem);
        assert_eq!(option.defname.as_deref(), Some(defname), "{sql}");
        assert!(matches!(option.arg.as_deref(), Some(Node::AArrayExpr(_))));
    }

    let legacy = parse_node!("create role legacy sysid 42", CreateRoleStmt);
    assert!(matches!(
        legacy.options.as_slice(),
        [Node::DefElem(option)]
            if option.defname.as_deref() == Some("sysid")
                && matches!(option.arg.as_deref(), Some(Node::Integer(value)) if value.ival == 42)
    ));

    let sequence = parse_node!(
        "create sequence if not exists app.events_id_seq as bigint increment by 2 minvalue 1 maxvalue 999 start with 10 cache 20 cycle owned by app.events.id",
        CreateSeqStmt
    );
    assert!(sequence.if_not_exists);
    assert!(sequence.sequence.is_some());
    assert_eq!(sequence.options.len(), 8);
    assert!(!sequence.for_identity);
    let option = |index| expect_node!(&sequence.options[index], DefElem);
    assert!(matches!(option(0).arg.as_deref(), Some(Node::TypeName(_))));
    for index in 1..=5 {
        assert!(matches!(
            option(index).arg.as_deref(),
            Some(Node::Integer(_))
        ));
    }
    assert!(matches!(option(6).arg.as_deref(), Some(Node::Boolean(value)) if value.boolval));
    assert!(matches!(
        option(7).arg.as_deref(),
        Some(Node::AArrayExpr(_))
    ));

    let fractional_sequence = parse_node!(
        "create sequence fractional_seq increment by 1.5",
        CreateSeqStmt
    );
    let increment = expect_node!(&fractional_sequence.options[0], DefElem);
    assert!(matches!(increment.arg.as_deref(), Some(Node::Float(_))));

    for (sql, expected_names) in [
        ("create sequence keyword_type as cache restart", 1),
        (
            "create sequence qualified_keyword_type as app.restart cache 2",
            2,
        ),
    ] {
        let sequence = parse_node!(sql, CreateSeqStmt);
        let as_type = expect_node!(&sequence.options[0], DefElem);
        assert!(matches!(
            as_type.arg.as_deref(),
            Some(Node::TypeName(type_name)) if type_name.names.len() == expected_names
        ));
    }

    let sequence = parse_node!(
        "create sequence sequence_options no minvalue no maxvalue no cycle logged unlogged restart sequence name app.internal_seq",
        CreateSeqStmt
    );
    assert!(matches!(
        sequence.options.as_slice(),
        [Node::DefElem(min), Node::DefElem(max), Node::DefElem(cycle), Node::DefElem(logged), Node::DefElem(unlogged), Node::DefElem(restart), Node::DefElem(name)]
            if min.defname.as_deref() == Some("minvalue") && min.arg.is_none()
                && max.defname.as_deref() == Some("maxvalue") && max.arg.is_none()
                && cycle.defname.as_deref() == Some("cycle")
                    && matches!(cycle.arg.as_deref(), Some(Node::Boolean(value)) if !value.boolval)
                && logged.defname.as_deref() == Some("logged") && logged.arg.is_none()
                && unlogged.defname.as_deref() == Some("unlogged") && unlogged.arg.is_none()
                && restart.defname.as_deref() == Some("restart") && restart.arg.is_none()
                && name.defname.as_deref() == Some("sequence_name")
                    && matches!(name.arg.as_deref(), Some(Node::AArrayExpr(_)))
    ));

    let domain = parse_node!(
        "create domain app.positive_int as int default 1 not null check (value > 0)",
        CreateDomainStmt
    );
    assert_eq!(domain.domainname.len(), 2);
    assert!(domain.type_name.is_some());
    assert_eq!(domain.constraints.len(), 3);

    let collated_domain = parse_node!(
        "create domain app.label as text collate pg_catalog.\"C\"",
        CreateDomainStmt
    );
    assert!(collated_domain.coll_clause.is_some());

    let enum_type = parse_node!(
        "create type app.mood as enum ('sad', 'ok', 'happy')",
        CreateEnumStmt
    );
    assert_eq!(enum_type.vals.len(), 3);

    let empty_enum = parse_node!("create type empty_enum as enum ()", CreateEnumStmt);
    assert!(empty_enum.vals.is_empty());

    let empty_composite = parse_node!("create type app.empty_composite as ()", CompositeTypeStmt);
    assert!(empty_composite.coldeflist.is_empty());
    assert!(empty_composite.typevar.is_some());

    let composite = parse_node!(
        "create type app.pair as (left_value int collate pg_catalog.default, right_value int)",
        CompositeTypeStmt
    );
    assert_eq!(
        composite.typevar.as_deref().map(|typevar| typevar.location),
        Some(12)
    );
    let left = expect_node!(&composite.coldeflist[0], ColumnDef);
    assert!(left.coll_clause.is_some());

    let range_type = parse_node!(
        "create type app.price_range as range (subtype = int, collation = default)",
        CreateRangeStmt
    );
    assert_eq!(range_type.params.len(), 2);

    let composite = parse_node!(
        "create type app.pair as (left_value int, right_value text)",
        CompositeTypeStmt
    );
    assert_eq!(composite.coldeflist.len(), 2);
    let typevar = composite
        .typevar
        .as_deref()
        .expect("composite type RangeVar");
    assert_eq!(typevar.schemaname.as_deref(), Some("app"));
    assert_eq!(typevar.relname.as_deref(), Some("pair"));
}

#[test]
fn create_schema_keeps_begin_atomic_function_bodies_inside_the_schema_element() {
    let schema = parse_node!(
        "create schema app
         create function answer(flag boolean) returns integer language sql begin atomic
             return case when flag then 42 else 0 end;
         end
         create table answers (value integer)",
        CreateSchemaStmt
    );
    assert!(matches!(
        schema.schema_elts.as_slice(),
        [Node::CreateFunctionStmt(function), Node::CreateStmt(_)]
            if matches!(function.sql_body.as_deref(), Some(Node::AArrayExpr(_)))
    ));
}
