use super::*;

#[test]
fn lock_notify_listen_unlisten_and_load_require_and_store_arguments() {
    let lock = parse_node!(
        "lock table only app.items, app.other * in share row exclusive mode nowait",
        LockStmt
    );
    assert_eq!(lock.relations.len(), 2);
    assert_eq!(lock.mode, 6);
    assert!(lock.nowait);
    assert!(matches!(
        lock.relations.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if !first.inh && second.inh
    ));

    for (mode, expected) in [
        ("access share", 1),
        ("row share", 2),
        ("row exclusive", 3),
        ("share update exclusive", 4),
        ("share", 5),
        ("share row exclusive", 6),
        ("exclusive", 7),
        ("access exclusive", 8),
    ] {
        let sql = format!("lock app.items in {mode} mode");
        let lock = parse_node!(&sql, LockStmt);
        assert_eq!(lock.mode, expected, "{sql}");
        assert!(!lock.nowait, "{sql}");
    }
    let default_lock = parse_node!("lock app.items", LockStmt);
    assert_eq!(default_lock.mode, 8);

    let default_nowait = parse_node!(
        "lock table only (app.items), app.children * nowait",
        LockStmt
    );
    assert_eq!(default_nowait.mode, 8);
    assert!(default_nowait.nowait);
    assert!(matches!(
        default_nowait.relations.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)]
            if !first.inh
                && first.location == 17
                && second.inh
                && second.location == 29
    ));

    let listen = parse_node!("listen item_changes", ListenStmt);
    assert_eq!(listen.conditionname.as_deref(), Some("item_changes"));

    let notify = parse_node!("notify item_changes, 'updated'", NotifyStmt);
    assert_eq!(notify.conditionname.as_deref(), Some("item_changes"));
    assert_eq!(notify.payload.as_deref(), Some("updated"));

    let empty_notify = parse_node!("notify item_changes", NotifyStmt);
    assert!(empty_notify.payload.is_none());

    let unlisten = parse_node!("unlisten *", UnlistenStmt);
    assert!(unlisten.conditionname.is_none());

    let named_unlisten = parse_node!("unlisten item_changes", UnlistenStmt);
    assert_eq!(
        named_unlisten.conditionname.as_deref(),
        Some("item_changes")
    );

    let load = parse_node!("load 'extension.so'", LoadStmt);
    assert_eq!(load.filename.as_deref(), Some("extension.so"));
}

#[test]
fn import_do_return_and_wait_populate_all_clauses() {
    let import = parse_node!(
        "import foreign schema remote limit to (items, app.events) from server foreign_srv into staging options (case 'lower')",
        ImportForeignSchemaStmt
    );
    assert_eq!(import.remote_schema.as_deref(), Some("remote"));
    assert_eq!(import.server_name.as_deref(), Some("foreign_srv"));
    assert_eq!(import.local_schema.as_deref(), Some("staging"));
    assert_eq!(import.list_type, ImportForeignSchemaType::LimitTo);
    assert_eq!(import.table_list.len(), 2);
    assert!(matches!(
        import.table_list.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if first.inh && second.inh
    ));
    assert_eq!(import.options.len(), 1);

    let import = parse_node!(
        "import foreign schema remote except (only items, app.events *) from server foreign_srv into staging",
        ImportForeignSchemaStmt
    );
    assert_eq!(import.list_type, ImportForeignSchemaType::Except);
    assert!(matches!(
        import.table_list.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if !first.inh && second.inh
    ));

    let do_stmt = parse_node!("do language plpgsql 'begin perform 1; end'", DoStmt);
    assert_eq!(do_stmt.args.len(), 2);

    let language_only = parse_node!("do language plpgsql", DoStmt);
    assert!(matches!(
        language_only.args.as_slice(),
        [Node::DefElem(option)] if option.defname.as_deref() == Some("language")
    ));

    let wait = parse_node!("wait for lsn '0/16B6C50' with (timeout 1000)", WaitStmt);
    assert_eq!(wait.lsn_literal.as_deref(), Some("0/16B6C50"));
    assert_eq!(wait.options.len(), 1);

    let wait = parse_node!(
        "wait for lsn '0/16B6C51' with (timeout -1.5, polling true, mode 'strict', trace)",
        WaitStmt
    );
    assert_eq!(wait.options.len(), 4);
    assert!(matches!(
        def(&wait.options[0]).arg.as_deref(),
        Some(Node::Float(value)) if value.fval.as_deref() == Some("-1.5")
    ));
    assert!(matches!(
        def(&wait.options[1]).arg.as_deref(),
        Some(Node::String(value)) if value.sval.as_deref() == Some("true")
    ));
    assert!(def(&wait.options[3]).arg.is_none());

    let wait = parse_node!("wait for lsn '0/16B6C52'", WaitStmt);
    assert!(wait.options.is_empty());
}

#[test]
fn utility_statement_names_follow_colid_categories() {
    let setting = parse_node!("set \"select\" to 'value'", VariableSetStmt);
    assert_eq!(setting.name.as_deref(), Some("select"));

    let savepoint = parse_node!("savepoint \"select\"", TransactionStmt);
    assert_eq!(savepoint.savepoint_name.as_deref(), Some("select"));

    let prepared = parse_node!("prepare \"select\" as select 1", PrepareStmt);
    assert_eq!(prepared.name.as_deref(), Some("select"));

    let listen = parse_node!("listen \"select\"", ListenStmt);
    assert_eq!(listen.conditionname.as_deref(), Some("select"));

    let notify = parse_node!("notify \"select\", ''", NotifyStmt);
    assert_eq!(notify.conditionname.as_deref(), Some("select"));
    assert_eq!(notify.payload.as_deref(), Some(""));

    let unlisten = parse_node!("unlisten \"select\"", UnlistenStmt);
    assert_eq!(unlisten.conditionname.as_deref(), Some("select"));

    let cursor = parse_node!("declare \"select\" cursor for select 1", DeclareCursorStmt);
    assert_eq!(cursor.portalname.as_deref(), Some("select"));

    let import = parse_node!(
        "import foreign schema \"select\" from server \"from\" into \"where\"",
        ImportForeignSchemaStmt
    );
    assert_eq!(import.remote_schema.as_deref(), Some("select"));
    assert_eq!(import.server_name.as_deref(), Some("from"));
    assert_eq!(import.local_schema.as_deref(), Some("where"));
}
