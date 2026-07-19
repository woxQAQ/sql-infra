use super::*;

#[test]
fn set_constraints_preserves_qualified_names_and_mode() {
    let stmt = parse_node!(
        "set constraints app.orders_fk, local_fk deferred",
        ConstraintsSetStmt
    );
    assert!(stmt.deferred);
    assert!(matches!(
        stmt.constraints.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)]
            if first.schemaname.as_deref() == Some("app")
                && first.relname.as_deref() == Some("orders_fk")
                && first.inh
                && second.schemaname.is_none()
                && second.relname.as_deref() == Some("local_fk")
                && second.inh
    ));

    let all = parse_node!("set constraints all immediate", ConstraintsSetStmt);
    assert!(!all.deferred);
    assert!(all.constraints.is_empty());

    let qualified = parse_node!(
        "set constraints catalog.app.orders_fk immediate",
        ConstraintsSetStmt
    );
    assert!(matches!(
        qualified.constraints.as_slice(),
        [Node::RangeVar(name)]
            if name.catalogname.as_deref() == Some("catalog")
                && name.schemaname.as_deref() == Some("app")
                && name.relname.as_deref() == Some("orders_fk")
    ));
}

#[test]
fn do_stmt_preserves_flexible_option_order() {
    let code_first = parse_node!("do 'begin null; end' language plpgsql", DoStmt);
    assert_eq!(code_first.args.len(), 2);
    assert_eq!(def(&code_first.args[0]).defname.as_deref(), Some("as"));
    assert_eq!(
        def(&code_first.args[1]).defname.as_deref(),
        Some("language")
    );

    let language_first = parse_node!("do language 'plpgsql' 'begin null; end'", DoStmt);
    assert_eq!(language_first.args.len(), 2);
    assert_eq!(
        def(&language_first.args[0]).defname.as_deref(),
        Some("language")
    );
    assert_eq!(def(&language_first.args[1]).defname.as_deref(), Some("as"));

    let repeated = parse_node!(
        "do 'first' language sql 'second' language 'plpgsql'",
        DoStmt
    );
    assert_eq!(repeated.args.len(), 4);
    assert_eq!(def(&repeated.args[0]).location, 3);
    assert_eq!(def(&repeated.args[1]).location, 11);
    assert_eq!(def(&repeated.args[2]).location, 24);
    assert_eq!(def(&repeated.args[3]).location, 33);
}

#[test]
fn show_and_transaction_statements_populate_special_names_modes_and_identifiers() {
    let timezone = parse_node!(
        "set time zone interval '02:30' hour to minute",
        VariableSetStmt
    );
    assert_eq!(timezone.kind, VariableSetKind::SetValue);
    assert_eq!(timezone.name.as_deref(), Some("timezone"));
    assert!(matches!(timezone.args.as_slice(), [Node::TypeCast(_)]));

    let show = parse_node!("show transaction isolation level", VariableShowStmt);
    assert_eq!(show.name.as_deref(), Some("transaction_isolation"));

    let begin = parse_node!(
        "begin transaction isolation level repeatable read, read only not deferrable",
        TransactionStmt
    );
    assert_eq!(begin.kind, TransactionStmtKind::Begin);
    assert_eq!(begin.options.len(), 3);
    assert_eq!(begin.location, -1);
    let isolation = expect_node!(&begin.options[0], DefElem);
    assert_eq!(
        isolation.location,
        "begin transaction isolation level repeatable read, read only not deferrable"
            .find("isolation")
            .unwrap() as i32
    );
    assert!(matches!(
        isolation.arg.as_deref(),
        Some(Node::AConst(value))
            if value.location
                == "begin transaction isolation level repeatable read, read only not deferrable"
                    .find("repeatable")
                    .unwrap() as i32
    ));

    let commit = parse_node!("commit work and chain", TransactionStmt);
    assert_eq!(commit.kind, TransactionStmtKind::Commit);
    assert!(commit.chain);
    assert_eq!(commit.location, -1);

    let rollback_sql = "rollback transaction to savepoint s1";
    let rollback = parse_node!(rollback_sql, TransactionStmt);
    assert_eq!(rollback.kind, TransactionStmtKind::RollbackTo);
    assert_eq!(rollback.savepoint_name.as_deref(), Some("s1"));
    assert_eq!(rollback.location, rollback_sql.find("s1").unwrap() as i32);

    let prepare_sql = "prepare transaction 'gid-1'";
    let prepare = parse_node!(prepare_sql, TransactionStmt);
    assert_eq!(
        prepare.location,
        prepare_sql.find("'gid-1'").unwrap() as i32
    );
    assert_eq!(prepare.kind, TransactionStmtKind::Prepare);
    assert_eq!(prepare.gid.as_deref(), Some("gid-1"));

    let commit_prepared = parse_node!("commit prepared 'gid-1'", TransactionStmt);
    assert_eq!(commit_prepared.kind, TransactionStmtKind::CommitPrepared);
    assert_eq!(commit_prepared.gid.as_deref(), Some("gid-1"));

    let start = parse_node!(
        "start transaction isolation level read uncommitted read write deferrable",
        TransactionStmt
    );
    assert_eq!(start.kind, TransactionStmtKind::Start);
    assert_eq!(start.options.len(), 3);

    for (level, expected) in [
        ("read uncommitted", "read uncommitted"),
        ("read committed", "read committed"),
        ("repeatable read", "repeatable read"),
        ("serializable", "serializable"),
    ] {
        let sql = format!("begin isolation level {level}");
        let stmt = parse_node!(&sql, TransactionStmt);
        let [Node::DefElem(option)] = stmt.options.as_slice() else {
            panic!("expected one transaction option for {sql}");
        };
        assert_eq!(option.defname.as_deref(), Some("transaction_isolation"));
        assert!(matches!(
            option.arg.as_deref(),
            Some(Node::AConst(value))
                if matches!(&value.val, ValUnion::String(value) if value.sval.as_deref() == Some(expected))
        ));
    }

    for (mode, name, expected) in [
        ("read only", "transaction_read_only", 1),
        ("read write", "transaction_read_only", 0),
        ("deferrable", "transaction_deferrable", 1),
        ("not deferrable", "transaction_deferrable", 0),
    ] {
        let sql = format!("start transaction {mode}");
        let stmt = parse_node!(&sql, TransactionStmt);
        let [Node::DefElem(option)] = stmt.options.as_slice() else {
            panic!("expected one transaction option for {sql}");
        };
        assert_eq!(option.defname.as_deref(), Some(name));
        assert!(matches!(
            option.arg.as_deref(),
            Some(Node::AConst(value))
                if matches!(&value.val, ValUnion::Integer(value) if value.ival == expected)
        ));
    }

    for (sql, expected_kind, chain) in [
        (
            "abort transaction and chain",
            TransactionStmtKind::Rollback,
            true,
        ),
        ("end work and no chain", TransactionStmtKind::Commit, false),
        ("rollback and chain", TransactionStmtKind::Rollback, true),
    ] {
        let stmt = parse_node!(sql, TransactionStmt);
        assert_eq!(stmt.kind, expected_kind, "{sql}");
        assert_eq!(stmt.chain, chain, "{sql}");
        assert_eq!(stmt.location, -1, "{sql}");
    }

    for (sql, expected_kind, name) in [
        (
            "savepoint point_a",
            TransactionStmtKind::Savepoint,
            "point_a",
        ),
        ("release point_a", TransactionStmtKind::Release, "point_a"),
        (
            "release savepoint point_a",
            TransactionStmtKind::Release,
            "point_a",
        ),
        (
            "rollback work to point_a",
            TransactionStmtKind::RollbackTo,
            "point_a",
        ),
    ] {
        let stmt = parse_node!(sql, TransactionStmt);
        assert_eq!(stmt.kind, expected_kind, "{sql}");
        assert_eq!(stmt.savepoint_name.as_deref(), Some(name), "{sql}");
        assert_eq!(stmt.location, sql.find(name).unwrap() as i32, "{sql}");
    }

    let rollback_prepared_sql = "rollback prepared 'gid-2'";
    let rollback_prepared = parse_node!(rollback_prepared_sql, TransactionStmt);
    assert_eq!(
        rollback_prepared.kind,
        TransactionStmtKind::RollbackPrepared
    );
    assert_eq!(rollback_prepared.gid.as_deref(), Some("gid-2"));
    assert_eq!(
        rollback_prepared.location,
        rollback_prepared_sql.find("'gid-2'").unwrap() as i32
    );
}

#[test]
fn variable_set_locations_follow_postgres_set_rest_productions() {
    let cases = [
        ("set transaction isolation level serializable", -1),
        ("set session characteristics as transaction read only", -1),
        ("set work_mem to '4MB'", 16),
        ("set work_mem = null", 15),
        ("set work_mem to default", -1),
        ("set work_mem from current", -1),
        ("set time zone 'UTC'", -1),
        ("set schema 'app'", 11),
        ("set names 'UTF8'", 10),
        ("set names default", 10),
        ("set names", -1),
        ("set role app_user", 9),
        ("set session authorization app_user", 26),
        ("set session authorization default", -1),
        ("set xml option document", -1),
        ("set transaction snapshot 'x'", 25),
        ("reset all", -1),
        ("reset work_mem", -1),
        ("reset time zone", -1),
        ("reset transaction isolation level", -1),
        ("reset session authorization", -1),
    ];
    for (sql, location) in cases {
        let stmt = parse_node!(sql, VariableSetStmt);
        assert_eq!(stmt.location, location, "{sql}");
    }

    let xml = parse_node!("set xml option document", VariableSetStmt);
    assert!(matches!(
        xml.args.as_slice(),
        [Node::AConst(value)] if value.location == 15
    ));
}

#[test]
fn variable_set_reset_and_show_follow_special_value_grammars() {
    for (sql, name, expected_kind, is_local) in [
        (
            "set local app.work_mem to '4MB', 8, on",
            "app.work_mem",
            VariableSetKind::SetValue,
            true,
        ),
        (
            "set session app.work_mem from current",
            "app.work_mem",
            VariableSetKind::SetCurrent,
            false,
        ),
        (
            "set names 'UTF8'",
            "client_encoding",
            VariableSetKind::SetValue,
            false,
        ),
        (
            "set names default",
            "client_encoding",
            VariableSetKind::SetDefault,
            false,
        ),
        (
            "set role 'app_user'",
            "role",
            VariableSetKind::SetValue,
            false,
        ),
        (
            "set session authorization default",
            "session_authorization",
            VariableSetKind::SetDefault,
            false,
        ),
        (
            "set time zone local",
            "timezone",
            VariableSetKind::SetDefault,
            false,
        ),
        (
            "set xml option content",
            "xmloption",
            VariableSetKind::SetValue,
            false,
        ),
    ] {
        let stmt = parse_node!(sql, VariableSetStmt);
        assert_eq!(stmt.name.as_deref(), Some(name), "{sql}");
        assert_eq!(stmt.kind, expected_kind, "{sql}");
        assert_eq!(stmt.is_local, is_local, "{sql}");
    }

    let values = parse_node!("set local app.work_mem to '4MB', 8, on", VariableSetStmt);
    assert_eq!(values.args.len(), 3);

    let reset = parse_node!("reset app.work_mem", VariableSetStmt);
    assert_eq!(reset.name.as_deref(), Some("app.work_mem"));
    assert_eq!(reset.kind, VariableSetKind::Reset);

    let show = parse_node!("show app.work_mem", VariableShowStmt);
    assert_eq!(show.name.as_deref(), Some("app.work_mem"));

    let show_all = parse_node!("show all", VariableShowStmt);
    assert_eq!(show_all.name.as_deref(), Some("all"));
}
