use pg_parser::{
    AlterDomainType, AlterPropGraphElementKind, AlterSubscriptionType, AlterTableType,
    AlterTsConfigType, ConstrType, DefElem, DefElemAction, DropBehavior, Node, ObjectType,
    VariableSetKind,
};

use super::common::{expect_node, parse_node};

fn def(node: &Node) -> &DefElem {
    expect_node!(node, DefElem)
}

#[test]
fn alter_table_move_all_stmt_populates_tablespaces_roles_and_nowait() {
    let stmt = parse_node!(
        "alter index all in tablespace old_space owned by alice, bob set tablespace new_space nowait",
        AlterTableMoveAllStmt
    );
    assert_eq!(stmt.objtype, ObjectType::Index);
    assert_eq!(stmt.orig_tablespacename.as_deref(), Some("old_space"));
    assert_eq!(stmt.new_tablespacename.as_deref(), Some("new_space"));
    assert_eq!(stmt.roles.len(), 2);
    assert!(stmt.nowait);
}

#[test]
fn replica_identity_stmt_is_nested_in_alter_table_cmd() {
    let stmt = parse_node!(
        "alter table items replica identity using index items_identity_idx",
        AlterTableStmt
    );
    let cmd = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(cmd.subtype, AlterTableType::ReplicaIdentity);
    let identity = expect_node!(cmd.def.as_deref(), Some(ReplicaIdentityStmt));
    assert_eq!(identity.identity_type, b'i');
    assert_eq!(identity.name.as_deref(), Some("items_identity_idx"));
}

#[test]
fn alter_table_partition_commands_build_partition_cmd_and_single_specs() {
    let attach = parse_node!(
        "alter table events attach partition events_2026 for values from ('2026-01-01') to ('2027-01-01')",
        AlterTableStmt
    );
    let attach_cmd = expect_node!(&attach.cmds[0], AlterTableCmd);
    assert_eq!(attach_cmd.subtype, AlterTableType::AttachPartition);
    let attach_partition = expect_node!(attach_cmd.def.as_deref(), Some(PartitionCmd));
    assert!(attach_partition.name.is_some());
    assert!(attach_partition.bound.is_some());

    let detach = parse_node!(
        "alter table events detach partition events_old concurrently",
        AlterTableStmt
    );
    let detach_cmd = expect_node!(
        detach.cmds.first().expect("concurrent detach command"),
        AlterTableCmd
    );
    let detach_partition = expect_node!(detach_cmd.def.as_deref(), Some(PartitionCmd));
    assert!(detach_partition.concurrent);

    let split = parse_node!(
        "alter table events split partition events_old into (partition events_a for values from (minvalue) to ('2020-01-01'), partition events_b for values from ('2020-01-01') to (maxvalue))",
        AlterTableStmt
    );
    let split_cmd = expect_node!(&split.cmds[0], AlterTableCmd);
    assert_eq!(split_cmd.subtype, AlterTableType::SplitPartition);
    let split_partition = expect_node!(split_cmd.def.as_deref(), Some(PartitionCmd));
    assert_eq!(split_partition.partlist.len(), 2);
    assert!(
        split_partition
            .partlist
            .iter()
            .all(|node| matches!(node, Node::SinglePartitionSpec(_)))
    );
}

#[test]
fn alter_sequence_database_system_and_tablespace_populate_options() {
    let sequence = parse_node!(
        "alter sequence if exists app.order_ids increment by 5 restart with 20 no cycle",
        AlterSeqStmt
    );
    let sequence_name = sequence.sequence.as_deref().expect("sequence name");
    assert_eq!(sequence_name.schemaname.as_deref(), Some("app"));
    assert_eq!(sequence_name.relname.as_deref(), Some("order_ids"));
    assert!(sequence.missing_ok);
    assert!(!sequence.for_identity);
    assert_eq!(
        sequence
            .options
            .iter()
            .map(|node| def(node).defname.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["increment", "restart", "cycle"]
    );

    let database = parse_node!(
        "alter database analytics connection limit = 50 allow_connections true",
        AlterDatabaseStmt
    );
    assert_eq!(database.dbname.as_deref(), Some("analytics"));
    assert_eq!(database.options.len(), 2);
    assert_eq!(
        def(&database.options[0]).defname.as_deref(),
        Some("connection_limit")
    );
    assert_eq!(
        def(&database.options[1]).defname.as_deref(),
        Some("allow_connections")
    );

    let tablespace_sql = "alter database analytics set tablespace fast_space";
    let database = parse_node!(tablespace_sql, AlterDatabaseStmt);
    assert_eq!(
        def(&database.options[0]).defname.as_deref(),
        Some("tablespace")
    );
    assert_eq!(
        def(&database.options[0]).location as usize,
        tablespace_sql.find("fast_space").unwrap()
    );

    let refresh = parse_node!(
        "alter database \"select\" refresh collation version",
        AlterDatabaseRefreshCollStmt
    );
    assert_eq!(refresh.dbname.as_deref(), Some("select"));

    let database_set = parse_node!(
        "alter database analytics reset search_path",
        AlterDatabaseSetStmt
    );
    let setstmt = database_set.setstmt.as_deref().expect("set statement");
    assert_eq!(setstmt.kind, VariableSetKind::Reset);
    assert_eq!(setstmt.name.as_deref(), Some("search_path"));
    assert_eq!(setstmt.location, -1);

    let database_set = parse_node!(
        "alter database analytics set time zone 'UTC'",
        AlterDatabaseSetStmt
    );
    let setstmt = database_set.setstmt.as_deref().expect("set statement");
    assert_eq!(setstmt.name.as_deref(), Some("timezone"));
    assert_eq!(setstmt.location, -1);

    let role_set = parse_node!(
        "alter role analyst set session authorization default",
        AlterRoleSetStmt
    );
    let setstmt = role_set.setstmt.as_deref().expect("set statement");
    assert_eq!(setstmt.kind, VariableSetKind::SetDefault);
    assert_eq!(setstmt.name.as_deref(), Some("session_authorization"));
    assert_eq!(setstmt.location, -1);

    let system = parse_node!("alter system set work_mem = '64MB'", AlterSystemStmt);
    let setstmt = system.setstmt.as_deref().expect("system set statement");
    assert_eq!(setstmt.kind, VariableSetKind::SetValue);
    assert_eq!(setstmt.location, 28);
    assert_eq!(setstmt.name.as_deref(), Some("work_mem"));
    assert_eq!(setstmt.args.len(), 1);

    let tablespace = parse_node!(
        "alter tablespace fast_space reset (random_page_cost, seq_page_cost)",
        AlterTableSpaceOptionsStmt
    );
    assert_eq!(tablespace.tablespacename.as_deref(), Some("fast_space"));
    assert!(tablespace.is_reset);
    assert_eq!(tablespace.options.len(), 2);
}

#[test]
fn alter_role_and_enum_preserve_actions_and_values() {
    let role = parse_node!(
        "alter role alice with superuser nologin connection limit -1 valid until 'infinity'",
        AlterRoleStmt
    );
    assert_eq!(
        role.role
            .as_deref()
            .and_then(|role| role.rolename.as_deref()),
        Some("alice")
    );
    assert_eq!(role.action, 1);
    assert_eq!(role.options.len(), 4);
    assert_eq!(def(&role.options[0]).defname.as_deref(), Some("superuser"));
    assert_eq!(def(&role.options[1]).defname.as_deref(), Some("canlogin"));

    let group = parse_node!("alter group developers drop user alice, bob", AlterRoleStmt);
    assert_eq!(group.action, -1);
    assert_eq!(
        def(&group.options[0]).defname.as_deref(),
        Some("rolemembers")
    );
    assert_eq!(def(&group.options[0]).location, 33);

    let all_roles = parse_node!(
        "alter role all in database analytics set search_path to app, public",
        AlterRoleSetStmt
    );
    assert!(all_roles.role.is_none());
    assert_eq!(all_roles.database.as_deref(), Some("analytics"));
    assert_eq!(
        all_roles
            .setstmt
            .as_deref()
            .expect("set statement")
            .args
            .len(),
        2
    );

    let add = parse_node!(
        "alter type mood add value if not exists 'happy' before 'sad'",
        AlterEnumStmt
    );
    assert_eq!(add.type_name.len(), 1);
    assert_eq!(add.new_val.as_deref(), Some("happy"));
    assert_eq!(add.new_val_neighbor.as_deref(), Some("sad"));
    assert!(!add.new_val_is_after);
    assert!(add.skip_if_new_val_exists);

    let rename = parse_node!(
        "alter type mood rename value 'sad' to 'unhappy'",
        AlterEnumStmt
    );
    assert_eq!(rename.old_val.as_deref(), Some("sad"));
    assert_eq!(rename.new_val.as_deref(), Some("unhappy"));
}

#[test]
fn alter_stats_event_trigger_fdw_and_server_are_field_complete() {
    let stats = parse_node!(
        "alter statistics if exists app.orders_stats set statistics -25",
        AlterStatsStmt
    );
    assert!(stats.missing_ok);
    assert_eq!(stats.defnames.len(), 2);
    assert!(
        matches!(stats.stxstattarget.as_deref(), Some(Node::Integer(value)) if value.ival == -25)
    );

    let trigger = parse_node!(
        "alter event trigger audit_ddl enable always",
        AlterEventTrigStmt
    );
    assert_eq!(trigger.trigname.as_deref(), Some("audit_ddl"));
    assert_eq!(trigger.tgenabled, b'A');

    for (sql, expected) in [
        ("alter event trigger audit_ddl enable", b'O'),
        ("alter event trigger audit_ddl enable replica", b'R'),
        ("alter event trigger audit_ddl disable", b'D'),
    ] {
        let mode = parse_node!(sql, AlterEventTrigStmt);
        assert_eq!(mode.tgenabled, expected, "{sql}");
    }

    let fdw_sql = "alter foreign data wrapper remote_fdw no validator handler app.remote_handler options (set host 'db', drop port)";
    let fdw = parse_node!(fdw_sql, AlterFdwStmt);
    assert_eq!(fdw.fdwname.as_deref(), Some("remote_fdw"));
    assert_eq!(fdw.func_options.len(), 2);
    assert_eq!(fdw.options.len(), 2);
    assert_eq!(def(&fdw.options[0]).defaction, DefElemAction::Set);
    assert_eq!(def(&fdw.options[1]).defaction, DefElemAction::Drop);
    assert_eq!(
        def(&fdw.options[0]).location,
        fdw_sql.find("host").unwrap() as i32
    );
    assert_eq!(
        def(&fdw.options[1]).location,
        fdw_sql.find("port").unwrap() as i32
    );

    let server = parse_node!(
        "alter server remote version null options (add host 'db')",
        AlterForeignServerStmt
    );
    assert_eq!(server.servername.as_deref(), Some("remote"));
    assert!(server.has_version);
    assert!(server.version.is_none());
    assert_eq!(def(&server.options[0]).defaction, DefElemAction::Add);
}

#[test]
fn alter_subscription_populates_each_action_payload() {
    let connection = parse_node!(
        "alter subscription sub connection 'host=db'",
        AlterSubscriptionStmt
    );
    assert_eq!(connection.subname.as_deref(), Some("sub"));
    assert_eq!(connection.kind, AlterSubscriptionType::Connection);
    assert_eq!(connection.conninfo.as_deref(), Some("host=db"));

    let server = parse_node!(
        "alter subscription sub server logical_server",
        AlterSubscriptionStmt
    );
    assert_eq!(server.kind, AlterSubscriptionType::Server);
    assert_eq!(server.servername.as_deref(), Some("logical_server"));

    let publications = parse_node!(
        "alter subscription sub set publication pub_a, pub_b with (copy_data = false)",
        AlterSubscriptionStmt
    );
    assert_eq!(publications.kind, AlterSubscriptionType::SetPublication);
    assert_eq!(publications.publication.len(), 2);
    assert_eq!(publications.options.len(), 1);

    for (sql, expected) in [
        (
            "alter subscription sub add publication pub_c with (copy_data = true)",
            AlterSubscriptionType::AddPublication,
        ),
        (
            "alter subscription sub drop publication pub_c with (refresh = false)",
            AlterSubscriptionType::DropPublication,
        ),
    ] {
        let stmt = parse_node!(sql, AlterSubscriptionStmt);
        assert_eq!(stmt.kind, expected);
        assert_eq!(stmt.publication.len(), 1);
        assert_eq!(stmt.options.len(), 1);
    }

    let options = parse_node!(
        "alter subscription sub set (slot_name = none, binary)",
        AlterSubscriptionStmt
    );
    assert_eq!(options.kind, AlterSubscriptionType::Options);
    assert_eq!(options.options.len(), 2);

    let refresh = parse_node!(
        "alter subscription sub refresh publication with (copy_data = false)",
        AlterSubscriptionStmt
    );
    assert_eq!(refresh.kind, AlterSubscriptionType::RefreshPublication);
    assert_eq!(refresh.options.len(), 1);

    let refresh_sequences = parse_node!(
        "alter subscription sub refresh sequences",
        AlterSubscriptionStmt
    );
    assert_eq!(
        refresh_sequences.kind,
        AlterSubscriptionType::RefreshSequences
    );

    let enable = parse_node!("alter subscription sub enable", AlterSubscriptionStmt);
    assert_eq!(enable.kind, AlterSubscriptionType::Enabled);
    assert_eq!(def(&enable.options[0]).location, 0);

    let enabled = parse_node!("alter subscription sub disable", AlterSubscriptionStmt);
    assert_eq!(enabled.kind, AlterSubscriptionType::Enabled);
    let enabled_arg = def(&enabled.options[0]).arg.as_deref();
    assert!(matches!(enabled_arg, Some(Node::Boolean(value)) if !value.boolval));
    assert_eq!(def(&enabled.options[0]).location, 0);

    let skip = parse_node!(
        "alter subscription sub skip (lsn = '0/16B6C50')",
        AlterSubscriptionStmt
    );
    assert_eq!(skip.kind, AlterSubscriptionType::Skip);
    assert_eq!(def(&skip.options[0]).defname.as_deref(), Some("lsn"));
}

#[test]
fn alter_user_mapping_type_collation_and_policy_are_strict_and_complete() {
    let mapping = parse_node!(
        "alter user mapping for current_user server remote options (set user 'alice', drop password)",
        AlterUserMappingStmt
    );
    assert_eq!(mapping.servername.as_deref(), Some("remote"));
    assert_eq!(mapping.options.len(), 2);
    assert_eq!(def(&mapping.options[0]).defaction, DefElemAction::Set);
    assert_eq!(def(&mapping.options[1]).defaction, DefElemAction::Drop);

    let alter_type = parse_node!(
        "alter type app.currency set (internallength = 8, input = app.currency_in, passedbyvalue)",
        AlterTypeStmt
    );
    assert_eq!(alter_type.type_name.len(), 2);
    assert_eq!(alter_type.options.len(), 3);
    assert!(
        matches!(def(&alter_type.options[0]).arg.as_deref(), Some(Node::Integer(value)) if value.ival == 8)
    );
    assert!(matches!(
        def(&alter_type.options[1]).arg.as_deref(),
        Some(Node::TypeName(_))
    ));
    assert!(def(&alter_type.options[2]).arg.is_none());

    let collation = parse_node!(
        "alter collation app.en_us refresh version",
        AlterCollationStmt
    );
    assert_eq!(collation.collname.len(), 2);

    let policy = parse_node!(
        "alter policy tenant_policy on app.orders to analyst, current_user using (tenant_id = 7) with check (amount > 0)",
        AlterPolicyStmt
    );
    assert_eq!(policy.policy_name.as_deref(), Some("tenant_policy"));
    assert_eq!(
        policy
            .table
            .as_deref()
            .and_then(|table| table.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(policy.roles.len(), 2);
    assert!(
        policy
            .roles
            .iter()
            .all(|role| matches!(role, Node::RoleSpec(_)))
    );
    assert!(policy.qual.is_some());
    assert!(policy.with_check.is_some());
}

#[test]
fn alter_domain_populates_defaults_constraints_and_behavior() {
    let default = parse_node!(
        "alter domain app.positive_int set default 1 + 2",
        AlterDomainStmt
    );
    assert_eq!(default.subtype, AlterDomainType::AlterDefault);
    assert!(default.def.is_some());

    let add = parse_node!(
        "alter domain app.positive_int add constraint positive check (value > 0) not valid no inherit",
        AlterDomainStmt
    );
    assert_eq!(add.subtype, AlterDomainType::AddConstraint);
    let constraint = expect_node!(add.def.as_deref(), Some(Constraint));
    assert_eq!(constraint.contype, ConstrType::Check);
    assert_eq!(constraint.conname.as_deref(), Some("positive"));
    assert!(constraint.skip_validation);
    assert!(!constraint.initially_valid);
    assert!(constraint.is_no_inherit);

    let drop = parse_node!(
        "alter domain app.positive_int drop constraint if exists positive cascade",
        AlterDomainStmt
    );
    assert_eq!(drop.subtype, AlterDomainType::DropConstraint);
    assert_eq!(drop.type_name.len(), 2);
    assert_eq!(drop.name.as_deref(), Some("positive"));
    assert!(drop.missing_ok);
    assert_eq!(drop.behavior, DropBehavior::Cascade);
}

#[test]
fn alter_property_graph_populates_table_label_and_property_actions() {
    let add = parse_node!(
        "alter property graph social add vertex tables (users as u key (id) label person properties (name as display_name)) add edge tables (follows as f source u destination u no properties)",
        AlterPropGraphStmt
    );
    assert_eq!(
        add.pgname
            .as_deref()
            .and_then(|name| name.relname.as_deref()),
        Some("social")
    );
    assert_eq!(add.add_vertex_tables.len(), 1);
    assert_eq!(add.add_edge_tables.len(), 1);
    let vertex = expect_node!(&add.add_vertex_tables[0], PropGraphVertex);
    assert_eq!(vertex.vkey.len(), 1);
    assert_eq!(vertex.labels.len(), 1);
    let edge = expect_node!(&add.add_edge_tables[0], PropGraphEdge);
    assert_eq!(
        edge.etable
            .as_deref()
            .and_then(|table| table.relname.as_deref()),
        Some("follows")
    );
    assert_eq!(edge.esrcvertex.as_deref(), Some("u"));
    assert_eq!(edge.edestvertex.as_deref(), Some("u"));

    let labels = parse_node!(
        "alter property graph social alter vertex table u add label employee properties (salary as pay) add label active no properties",
        AlterPropGraphStmt
    );
    assert_eq!(labels.element_kind, AlterPropGraphElementKind::Vertex);
    assert_eq!(labels.element_alias.as_deref(), Some("u"));
    assert_eq!(labels.add_labels.len(), 2);

    let properties = parse_node!(
        "alter property graph social alter edge table f alter label follows drop properties (weight, since) cascade",
        AlterPropGraphStmt
    );
    assert_eq!(properties.element_kind, AlterPropGraphElementKind::Edge);
    assert_eq!(properties.alter_label.as_deref(), Some("follows"));
    assert_eq!(properties.drop_properties.len(), 2);
    assert_eq!(properties.drop_behavior, DropBehavior::Cascade);

    let drop_tables = parse_node!(
        "alter property graph social drop vertex tables (u, company) cascade",
        AlterPropGraphStmt
    );
    assert_eq!(drop_tables.drop_vertex_tables.len(), 2);
    assert!(drop_tables.drop_edge_tables.is_empty());
    assert_eq!(drop_tables.drop_behavior, DropBehavior::Cascade);

    let drop_edges = parse_node!(
        "alter property graph social drop edge tables (f) restrict",
        AlterPropGraphStmt
    );
    assert_eq!(drop_edges.drop_edge_tables.len(), 1);
    assert!(drop_edges.drop_vertex_tables.is_empty());

    let drop_label = parse_node!(
        "alter property graph social alter vertex table u drop label employee restrict",
        AlterPropGraphStmt
    );
    assert_eq!(drop_label.drop_label.as_deref(), Some("employee"));
    assert_eq!(drop_label.drop_behavior, DropBehavior::Restrict);

    let sql = "alter property graph social alter edge table f alter label follows add properties (weight as strength)";
    let add_properties = parse_node!(sql, AlterPropGraphStmt);
    let properties = add_properties
        .add_properties
        .as_deref()
        .expect("ADD PROPERTIES payload");
    assert_eq!(properties.properties.len(), 1);
    assert_eq!(
        properties.location,
        sql.find("add properties").unwrap() as i32
    );
}

#[test]
fn alter_composite_type_builds_complete_alter_table_commands() {
    let sql = "alter type app.address add attribute zip text collate c cascade, drop attribute if exists legacy restrict, alter attribute city set data type varchar collate c cascade";
    let stmt = parse_node!(sql, AlterTableStmt);
    assert_eq!(stmt.objtype, ObjectType::Type);
    assert_eq!(stmt.cmds.len(), 3);
    assert_eq!(stmt.relation.as_deref().expect("relation").location, 11);

    let add = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(add.subtype, AlterTableType::AddColumn);
    assert!(!add.recurse);
    assert_eq!(add.behavior, DropBehavior::Cascade);
    let column = expect_node!(add.def.as_deref(), Some(ColumnDef));
    assert_eq!(column.colname.as_deref(), Some("zip"));
    assert!(column.type_name.is_some());
    assert!(column.coll_clause.is_some());

    let drop = expect_node!(&stmt.cmds[1], AlterTableCmd);
    assert_eq!(drop.subtype, AlterTableType::DropColumn);
    assert_eq!(drop.name.as_deref(), Some("legacy"));
    assert!(drop.missing_ok);

    let alter = expect_node!(&stmt.cmds[2], AlterTableCmd);
    assert_eq!(alter.subtype, AlterTableType::AlterColumnType);
    let column = expect_node!(alter.def.as_deref(), Some(ColumnDef));
    assert!(column.colname.is_none());
    assert!(column.type_name.is_some());
    assert!(column.coll_clause.is_some());
    assert_eq!(column.location, sql.find("city").unwrap() as i32);
}

#[test]
fn alter_extension_populates_update_and_member_objects() {
    let update = parse_node!(
        "alter extension hstore update to '2.0' to next_version",
        AlterExtensionStmt
    );
    assert_eq!(update.extname.as_deref(), Some("hstore"));
    assert_eq!(
        def(&update.options[0]).defname.as_deref(),
        Some("new_version")
    );
    assert_eq!(update.options.len(), 2);
    assert_eq!(def(&update.options[0]).location, 30);
    assert_eq!(def(&update.options[1]).location, 39);

    let function = parse_node!(
        "alter extension toolkit add function app.normalize(text)",
        AlterExtensionContentsStmt
    );
    assert_eq!(function.action, 1);
    assert_eq!(function.objtype, ObjectType::Function);
    assert!(matches!(
        function.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));

    let cast = parse_node!(
        "alter extension toolkit drop cast (int as text)",
        AlterExtensionContentsStmt
    );
    assert_eq!(cast.action, -1);
    assert_eq!(cast.objtype, ObjectType::Cast);
    assert!(
        matches!(cast.object.as_deref(), Some(Node::AArrayExpr(list)) if list.elements.len() == 2)
    );

    let opclass = parse_node!(
        "alter extension toolkit add operator class app.text_ops using btree",
        AlterExtensionContentsStmt
    );
    assert_eq!(opclass.objtype, ObjectType::Opclass);
    assert!(
        matches!(opclass.object.as_deref(), Some(Node::AArrayExpr(list)) if list.elements.len() == 3)
    );

    let aggregate = parse_node!(
        "alter extension toolkit add aggregate app.percentile(float8 order by float8)",
        AlterExtensionContentsStmt
    );
    assert_eq!(aggregate.objtype, ObjectType::Aggregate);
    assert!(matches!(
        aggregate.object.as_deref(),
        Some(Node::ObjectWithArgs(signature))
            if signature.objargs.len() == 2 && signature.objfuncargs.len() == 2
    ));

    let schema = parse_node!(
        "alter extension toolkit add schema app",
        AlterExtensionContentsStmt
    );
    assert_eq!(schema.objtype, ObjectType::Schema);
    assert!(
        matches!(schema.object.as_deref(), Some(Node::String(name)) if name.sval.as_deref() == Some("app"))
    );
}

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

#[test]
fn alter_text_search_dictionary_and_configuration_populate_mapping_fields() {
    let dictionary = parse_node!(
        "alter text search dictionary app.english (stopwords = 'english', accept = false)",
        AlterTsDictionaryStmt
    );
    assert_eq!(dictionary.dictname.len(), 2);
    assert_eq!(dictionary.options.len(), 2);

    let add = parse_node!(
        "alter text search configuration app.english add mapping for asciiword, word with app.simple, public.english_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(add.cfgname.len(), 2);
    assert_eq!(add.kind, AlterTsConfigType::AddMapping);
    assert_eq!(add.tokentype.len(), 2);
    assert_eq!(add.dicts.len(), 2);
    assert!(!add.override_);
    assert!(!add.replace);

    let replace = parse_node!(
        "alter text search configuration app.english alter mapping for word replace public.english_stem with app.custom_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(replace.kind, AlterTsConfigType::ReplaceDictForToken);
    assert_eq!(replace.tokentype.len(), 1);
    assert_eq!(replace.dicts.len(), 2);
    assert!(replace.replace);

    let override_mapping = parse_node!(
        "alter text search configuration app.english alter mapping for word with app.simple",
        AlterTsConfigurationStmt
    );
    assert_eq!(
        override_mapping.kind,
        AlterTsConfigType::AlterMappingForToken
    );
    assert!(override_mapping.override_);
    assert!(!override_mapping.replace);

    let replace_all = parse_node!(
        "alter text search configuration app.english alter mapping replace public.english_stem with app.custom_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(replace_all.kind, AlterTsConfigType::ReplaceDict);
    assert!(replace_all.tokentype.is_empty());
    assert_eq!(replace_all.dicts.len(), 2);
    assert!(replace_all.replace);

    let drop = parse_node!(
        "alter text search configuration app.english drop mapping if exists for email, url",
        AlterTsConfigurationStmt
    );
    assert_eq!(drop.kind, AlterTsConfigType::DropMapping);
    assert!(drop.missing_ok);
    assert_eq!(drop.tokentype.len(), 2);
}

#[test]
fn rename_stmt_preserves_relation_object_and_subobject_identities() {
    let column = parse_node!(
        "alter table if exists app.orders rename column total to amount",
        RenameStmt
    );
    assert_eq!(column.rename_type, ObjectType::Column);
    assert_eq!(column.relation_type, ObjectType::Table);
    assert_eq!(
        column
            .relation
            .as_deref()
            .and_then(|rel| rel.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(column.subname.as_deref(), Some("total"));
    assert_eq!(column.newname.as_deref(), Some("amount"));
    assert!(column.missing_ok);

    let constraint = parse_node!(
        "alter domain app.positive rename constraint positive_check to positive_value_check",
        RenameStmt
    );
    assert_eq!(constraint.rename_type, ObjectType::Domconstraint);
    assert!(
        matches!(constraint.object.as_deref(), Some(Node::AArrayExpr(names)) if names.elements.len() == 2)
    );
    assert_eq!(constraint.subname.as_deref(), Some("positive_check"));

    let function = parse_node!(
        "alter function app.calculate(int) rename to compute",
        RenameStmt
    );
    assert_eq!(function.rename_type, ObjectType::Function);
    assert!(matches!(
        function.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));
    assert_eq!(function.newname.as_deref(), Some("compute"));

    let aggregate = parse_node!(
        "alter aggregate app.percentile(float8 order by float8) rename to pctl",
        RenameStmt
    );
    assert_eq!(aggregate.rename_type, ObjectType::Aggregate);
    assert!(matches!(
        aggregate.object.as_deref(),
        Some(Node::ObjectWithArgs(signature)) if signature.objargs.len() == 2
    ));

    let opclass = parse_node!(
        "alter operator class app.text_ops using btree rename to text_ops_v2",
        RenameStmt
    );
    assert_eq!(opclass.rename_type, ObjectType::Opclass);
    assert!(
        matches!(opclass.object.as_deref(), Some(Node::AArrayExpr(names)) if names.elements.len() == 3)
    );

    let quoted = parse_node!(
        "alter table items rename column \"select\" to \"from\"",
        RenameStmt
    );
    assert_eq!(quoted.subname.as_deref(), Some("select"));
    assert_eq!(quoted.newname.as_deref(), Some("from"));

    let attribute = parse_node!(
        "alter type app.composite rename attribute old_name to new_name cascade",
        RenameStmt
    );
    assert_eq!(attribute.rename_type, ObjectType::Attribute);
    assert_eq!(attribute.relation_type, ObjectType::Type);
    assert_eq!(attribute.behavior, DropBehavior::Cascade);
    assert_eq!(
        attribute
            .relation
            .as_deref()
            .map(|relation| relation.location),
        Some(11)
    );

    for (sql, inherited) in [
        (
            "alter table only app.orders rename to archived_orders",
            false,
        ),
        ("alter table app.orders * rename to archived_orders", true),
        (
            "alter foreign table only app.foreign_orders rename to archived_orders",
            false,
        ),
    ] {
        let stmt = parse_node!(sql, RenameStmt);
        assert_eq!(
            stmt.relation.as_deref().map(|relation| relation.inh),
            Some(inherited)
        );
    }
}

#[test]
fn alter_object_names_follow_colid_categories() {
    let database = parse_node!(
        "alter database \"select\" set tablespace \"from\"",
        AlterDatabaseStmt
    );
    assert_eq!(database.dbname.as_deref(), Some("select"));
    assert!(matches!(
        database.options.as_slice(),
        [Node::DefElem(option)]
            if option.defname.as_deref() == Some("tablespace")
                && matches!(option.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("from"))
    ));

    let policy = parse_node!(
        "alter policy \"select\" on items using (true)",
        AlterPolicyStmt
    );
    assert_eq!(policy.policy_name.as_deref(), Some("select"));

    let fdw = parse_node!(
        "alter foreign data wrapper \"select\" no handler",
        AlterFdwStmt
    );
    assert_eq!(fdw.fdwname.as_deref(), Some("select"));
}

#[test]
fn alter_depends_schema_and_owner_populate_the_correct_identity_field() {
    let function = parse_node!(
        "alter function app.calculate(int) no depends on extension toolkit",
        AlterObjectDependsStmt
    );
    assert_eq!(function.object_type, ObjectType::Function);
    assert!(function.remove);
    assert!(matches!(
        function.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));
    assert_eq!(
        function
            .extname
            .as_deref()
            .and_then(|name| name.sval.as_deref()),
        Some("toolkit")
    );

    let trigger = parse_node!(
        "alter trigger audit on app.orders depends on extension audit_ext",
        AlterObjectDependsStmt
    );
    assert_eq!(trigger.object_type, ObjectType::Trigger);
    assert!(trigger.relation.is_some());
    assert!(
        matches!(trigger.object.as_deref(), Some(Node::AArrayExpr(names)) if names.elements.len() == 1)
    );

    let function_schema = parse_node!(
        "alter procedure app.refresh(int) set schema maintenance",
        AlterObjectSchemaStmt
    );
    assert_eq!(function_schema.object_type, ObjectType::Procedure);
    assert!(matches!(
        function_schema.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));
    assert_eq!(function_schema.newschema.as_deref(), Some("maintenance"));

    let table_schema = parse_node!(
        "alter table if exists app.orders set schema archive",
        AlterObjectSchemaStmt
    );
    assert!(table_schema.relation.is_some());
    assert!(table_schema.object.is_none());
    assert!(table_schema.missing_ok);

    for (sql, object_type, inherited) in [
        (
            "alter table only (app.orders) set schema archive",
            ObjectType::Table,
            false,
        ),
        (
            "alter foreign table app.foreign_orders * set schema archive",
            ObjectType::ForeignTable,
            true,
        ),
    ] {
        let stmt = parse_node!(sql, AlterObjectSchemaStmt);
        assert_eq!(stmt.object_type, object_type);
        assert_eq!(
            stmt.relation.as_deref().map(|relation| relation.inh),
            Some(inherited)
        );
    }

    let owner = parse_node!(
        "alter property graph app.social owner to graph_admin",
        AlterOwnerStmt
    );
    assert_eq!(owner.object_type, ObjectType::Propgraph);
    assert!(owner.relation.is_some());
    assert!(owner.object.is_none());
    assert_eq!(
        owner
            .newowner
            .as_deref()
            .and_then(|role| role.rolename.as_deref()),
        Some("graph_admin")
    );

    let public_owner = parse_node!("alter schema app owner to public", AlterOwnerStmt);
    assert!(matches!(
        public_owner.newowner.as_deref(),
        Some(role) if role.roletype == pg_parser::RoleSpecType::Public && role.rolename.is_none()
    ));

    let table_owner = parse_node!("alter table app.orders owner to app_owner", AlterTableStmt);
    let cmd = expect_node!(&table_owner.cmds[0], AlterTableCmd);
    assert_eq!(cmd.subtype, AlterTableType::ChangeOwner);
    assert_eq!(
        cmd.newowner
            .as_deref()
            .and_then(|role| role.rolename.as_deref()),
        Some("app_owner")
    );
}

#[test]
fn alter_table_commands_preserve_fields_without_skipping_tokens() {
    let stmt = parse_node!(
        "alter table app.items add column score integer, alter column score set default 1 + 2, alter column score set not null, alter column score set (n_distinct = 10), drop column if exists obsolete cascade",
        AlterTableStmt
    );
    assert_eq!(stmt.cmds.len(), 5);

    let add = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(add.subtype, AlterTableType::AddColumn);
    let column = expect_node!(add.def.as_deref(), Some(ColumnDef));
    assert_eq!(column.colname.as_deref(), Some("score"));
    assert!(column.type_name.is_some());

    let default = expect_node!(&stmt.cmds[1], AlterTableCmd);
    assert_eq!(default.subtype, AlterTableType::ColumnDefault);
    assert_eq!(default.name.as_deref(), Some("score"));
    assert!(matches!(default.def.as_deref(), Some(Node::AExpr(_))));

    let not_null = expect_node!(&stmt.cmds[2], AlterTableCmd);
    assert_eq!(not_null.subtype, AlterTableType::SetNotNull);
    assert_eq!(not_null.name.as_deref(), Some("score"));

    let options = expect_node!(&stmt.cmds[3], AlterTableCmd);
    assert_eq!(options.subtype, AlterTableType::SetOptions);
    assert!(matches!(
        options.def.as_deref(),
        Some(Node::AArrayExpr(items)) if items.elements.len() == 1
    ));

    let drop = expect_node!(&stmt.cmds[4], AlterTableCmd);
    assert_eq!(drop.subtype, AlterTableType::DropColumn);
    assert_eq!(drop.name.as_deref(), Some("obsolete"));
    assert!(drop.missing_ok);
    assert_eq!(drop.behavior, DropBehavior::Cascade);
}

#[test]
fn alter_table_command_names_follow_colid_categories() {
    let stmt = parse_node!(
        "alter table items drop column \"select\", enable trigger \"from\", set tablespace \"where\"",
        AlterTableStmt
    );
    assert!(matches!(
        stmt.cmds.as_slice(),
        [Node::AlterTableCmd(drop), Node::AlterTableCmd(trigger), Node::AlterTableCmd(tablespace)]
            if drop.name.as_deref() == Some("select")
                && trigger.name.as_deref() == Some("from")
                && tablespace.name.as_deref() == Some("where")
    ));
}

#[test]
fn alter_column_type_and_statistics_preserve_raw_payloads() {
    let stmt = parse_node!(
        "alter table items alter column payload type text collate pg_catalog.c using payload::text, alter 2 set statistics -10",
        AlterTableStmt
    );

    let change_type = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(change_type.subtype, AlterTableType::AlterColumnType);
    assert_eq!(change_type.name.as_deref(), Some("payload"));
    let column = expect_node!(change_type.def.as_deref(), Some(ColumnDef));
    assert!(column.type_name.is_some());
    assert_eq!(
        column
            .coll_clause
            .as_deref()
            .map(|collation| collation.collname.len()),
        Some(2)
    );
    assert!(column.raw_default.is_some());

    let statistics = expect_node!(&stmt.cmds[1], AlterTableCmd);
    assert_eq!(statistics.subtype, AlterTableType::SetStatistics);
    assert_eq!(statistics.num, 2);
    assert!(matches!(
        statistics.def.as_deref(),
        Some(Node::Integer(value)) if value.ival == -10
    ));
}

#[test]
fn alter_table_add_and_alter_constraint_use_complete_raw_nodes() {
    let stmt = parse_node!(
        "alter table orders add constraint positive_amount check (amount > 0) not valid, add column code text constraint code_not_null not null, alter constraint orders_fk deferrable initially deferred not enforced, alter constraint positive_amount no inherit",
        AlterTableStmt
    );
    assert_eq!(stmt.cmds.len(), 4);

    let add_constraint = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(add_constraint.subtype, AlterTableType::AddConstraint);
    let check = expect_node!(add_constraint.def.as_deref(), Some(Constraint));
    assert_eq!(check.contype, ConstrType::Check);
    assert_eq!(check.conname.as_deref(), Some("positive_amount"));
    assert!(check.raw_expr.is_some());
    assert!(check.skip_validation);

    let add_column = expect_node!(&stmt.cmds[1], AlterTableCmd);
    assert_eq!(add_column.subtype, AlterTableType::AddColumn);
    let column = expect_node!(add_column.def.as_deref(), Some(ColumnDef));
    assert_eq!(column.colname.as_deref(), Some("code"));
    assert!(matches!(
        &column.constraints[0],
        Node::Constraint(c)
            if c.contype == ConstrType::Notnull
                && c.conname.as_deref() == Some("code_not_null")
    ));

    let alter_fk = expect_node!(&stmt.cmds[2], AlterTableCmd);
    assert_eq!(alter_fk.subtype, AlterTableType::AlterConstraint);
    let altered = expect_node!(alter_fk.def.as_deref(), Some(AtAlterConstraint));
    assert_eq!(altered.conname.as_deref(), Some("orders_fk"));
    assert!(altered.alter_deferrability);
    assert!(altered.deferrable);
    assert!(altered.initdeferred);
    assert!(altered.alter_enforceability);
    assert!(!altered.is_enforced);

    let alter_inherit = expect_node!(&stmt.cmds[3], AlterTableCmd);
    let altered = expect_node!(alter_inherit.def.as_deref(), Some(AtAlterConstraint));
    assert!(altered.alter_inheritability);
    assert!(altered.noinherit);
}

#[test]
fn alter_table_identity_commands_preserve_constraint_and_option_nodes() {
    let stmt = parse_node!(
        "alter table items alter column id add generated by default as identity (start with 10), alter column id set generated always, alter column id restart with 100, alter column id drop identity if exists",
        AlterTableStmt
    );
    assert_eq!(stmt.cmds.len(), 4);

    let add = expect_node!(&stmt.cmds[0], AlterTableCmd);
    assert_eq!(add.subtype, AlterTableType::AddIdentity);
    let identity = expect_node!(add.def.as_deref(), Some(Constraint));
    assert_eq!(identity.contype, ConstrType::Identity);
    assert_eq!(identity.generated_when, b'd');
    assert_eq!(identity.options.len(), 1);

    let generated = expect_node!(&stmt.cmds[1], AlterTableCmd);
    assert_eq!(generated.subtype, AlterTableType::SetIdentity);
    assert!(matches!(
        generated.def.as_deref(),
        Some(Node::AArrayExpr(_))
    ));

    let restart = expect_node!(&stmt.cmds[2], AlterTableCmd);
    assert_eq!(restart.subtype, AlterTableType::SetIdentity);
    assert!(matches!(
        restart.def.as_deref(),
        Some(Node::AArrayExpr(options)) if options.elements.len() == 1
    ));

    let drop = expect_node!(&stmt.cmds[3], AlterTableCmd);
    assert_eq!(drop.subtype, AlterTableType::DropIdentity);
    assert!(drop.missing_ok);
}
