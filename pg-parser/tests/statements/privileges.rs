use pg_parser::{DropBehavior, GrantTargetType, Node, ObjectType, RoleSpecType};

use super::common::{expect_node, parse_node};

#[test]
fn grant_stmt_populates_privileges_target_grantees_option_and_grantor() {
    let stmt = parse_node!(
        "grant select(id), update(name) on table app.items to alice, group analysts with grant option granted by admin",
        GrantStmt
    );
    assert!(stmt.is_grant);
    assert_eq!(stmt.targtype, GrantTargetType::Object);
    assert_eq!(stmt.objtype, ObjectType::Table);
    assert_eq!(stmt.objects.len(), 1);
    assert!(matches!(stmt.objects.as_slice(), [Node::RangeVar(_)]));
    assert_eq!(stmt.privileges.len(), 2);
    assert_eq!(stmt.grantees.len(), 2);
    assert!(stmt.grant_option);
    assert!(stmt.grantor.is_some());

    let select = expect_node!(&stmt.privileges[0], AccessPriv);
    assert_eq!(select.priv_name.as_deref(), Some("select"));
    assert_eq!(select.cols.len(), 1);

    let quoted = parse_node!(
        "grant \"from\"(\"select\") on table app.items to alice",
        GrantStmt
    );
    assert!(matches!(
        quoted.privileges.as_slice(),
        [Node::AccessPriv(privilege)]
            if privilege.priv_name.as_deref() == Some("from") && privilege.cols.len() == 1
    ));

    let all_columns = parse_node!(
        "grant all privileges (id, name) on table app.items to alice",
        GrantStmt
    );
    assert!(matches!(
        all_columns.privileges.as_slice(),
        [Node::AccessPriv(privilege)]
            if privilege.priv_name.is_none() && privilege.cols.len() == 2
    ));

    let alter_system = parse_node!(
        "grant alter system on parameter work_mem to admin",
        GrantStmt
    );
    assert!(matches!(
        alter_system.privileges.as_slice(),
        [Node::AccessPriv(privilege)]
            if privilege.priv_name.as_deref() == Some("alter system")
                && privilege.cols.is_empty()
    ));
}

#[test]
fn grant_role_spec_locations_follow_each_role_token() {
    let sql = "grant select on table t to alice, current_role, current_user, session_user, public";
    let stmt = parse_node!(sql, GrantStmt);
    let expected = [
        ("alice", RoleSpecType::Cstring),
        ("current_role", RoleSpecType::CurrentRole),
        ("current_user", RoleSpecType::CurrentUser),
        ("session_user", RoleSpecType::SessionUser),
        ("public", RoleSpecType::Public),
    ];
    assert_eq!(stmt.grantees.len(), expected.len());
    for (node, (token, role_type)) in stmt.grantees.iter().zip(expected) {
        let role = expect_node!(node, RoleSpec);
        assert_eq!(role.roletype, role_type);
        assert_eq!(role.location as usize, sql.find(token).unwrap());
    }
}

#[test]
fn grant_targets_preserve_object_family_specific_raw_nodes() {
    for (sql, expected_type) in [
        ("grant usage on table app.items to alice", ObjectType::Table),
        (
            "grant usage on sequence app.seq to alice",
            ObjectType::Sequence,
        ),
        (
            "grant usage on property graph app.social to alice",
            ObjectType::Propgraph,
        ),
    ] {
        let stmt = parse_node!(sql, GrantStmt);
        assert_eq!(stmt.objtype, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::RangeVar(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        (
            "grant usage on foreign data wrapper fdw1 to alice",
            ObjectType::Fdw,
        ),
        (
            "grant usage on foreign server srv1 to alice",
            ObjectType::ForeignServer,
        ),
        (
            "grant connect on database appdb to alice",
            ObjectType::Database,
        ),
        (
            "grant usage on language plpgsql to alice",
            ObjectType::Language,
        ),
        ("grant usage on schema app to alice", ObjectType::Schema),
        (
            "grant create on tablespace fast_space to alice",
            ObjectType::Tablespace,
        ),
    ] {
        let stmt = parse_node!(sql, GrantStmt);
        assert_eq!(stmt.objtype, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::String(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        (
            "grant execute on function app.f(int) to alice",
            ObjectType::Function,
        ),
        (
            "grant execute on procedure app.p(int) to alice",
            ObjectType::Procedure,
        ),
        (
            "grant execute on routine app.r(int) to alice",
            ObjectType::Routine,
        ),
    ] {
        let stmt = parse_node!(sql, GrantStmt);
        assert_eq!(stmt.objtype, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::ObjectWithArgs(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        ("grant usage on domain app.d to alice", ObjectType::Domain),
        ("grant usage on type app.t to alice", ObjectType::Type),
    ] {
        let stmt = parse_node!(sql, GrantStmt);
        assert_eq!(stmt.objtype, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::AArrayExpr(_)]),
            "{sql}"
        );
    }

    let large_objects = parse_node!("grant select on large object 42, -7 to alice", GrantStmt);
    assert_eq!(large_objects.objtype, ObjectType::Largeobject);
    assert!(matches!(
        large_objects.objects.as_slice(),
        [Node::Integer(first), Node::Integer(second)] if first.ival == 42 && second.ival == -7
    ));

    let parameters = parse_node!(
        "grant set on parameter app.work_mem, search_path to alice",
        GrantStmt
    );
    assert_eq!(parameters.objtype, ObjectType::ParameterAcl);
    assert!(matches!(
        parameters.objects.as_slice(),
        [Node::String(first), Node::String(second)]
            if first.sval.as_deref() == Some("app.work_mem")
                && second.sval.as_deref() == Some("search_path")
    ));

    let all_tables = parse_node!(
        "grant select on all tables in schema app, audit to alice",
        GrantStmt
    );
    assert_eq!(all_tables.targtype, GrantTargetType::AllInSchema);
    assert!(matches!(
        all_tables.objects.as_slice(),
        [Node::String(_), Node::String(_)]
    ));
}

#[test]
fn revoke_stmt_populates_grant_option_grantor_and_behavior() {
    let stmt = parse_node!(
        "revoke grant option for select on table app.items from public granted by admin cascade",
        GrantStmt
    );
    assert!(!stmt.is_grant);
    assert!(stmt.grant_option);
    assert!(stmt.grantor.is_some());
    assert_eq!(stmt.behavior, DropBehavior::Cascade);
    assert!(matches!(
        stmt.grantees.as_slice(),
        [Node::RoleSpec(role)]
            if role.roletype == pg_parser::RoleSpecType::Public && role.rolename.is_none()
    ));
}

#[test]
fn grant_role_stmt_populates_roles_options_and_grantor() {
    let stmt = parse_node!(
        "grant app_reader, app_writer to alice with admin option, inherit true granted by admin",
        GrantRoleStmt
    );
    assert!(stmt.is_grant);
    assert_eq!(stmt.granted_roles.len(), 2);
    assert_eq!(stmt.grantee_roles.len(), 1);
    assert_eq!(stmt.opt.len(), 2);
    assert!(stmt.grantor.is_some());
}

#[test]
fn revoke_role_stmt_populates_revoked_option_and_behavior() {
    let stmt = parse_node!(
        "revoke admin option for app_reader from alice cascade",
        GrantRoleStmt
    );
    assert!(!stmt.is_grant);
    assert_eq!(stmt.granted_roles.len(), 1);
    assert_eq!(stmt.grantee_roles.len(), 1);
    assert_eq!(stmt.opt.len(), 1);
    assert_eq!(stmt.behavior, DropBehavior::Cascade);

    let quoted = parse_node!(
        "revoke \"select\" option for app_reader from alice",
        GrantRoleStmt
    );
    assert!(matches!(
        quoted.opt.as_slice(),
        [Node::DefElem(option)] if option.defname.as_deref() == Some("select")
    ));
}

#[test]
fn alter_default_privileges_populates_scope_options_and_grant_action() {
    let stmt = parse_node!(
        "alter default privileges for role current_user, owner2 in schema app, audit grant select, update on tables to reader with grant option",
        AlterDefaultPrivilegesStmt
    );
    assert_eq!(stmt.options.len(), 2);
    assert!(matches!(
        stmt.options.as_slice(),
        [Node::DefElem(roles), Node::DefElem(schemas)]
            if roles.defname.as_deref() == Some("roles")
                && schemas.defname.as_deref() == Some("schemas")
    ));
    let roles = expect_node!(&stmt.options[0], DefElem);
    assert!(matches!(
        roles.arg.as_deref(),
        Some(Node::AArrayExpr(array))
            if matches!(array.elements.as_slice(), [Node::RoleSpec(_), Node::RoleSpec(_)])
    ));
    let schemas = expect_node!(&stmt.options[1], DefElem);
    assert!(matches!(
        schemas.arg.as_deref(),
        Some(Node::AArrayExpr(array))
            if matches!(array.elements.as_slice(), [Node::String(_), Node::String(_)])
    ));
    let action = stmt.action.as_deref().expect("GrantStmt action");
    assert!(action.is_grant);
    assert_eq!(action.targtype, GrantTargetType::Defaults);
    assert_eq!(action.objtype, ObjectType::Table);
    assert_eq!(action.privileges.len(), 2);
    assert!(action.grant_option);
    assert!(action.objects.is_empty());

    for (target, expected) in [
        ("functions", ObjectType::Function),
        ("routines", ObjectType::Function),
        ("sequences", ObjectType::Sequence),
        ("types", ObjectType::Type),
        ("schemas", ObjectType::Schema),
        ("large objects", ObjectType::Largeobject),
    ] {
        let sql = format!("alter default privileges grant usage on {target} to reader");
        let stmt = parse_node!(&sql, AlterDefaultPrivilegesStmt);
        assert_eq!(
            stmt.action.as_deref().expect("action").objtype,
            expected,
            "{sql}"
        );
    }

    let revoke = parse_node!(
        "alter default privileges revoke grant option for usage on types from reader cascade",
        AlterDefaultPrivilegesStmt
    );
    let action = revoke.action.as_deref().expect("revoke action");
    assert!(!action.is_grant);
    assert!(action.grant_option);
    assert_eq!(action.behavior, DropBehavior::Cascade);
}
