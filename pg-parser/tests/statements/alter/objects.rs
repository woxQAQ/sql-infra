use super::*;

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
