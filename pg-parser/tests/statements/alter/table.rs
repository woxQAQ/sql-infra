use super::*;

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
