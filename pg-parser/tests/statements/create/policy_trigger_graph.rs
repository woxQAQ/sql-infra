use super::*;

#[test]
fn create_trigger_stmt_populates_transition_relations() {
    let stmt = parse_node!(
        "create trigger audit_changes after update on items referencing old table old_rows new table as new_rows for each statement execute function audit_items()",
        CreateTrigStmt
    );
    assert_eq!(stmt.transition_rels.len(), 2);
    let old = expect_node!(&stmt.transition_rels[0], TriggerTransition);
    assert!(!old.is_new);
    assert!(old.is_table);
    assert_eq!(old.name.as_deref(), Some("old_rows"));
    let new = expect_node!(&stmt.transition_rels[1], TriggerTransition);
    assert!(new.is_new);
    assert_eq!(new.name.as_deref(), Some("new_rows"));
}

#[test]
fn create_policy_trigger_constraint_trigger_and_event_trigger_are_complete() {
    let policy = parse_node!(
        "create policy restricted_items on app.items as restrictive for update to app_user, auditor using (owner_id = current_user) with check (active = true)",
        CreatePolicyStmt
    );
    assert!(!policy.permissive);
    assert_eq!(policy.policy_name.as_deref(), Some("restricted_items"));
    assert_eq!(
        policy
            .table
            .as_deref()
            .and_then(|table| table.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(policy.cmd_name.as_deref(), Some("update"));
    assert_eq!(policy.roles.len(), 2);
    assert!(policy.qual.is_some());
    assert!(policy.with_check.is_some());

    let default_policy = parse_node!("create policy visible_items on app.items", CreatePolicyStmt);
    assert!(default_policy.permissive);
    assert_eq!(default_policy.cmd_name.as_deref(), Some("all"));
    assert!(matches!(
        default_policy.roles.as_slice(),
        [Node::RoleSpec(role)]
            if role.roletype == pg_parser::RoleSpecType::Public
                && role.rolename.is_none()
                && role.location == -1
    ));

    let trigger = parse_node!(
        "create trigger audit_columns before update of name, status or insert on app.items for each row when (new.active is true) execute function app.audit_trigger(7, 1.5, plain, 'change')",
        CreateTrigStmt
    );
    assert!(!trigger.isconstraint);
    assert!(!trigger.replace);
    assert_eq!(
        trigger
            .relation
            .as_deref()
            .and_then(|relation| relation.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(trigger.timing, 2);
    assert_eq!(trigger.events, 20);
    assert_eq!(trigger.columns.len(), 2);
    assert!(matches!(
        trigger.args.as_slice(),
        [Node::String(integer), Node::String(float), Node::String(label), Node::String(string)]
            if integer.sval.as_deref() == Some("7")
                && float.sval.as_deref() == Some("1.5")
                && label.sval.as_deref() == Some("plain")
                && string.sval.as_deref() == Some("change")
    ));
    assert!(trigger.row);
    assert!(trigger.when_clause.is_some());

    let for_row = parse_node!(
        "create trigger row_without_each after insert on app.items for row execute function app.audit_trigger()",
        CreateTrigStmt
    );
    assert!(for_row.row);

    let constraint_trigger = parse_node!(
        "create constraint trigger check_parent after insert or update on app.children from app.parents deferrable initially deferred for each row execute function app.check_parent()",
        CreateTrigStmt
    );
    assert!(constraint_trigger.isconstraint);
    assert!(constraint_trigger.constrrel.is_some());
    assert!(constraint_trigger.deferrable);
    assert!(constraint_trigger.initdeferred);
    assert!(constraint_trigger.row);

    let enforced_constraint = parse_node!(
        "create constraint trigger enforced_parent after insert on app.children enforced for row execute function app.check_parent()",
        CreateTrigStmt
    );
    assert!(enforced_constraint.isconstraint);
    assert!(enforced_constraint.row);

    let implied_deferrable = parse_node!(
        "create constraint trigger deferred_parent after insert on app.children initially deferred for each row execute function app.check_parent()",
        CreateTrigStmt
    );
    assert!(implied_deferrable.deferrable);
    assert!(implied_deferrable.initdeferred);

    let event_trigger = parse_node!(
        "create event trigger ddl_audit on ddl_command_end when tag in ('CREATE TABLE', 'ALTER TABLE') and schema in ('public') execute function app.audit_ddl()",
        CreateEventTrigStmt
    );
    assert_eq!(event_trigger.trigname.as_deref(), Some("ddl_audit"));
    assert_eq!(event_trigger.eventname.as_deref(), Some("ddl_command_end"));
    assert_eq!(event_trigger.whenclause.len(), 2);
    assert_eq!(event_trigger.funcname.len(), 2);

    let no_when = parse_node!(
        "create event trigger ddl_simple on ddl_command_start execute procedure app.audit_ddl()",
        CreateEventTrigStmt
    );
    assert!(no_when.whenclause.is_empty());
    assert_eq!(no_when.funcname.len(), 2);
}

#[test]
fn create_property_graph_populates_keys_references_labels_and_properties() {
    let graph = parse_node!(
        "create property graph app.social vertex tables (app.users as u key (id) label person properties (name as display_name, age)) edge tables (app.follows as f key (id) source key (source_id) references u (id) destination key (target_id) references u (id) label follows properties all columns)",
        CreatePropGraphStmt
    );
    assert_eq!(
        graph
            .pgname
            .as_deref()
            .and_then(|name| name.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(graph.vertex_tables.len(), 1);
    assert_eq!(graph.edge_tables.len(), 1);

    let vertex = expect_node!(&graph.vertex_tables[0], PropGraphVertex);
    assert_eq!(
        vertex
            .vtable
            .as_deref()
            .and_then(|table| table.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("u")
    );
    assert_eq!(vertex.vkey.len(), 1);
    let label = expect_node!(&vertex.labels[0], PropGraphLabelAndProperties);
    assert_eq!(label.label.as_deref(), Some("person"));
    let properties: &PropGraphProperties = label.properties.as_deref().expect("properties");
    assert!(!properties.all);
    assert_eq!(
        label
            .properties
            .as_deref()
            .expect("properties")
            .properties
            .len(),
        2
    );

    let edge = expect_node!(&graph.edge_tables[0], PropGraphEdge);
    assert_eq!(edge.ekey.len(), 1);
    assert_eq!(edge.esrckey.len(), 1);
    assert_eq!(edge.esrcvertex.as_deref(), Some("u"));
    assert_eq!(edge.esrcvertexcols.len(), 1);
    assert_eq!(edge.edestkey.len(), 1);
    assert_eq!(edge.edestvertex.as_deref(), Some("u"));
    assert_eq!(edge.edestvertexcols.len(), 1);

    let default_graph = parse_node!(
        "create property graph g vertex tables (t)",
        CreatePropGraphStmt
    );
    let vertex = expect_node!(&default_graph.vertex_tables[0], PropGraphVertex);
    let defaults = expect_node!(&vertex.labels[0], PropGraphLabelAndProperties);
    assert_eq!(defaults.location, -1);
    assert_eq!(
        defaults.properties.as_deref().expect("properties").location,
        -1
    );

    let sql = "create property graph g vertex tables (t label item)";
    let labeled_graph = parse_node!(sql, CreatePropGraphStmt);
    let vertex = expect_node!(&labeled_graph.vertex_tables[0], PropGraphVertex);
    let label = expect_node!(&vertex.labels[0], PropGraphLabelAndProperties);
    assert_eq!(label.location, sql.find("label").unwrap() as i32);
    assert_eq!(
        label.properties.as_deref().expect("properties").location,
        -1
    );

    for (modifier, expected) in [
        ("temp", b't'),
        ("temporary", b't'),
        ("local temp", b't'),
        ("global temporary", b't'),
        ("unlogged", b'u'),
    ] {
        let graph = parse_node!(
            &format!("create {modifier} property graph g"),
            CreatePropGraphStmt
        );
        assert_eq!(
            graph.pgname.as_deref().map(|name| name.relpersistence),
            Some(expected)
        );
    }
}
