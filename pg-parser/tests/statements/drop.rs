use pg_parser::{DropBehavior, Node, ObjectType, RoleSpecType};

use super::common::parse_statement;

#[test]
fn drop_cast_transform_and_operator_family_preserve_object_identities() {
    let Node::DropStmt(cast) =
        parse_statement("drop cast if exists (app.source_type as text) cascade")
    else {
        panic!("expected DropStmt");
    };
    assert_eq!(cast.remove_type, ObjectType::Cast);
    assert!(cast.missing_ok);
    assert_eq!(cast.behavior, DropBehavior::Cascade);
    assert!(matches!(&cast.objects[0], Node::AArrayExpr(types) if types.elements.len() == 2));

    let Node::DropStmt(transform) =
        parse_statement("drop transform if exists for app.currency language sql restrict")
    else {
        panic!("expected DropStmt");
    };
    assert_eq!(transform.remove_type, ObjectType::Transform);
    assert!(matches!(&transform.objects[0], Node::AArrayExpr(parts) if parts.elements.len() == 2));

    let Node::DropStmt(opclass) =
        parse_statement("drop operator class if exists app.text_ops using btree cascade")
    else {
        panic!("expected DropStmt");
    };
    assert_eq!(opclass.remove_type, ObjectType::Opclass);
    assert!(opclass.missing_ok);
    assert!(matches!(&opclass.objects[0], Node::AArrayExpr(parts) if parts.elements.len() == 3));
}

#[test]
fn drop_index_concurrently_is_recorded_in_postgresql_token_order() {
    let Node::DropStmt(index) = parse_statement("drop index concurrently if exists app.orders_idx")
    else {
        panic!("expected DropStmt");
    };
    assert_eq!(index.remove_type, ObjectType::Index);
    assert!(index.concurrent);
    assert!(index.missing_ok);
}

#[test]
fn drop_function_distinguishes_unspecified_and_empty_argument_lists() {
    let Node::DropStmt(stmt) =
        parse_statement("drop function app.unspecified, app.parameterless()")
    else {
        panic!("expected DropStmt");
    };
    assert!(matches!(
        stmt.objects.as_slice(),
        [Node::ObjectWithArgs(unspecified), Node::ObjectWithArgs(parameterless)]
            if unspecified.args_unspecified
                && unspecified.objargs.is_empty()
                && unspecified.objfuncargs.is_empty()
                && !parameterless.args_unspecified
                && parameterless.objargs.is_empty()
                && parameterless.objfuncargs.is_empty()
    ));
}

#[test]
fn drop_unary_operator_preserves_the_missing_argument_position() {
    let Node::DropStmt(stmt) = parse_statement("drop operator if exists -(none, integer)") else {
        panic!("expected DropStmt");
    };
    assert_eq!(stmt.remove_type, ObjectType::Operator);
    assert!(matches!(
        stmt.objects.as_slice(),
        [Node::ObjectWithArgs(operator)]
            if !operator.args_unspecified
                && operator.objfuncargs.is_empty()
                && matches!(operator.objargs.as_slice(), [None, Some(Node::TypeName(_))])
    ));
}

#[test]
fn drop_aggregate_preserves_star_and_ordered_argument_signatures() {
    let Node::DropStmt(stmt) =
        parse_statement("drop aggregate app.count_rows(*), app.percentile(float8 order by float8)")
    else {
        panic!("expected DropStmt");
    };
    assert_eq!(stmt.remove_type, ObjectType::Aggregate);
    assert!(matches!(
        stmt.objects.as_slice(),
        [Node::ObjectWithArgs(star), Node::ObjectWithArgs(ordered)]
            if !star.args_unspecified
                && star.objargs.is_empty()
                && star.objfuncargs.is_empty()
                && ordered.objargs.len() == 2
                && ordered.objfuncargs.len() == 2
    ));
}

#[test]
fn generic_drop_preserves_object_family_specific_identity_nodes() {
    for (sql, expected_type) in [
        ("drop schema app", ObjectType::Schema),
        ("drop extension ext", ObjectType::Extension),
        ("drop access method heap", ObjectType::AccessMethod),
        ("drop event trigger ddl_event", ObjectType::EventTrigger),
        ("drop foreign data wrapper fdw1", ObjectType::Fdw),
        ("drop procedural language plpgsql", ObjectType::Language),
        ("drop publication pub1", ObjectType::Publication),
        ("drop server srv1", ObjectType::ForeignServer),
    ] {
        let Node::DropStmt(stmt) = parse_statement(sql) else {
            panic!("expected DropStmt for {sql}");
        };
        assert_eq!(stmt.remove_type, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::String(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        ("drop table app.items", ObjectType::Table),
        ("drop property graph app.social", ObjectType::Propgraph),
        ("drop text search parser app.parser", ObjectType::Tsparser),
        (
            "drop text search dictionary app.dictionary",
            ObjectType::Tsdictionary,
        ),
        (
            "drop text search template app.template",
            ObjectType::Tstemplate,
        ),
        (
            "drop text search configuration app.config",
            ObjectType::Tsconfiguration,
        ),
    ] {
        let Node::DropStmt(stmt) = parse_statement(sql) else {
            panic!("expected DropStmt for {sql}");
        };
        assert_eq!(stmt.remove_type, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::AArrayExpr(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        ("drop type app.item_type[]", ObjectType::Type),
        ("drop domain app.positive_int", ObjectType::Domain),
    ] {
        let Node::DropStmt(stmt) = parse_statement(sql) else {
            panic!("expected DropStmt for {sql}");
        };
        assert_eq!(stmt.remove_type, expected_type, "{sql}");
        assert!(
            matches!(stmt.objects.as_slice(), [Node::TypeName(_)]),
            "{sql}"
        );
    }

    for (sql, expected_type) in [
        (
            "drop trigger if exists tr on app.items cascade",
            ObjectType::Trigger,
        ),
        ("drop rule if exists r on app.items", ObjectType::Rule),
        ("drop policy if exists p on app.items", ObjectType::Policy),
    ] {
        let Node::DropStmt(stmt) = parse_statement(sql) else {
            panic!("expected DropStmt for {sql}");
        };
        assert_eq!(stmt.remove_type, expected_type, "{sql}");
        assert!(stmt.missing_ok, "{sql}");
        assert!(matches!(
            stmt.objects.as_slice(),
            [Node::AArrayExpr(identity)] if identity.elements.len() == 3
        ));
    }
}

#[test]
fn dedicated_drop_statements_require_and_store_all_names() {
    let Node::DropdbStmt(database) = parse_statement("drop database if exists \"select\"") else {
        panic!("expected DropdbStmt");
    };
    assert_eq!(database.dbname.as_deref(), Some("select"));
    assert!(database.missing_ok);

    for sql in [
        "drop database analytics (force)",
        "drop database if exists analytics with (force)",
    ] {
        let Node::DropdbStmt(database) = parse_statement(sql) else {
            panic!("expected DropdbStmt for {sql}");
        };
        assert!(matches!(
            database.options.as_slice(),
            [Node::DefElem(option)]
                if option.defname.as_deref() == Some("force") && option.arg.is_none()
        ));
        let expected_location = sql.find("force").unwrap() as i32;
        let Node::DefElem(option) = &database.options[0] else {
            unreachable!()
        };
        assert_eq!(option.location, expected_location, "{sql}");
    }

    let Node::DropRoleStmt(roles) = parse_statement("drop role if exists alice, current_user")
    else {
        panic!("expected DropRoleStmt");
    };
    assert!(roles.missing_ok);
    assert!(matches!(
        roles.roles.as_slice(),
        [Node::RoleSpec(alice), Node::RoleSpec(current)]
            if alice.rolename.as_deref() == Some("alice")
                && current.roletype == RoleSpecType::CurrentUser
    ));

    let Node::DropOwnedStmt(owned) = parse_statement("drop owned by alice, public cascade") else {
        panic!("expected DropOwnedStmt");
    };
    assert_eq!(owned.behavior, DropBehavior::Cascade);
    assert!(matches!(
        owned.roles.as_slice(),
        [Node::RoleSpec(alice), Node::RoleSpec(public)]
            if alice.rolename.as_deref() == Some("alice")
                && public.roletype == RoleSpecType::Public
    ));

    let Node::DropTableSpaceStmt(tablespace) =
        parse_statement("drop tablespace if exists fast_space")
    else {
        panic!("expected DropTableSpaceStmt");
    };
    assert_eq!(tablespace.tablespacename.as_deref(), Some("fast_space"));
    assert!(tablespace.missing_ok);

    let Node::DropTableSpaceStmt(quoted_tablespace) = parse_statement("drop tablespace \"select\"")
    else {
        panic!("expected DropTableSpaceStmt");
    };
    assert_eq!(quoted_tablespace.tablespacename.as_deref(), Some("select"));

    let Node::DropSubscriptionStmt(subscription) =
        parse_statement("drop subscription if exists logical_sub cascade")
    else {
        panic!("expected DropSubscriptionStmt");
    };
    assert_eq!(subscription.subname.as_deref(), Some("logical_sub"));
    assert_eq!(subscription.behavior, DropBehavior::Cascade);

    let Node::DropUserMappingStmt(mapping) =
        parse_statement("drop user mapping if exists for user server remote")
    else {
        panic!("expected DropUserMappingStmt");
    };
    assert_eq!(
        mapping.user.as_deref().map(|user| user.roletype),
        Some(RoleSpecType::CurrentUser)
    );
    assert_eq!(mapping.servername.as_deref(), Some("remote"));
    assert!(mapping.missing_ok);
}
