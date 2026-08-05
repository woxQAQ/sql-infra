use pg_parser::CURSOR_OPT_ASENSITIVE;
use pg_parser::CURSOR_OPT_BINARY;
use pg_parser::CURSOR_OPT_FAST_PLAN;
use pg_parser::CURSOR_OPT_HOLD;
use pg_parser::CURSOR_OPT_INSENSITIVE;
use pg_parser::CURSOR_OPT_NO_SCROLL;
use pg_parser::CURSOR_OPT_SCROLL;
use pg_parser::DefElem;
use pg_parser::DiscardMode;
use pg_parser::DropBehavior;
use pg_parser::FetchDirection;
use pg_parser::FetchDirectionKeywords;
use pg_parser::ImportForeignSchemaType;
use pg_parser::Node;
use pg_parser::ObjectType;
use pg_parser::ReindexObjectType;
use pg_parser::RepackCommand;
use pg_parser::TransactionStmtKind;
use pg_parser::ValUnion;
use pg_parser::VariableSetKind;

use super::common::parse_statement;

#[test]
fn declare_cursor_accepts_the_grammar_valid_empty_select_query() {
    let Node::DeclareCursorStmt(stmt) = parse_statement("declare c cursor for select") else {
        panic!("expected DeclareCursorStmt");
    };
    assert!(matches!(
        stmt.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));
}

#[test]
fn prepare_and_explain_accept_the_grammar_valid_empty_select() {
    let Node::PrepareStmt(prepare) = parse_statement("prepare empty_plan as select") else {
        panic!("expected PrepareStmt");
    };
    assert!(matches!(
        prepare.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));

    let Node::ExplainStmt(explain) = parse_statement("explain select") else {
        panic!("expected ExplainStmt");
    };
    assert!(matches!(
        explain.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));
}

fn def(node: &Node) -> &DefElem {
    let Node::DefElem(definition) = node else {
        panic!("expected DefElem");
    };
    definition
}

#[test]
fn copy_stmt_populates_relation_query_columns_program_options_and_filter() {
    let Node::CopyStmt(table_copy) = parse_statement(
        "copy app.items (id, name) from program 'cat data.csv' with (format csv, header true, delimiter ',') where id > 0",
    ) else {
        panic!("expected CopyStmt");
    };
    assert!(table_copy.relation.is_some());
    assert!(table_copy.query.is_none());
    assert_eq!(table_copy.attlist.len(), 2);
    assert!(table_copy.is_from);
    assert!(table_copy.is_program);
    assert_eq!(table_copy.filename.as_deref(), Some("cat data.csv"));
    assert_eq!(table_copy.options.len(), 3);
    assert!(table_copy.where_clause.is_some());

    let Node::CopyStmt(query_copy) =
        parse_statement("copy (select id from app.items) to stdout with (format csv)")
    else {
        panic!("expected CopyStmt");
    };
    assert!(query_copy.relation.is_none());
    assert!(matches!(
        query_copy.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));
    assert!(!query_copy.is_from);
    assert!(query_copy.filename.is_none());
    assert_eq!(query_copy.options.len(), 1);

    for (sql, expected) in [
        ("copy (insert into dst values (1)) to stdout", "insert"),
        (
            "copy (update dst set value = 1 returning value) to stdout",
            "update",
        ),
        ("copy (delete from dst returning value) to stdout", "delete"),
        (
            "copy (merge into dst using src on dst.id = src.id when matched then do nothing) to stdout",
            "merge",
        ),
    ] {
        let Node::CopyStmt(copy) = parse_statement(sql) else {
            panic!("expected CopyStmt for {sql}");
        };
        let matches_expected = matches!(
            (expected, copy.query.as_deref()),
            ("insert", Some(Node::InsertStmt(_)))
                | ("update", Some(Node::UpdateStmt(_)))
                | ("delete", Some(Node::DeleteStmt(_)))
                | ("merge", Some(Node::MergeStmt(_)))
        );
        assert!(matches_expected, "{sql}");
    }
}

#[test]
fn copy_generic_options_preserve_every_raw_argument_shape() {
    let Node::CopyStmt(copy) = parse_statement(
        "copy app.items to stdout with (header, format csv, freeze true, reject_limit 12, null default, force_quote *, force_not_null (id, name))",
    ) else {
        panic!("expected CopyStmt");
    };
    assert_eq!(copy.options.len(), 7);
    assert!(def(&copy.options[0]).arg.is_none());
    assert!(matches!(
        def(&copy.options[3]).arg.as_deref(),
        Some(Node::Integer(value)) if value.ival == 12
    ));
    assert!(matches!(
        def(&copy.options[4]).arg.as_deref(),
        Some(Node::String(value)) if value.sval.as_deref() == Some("default")
    ));
    assert!(matches!(
        def(&copy.options[5]).arg.as_deref(),
        Some(Node::AStar(_))
    ));
    assert!(matches!(
        def(&copy.options[6]).arg.as_deref(),
        Some(Node::AArrayExpr(values)) if values.elements.len() == 2
    ));

    let Node::CopyStmt(legacy) =
        parse_statement("copy binary app.items from stdin using delimiters '|' with null as 'N'")
    else {
        panic!("expected legacy CopyStmt");
    };
    assert_eq!(legacy.options.len(), 3);
    assert_eq!(def(&legacy.options[0]).defname.as_deref(), Some("format"));
    assert_eq!(
        def(&legacy.options[1]).defname.as_deref(),
        Some("delimiter")
    );
    assert_eq!(def(&legacy.options[2]).defname.as_deref(), Some("null"));

    let Node::CopyStmt(force) = parse_statement(
        "copy app.items to stdout csv force quote id, name force not null * force null archived_at encoding 'UTF8'",
    ) else {
        panic!("expected old-style CopyStmt");
    };
    assert_eq!(force.options.len(), 5);
    assert!(matches!(
        def(&force.options[1]).arg.as_deref(),
        Some(Node::AArrayExpr(columns)) if columns.elements.len() == 2
    ));

    let Node::CopyStmt(literal_columns) =
        parse_statement("copy app.items to stdout force null 1, 2.5, 'legacy_name'")
    else {
        panic!("expected legacy CopyStmt");
    };
    assert!(matches!(
        def(&literal_columns.options[0]).arg.as_deref(),
        Some(Node::AArrayExpr(columns))
            if matches!(columns.elements.as_slice(),
                [Node::String(one), Node::String(two), Node::String(name)]
                if one.sval.as_deref() == Some("1")
                    && two.sval.as_deref() == Some("2.5")
                    && name.sval.as_deref() == Some("legacy_name"))
    ));
    assert!(matches!(
        def(&force.options[2]).arg.as_deref(),
        Some(Node::AStar(_))
    ));
    assert!(matches!(
        def(&force.options[3]).arg.as_deref(),
        Some(Node::AArrayExpr(columns)) if columns.elements.len() == 1
    ));
}

#[test]
fn call_stmt_preserves_the_raw_function_call_only() {
    let Node::CallStmt(stmt) = parse_statement("call app.process_order(42, urgent => true)") else {
        panic!("expected CallStmt");
    };
    let call = stmt.funccall.as_deref().expect("raw FuncCall");
    assert_eq!(call.funcname.len(), 2);
    assert_eq!(call.args.len(), 2);
    assert!(
        matches!(call.args.get(1), Some(Node::NamedArgExpr(arg)) if arg.name.as_deref() == Some("urgent"))
    );
    assert!(stmt.funcexpr.is_none());
    assert!(stmt.outargs.is_empty());

    let Node::CallStmt(ordered) = parse_statement("call app.collect(1 order by 2 desc)") else {
        panic!("expected ordered CallStmt");
    };
    assert_eq!(
        ordered
            .funccall
            .as_deref()
            .expect("ordered FuncCall")
            .agg_order
            .len(),
        1
    );

    let Node::CallStmt(variadic) = parse_statement("call app.collect(1, variadic values)") else {
        panic!("expected variadic CallStmt");
    };
    assert!(
        variadic
            .funccall
            .as_deref()
            .expect("variadic FuncCall")
            .func_variadic
    );

    let Node::CallStmt(distinct) = parse_statement("call app.collect(distinct value)") else {
        panic!("expected distinct CallStmt");
    };
    assert!(
        distinct
            .funccall
            .as_deref()
            .expect("distinct FuncCall")
            .agg_distinct
    );

    let Node::CallStmt(all) = parse_statement("call app.collect(all value order by value)") else {
        panic!("expected ALL CallStmt");
    };
    let all = all.funccall.as_deref().expect("ALL FuncCall");
    assert!(!all.agg_distinct);
    assert_eq!(all.args.len(), 1);
    assert_eq!(all.agg_order.len(), 1);

    let Node::CallStmt(star) = parse_statement("call app.collect(*)") else {
        panic!("expected star CallStmt");
    };
    assert!(star.funccall.as_deref().expect("star FuncCall").agg_star);
}

#[test]
fn set_constraints_preserves_qualified_names_and_mode() {
    let Node::ConstraintsSetStmt(stmt) =
        parse_statement("set constraints app.orders_fk, local_fk deferred")
    else {
        panic!("expected ConstraintsSetStmt");
    };
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

    let Node::ConstraintsSetStmt(all) = parse_statement("set constraints all immediate") else {
        panic!("expected ALL ConstraintsSetStmt");
    };
    assert!(!all.deferred);
    assert!(all.constraints.is_empty());

    let Node::ConstraintsSetStmt(qualified) =
        parse_statement("set constraints catalog.app.orders_fk immediate")
    else {
        panic!("expected three-part ConstraintsSetStmt");
    };
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
    let Node::DoStmt(code_first) = parse_statement("do 'begin null; end' language plpgsql") else {
        panic!("expected DoStmt");
    };
    assert_eq!(code_first.args.len(), 2);
    assert_eq!(def(&code_first.args[0]).defname.as_deref(), Some("as"));
    assert_eq!(
        def(&code_first.args[1]).defname.as_deref(),
        Some("language")
    );

    let Node::DoStmt(language_first) = parse_statement("do language 'plpgsql' 'begin null; end'")
    else {
        panic!("expected DoStmt");
    };
    assert_eq!(language_first.args.len(), 2);
    assert_eq!(
        def(&language_first.args[0]).defname.as_deref(),
        Some("language")
    );
    assert_eq!(def(&language_first.args[1]).defname.as_deref(), Some("as"));

    let Node::DoStmt(repeated) =
        parse_statement("do 'first' language sql 'second' language 'plpgsql'")
    else {
        panic!("expected repeated-option DoStmt");
    };
    assert_eq!(repeated.args.len(), 4);
    assert_eq!(def(&repeated.args[0]).location, 3);
    assert_eq!(def(&repeated.args[1]).location, 11);
    assert_eq!(def(&repeated.args[2]).location, 24);
    assert_eq!(def(&repeated.args[3]).location, 33);
}

#[test]
fn vacuum_and_analyze_populate_options_relations_and_columns() {
    let Node::VacuumStmt(vacuum) =
        parse_statement("vacuum (full true, analyze true) app.items(id, name), app.other")
    else {
        panic!("expected VacuumStmt");
    };
    assert!(vacuum.is_vacuumcmd);
    assert_eq!(vacuum.options.len(), 2);
    assert_eq!(vacuum.rels.len(), 2);
    let Node::VacuumRelation(first) = &vacuum.rels[0] else {
        panic!("expected VacuumRelation");
    };
    assert_eq!(first.va_cols.len(), 2);
    assert!(first.relation.as_deref().expect("relation").inh);

    let Node::VacuumStmt(legacy) =
        parse_statement("vacuum full freeze verbose analyze only app.items, app.other *")
    else {
        panic!("expected VacuumStmt");
    };
    assert_eq!(legacy.options.len(), 4);
    assert!(legacy.options.iter().all(|option| matches!(
        option,
        Node::DefElem(definition) if definition.arg.is_none()
    )));
    assert!(matches!(
        legacy.rels.as_slice(),
        [Node::VacuumRelation(first), Node::VacuumRelation(second)]
            if !first.relation.as_deref().expect("relation").inh
                && second.relation.as_deref().expect("relation").inh
    ));

    let Node::VacuumStmt(analyze) = parse_statement("analyze verbose app.items(id)") else {
        panic!("expected VacuumStmt");
    };
    assert!(!analyze.is_vacuumcmd);
    assert_eq!(analyze.options.len(), 1);
    assert_eq!(analyze.rels.len(), 1);

    let Node::VacuumStmt(british) =
        parse_statement("analyse (verbose, buffer_usage_limit '8MB', sample_rate 0.25) app.items")
    else {
        panic!("expected VacuumStmt");
    };
    assert!(!british.is_vacuumcmd);
    assert_eq!(british.options.len(), 3);
    assert!(matches!(
        british.options.as_slice(),
        [Node::DefElem(verbose), Node::DefElem(buffer), Node::DefElem(rate)]
            if verbose.arg.is_none()
                && matches!(buffer.arg.as_deref(), Some(Node::String(_)))
                && matches!(rate.arg.as_deref(), Some(Node::Float(_)))
    ));
}

#[test]
fn explain_checkpoint_and_discard_populate_utility_options() {
    let Node::ExplainStmt(explain) =
        parse_statement("explain (analyze true, verbose true) select * from app.items")
    else {
        panic!("expected ExplainStmt");
    };
    assert_eq!(explain.options.len(), 2);
    assert!(matches!(
        explain.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    for (sql, expected_tag) in [
        (
            "explain analyse verbose execute prepared_query(1)",
            pg_parser::NodeTag::ExecuteStmt,
        ),
        (
            "explain declare c cursor for select 1",
            pg_parser::NodeTag::DeclareCursorStmt,
        ),
        (
            "explain create table explained as select 1",
            pg_parser::NodeTag::CreateTableAsStmt,
        ),
        (
            "explain create materialized view explained_mv as select 1",
            pg_parser::NodeTag::CreateTableAsStmt,
        ),
        (
            "explain refresh materialized view mv",
            pg_parser::NodeTag::RefreshMatViewStmt,
        ),
        (
            "explain insert into t values (1)",
            pg_parser::NodeTag::InsertStmt,
        ),
        (
            "explain update t set id = 1",
            pg_parser::NodeTag::UpdateStmt,
        ),
        ("explain delete from t", pg_parser::NodeTag::DeleteStmt),
        (
            "explain merge into t using s on t.id = s.id when matched then do nothing",
            pg_parser::NodeTag::MergeStmt,
        ),
    ] {
        let Node::ExplainStmt(explain) = parse_statement(sql) else {
            panic!("expected ExplainStmt for {sql}");
        };
        assert_eq!(
            explain.query.as_deref().map(Node::tag),
            Some(expected_tag),
            "{sql}"
        );
    }

    let Node::CheckPointStmt(bare_checkpoint) = parse_statement("checkpoint") else {
        panic!("expected CheckPointStmt");
    };
    assert!(bare_checkpoint.options.is_empty());

    let Node::CheckPointStmt(checkpoint) = parse_statement("checkpoint (fast true)") else {
        panic!("expected CheckPointStmt");
    };
    assert_eq!(checkpoint.options.len(), 1);

    for (sql, expected) in [
        ("discard all", DiscardMode::All),
        ("discard plans", DiscardMode::Plans),
        ("discard sequences", DiscardMode::Sequences),
        ("discard temp", DiscardMode::Temp),
        ("discard temporary", DiscardMode::Temp),
    ] {
        let Node::DiscardStmt(discard) = parse_statement(sql) else {
            panic!("expected DiscardStmt for {sql}");
        };
        assert_eq!(discard.target, expected, "{sql}");
    }
}

#[test]
fn lock_notify_listen_unlisten_and_load_require_and_store_arguments() {
    let Node::LockStmt(lock) = parse_statement(
        "lock table only app.items, app.other * in share row exclusive mode nowait",
    ) else {
        panic!("expected LockStmt");
    };
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
        let Node::LockStmt(lock) = parse_statement(&sql) else {
            panic!("expected LockStmt for {sql}");
        };
        assert_eq!(lock.mode, expected, "{sql}");
        assert!(!lock.nowait, "{sql}");
    }
    let Node::LockStmt(default_lock) = parse_statement("lock app.items") else {
        panic!("expected LockStmt");
    };
    assert_eq!(default_lock.mode, 8);

    let Node::LockStmt(default_nowait) =
        parse_statement("lock table only (app.items), app.children * nowait")
    else {
        panic!("expected LockStmt");
    };
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

    let Node::ListenStmt(listen) = parse_statement("listen item_changes") else {
        panic!("expected ListenStmt");
    };
    assert_eq!(listen.conditionname.as_deref(), Some("item_changes"));

    let Node::NotifyStmt(notify) = parse_statement("notify item_changes, 'updated'") else {
        panic!("expected NotifyStmt");
    };
    assert_eq!(notify.conditionname.as_deref(), Some("item_changes"));
    assert_eq!(notify.payload.as_deref(), Some("updated"));

    let Node::NotifyStmt(empty_notify) = parse_statement("notify item_changes") else {
        panic!("expected NotifyStmt");
    };
    assert!(empty_notify.payload.is_none());

    let Node::UnlistenStmt(unlisten) = parse_statement("unlisten *") else {
        panic!("expected UnlistenStmt");
    };
    assert!(unlisten.conditionname.is_none());

    let Node::UnlistenStmt(named_unlisten) = parse_statement("unlisten item_changes") else {
        panic!("expected UnlistenStmt");
    };
    assert_eq!(
        named_unlisten.conditionname.as_deref(),
        Some("item_changes")
    );

    let Node::LoadStmt(load) = parse_statement("load 'extension.so'") else {
        panic!("expected LoadStmt");
    };
    assert_eq!(load.filename.as_deref(), Some("extension.so"));
}

#[test]
fn refresh_reindex_and_truncate_populate_all_raw_fields() {
    let Node::RefreshMatViewStmt(refresh) =
        parse_statement("refresh materialized view concurrently app.summary with no data")
    else {
        panic!("expected RefreshMatViewStmt");
    };
    assert!(refresh.concurrent);
    assert!(refresh.skip_data);
    assert!(refresh.relation.is_some());

    for (sql, concurrent, skip_data) in [
        ("refresh materialized view app.summary", false, false),
        (
            "refresh materialized view app.summary with data",
            false,
            false,
        ),
        (
            "refresh materialized view concurrently app.summary with no data",
            true,
            true,
        ),
    ] {
        let Node::RefreshMatViewStmt(stmt) = parse_statement(sql) else {
            panic!("expected RefreshMatViewStmt for {sql}");
        };
        assert_eq!(stmt.concurrent, concurrent, "{sql}");
        assert_eq!(stmt.skip_data, skip_data, "{sql}");
    }

    let Node::ReindexStmt(reindex) =
        parse_statement("reindex (verbose true) table concurrently app.items")
    else {
        panic!("expected ReindexStmt");
    };
    assert_eq!(reindex.kind, ReindexObjectType::Table);
    assert!(reindex.relation.is_some());
    assert_eq!(reindex.params.len(), 2);
    assert!(matches!(
        reindex.params.as_slice(),
        [Node::DefElem(verbose), Node::DefElem(concurrently)]
            if verbose.defname.as_deref() == Some("verbose")
                && matches!(verbose.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("true"))
                && concurrently.defname.as_deref() == Some("concurrently")
                && concurrently.arg.is_none()
    ));

    let cases = [
        (
            "reindex index app.items_idx",
            ReindexObjectType::Index,
            true,
            None,
        ),
        (
            "reindex schema app",
            ReindexObjectType::Schema,
            false,
            Some("app"),
        ),
        ("reindex system", ReindexObjectType::System, false, None),
        (
            "reindex system postgres",
            ReindexObjectType::System,
            false,
            Some("postgres"),
        ),
        ("reindex database", ReindexObjectType::Database, false, None),
        (
            "reindex database postgres",
            ReindexObjectType::Database,
            false,
            Some("postgres"),
        ),
        (
            "reindex schema concurrently app",
            ReindexObjectType::Schema,
            false,
            Some("app"),
        ),
        (
            "reindex database concurrently postgres",
            ReindexObjectType::Database,
            false,
            Some("postgres"),
        ),
    ];
    for (sql, kind, has_relation, name) in cases {
        let Node::ReindexStmt(stmt) = parse_statement(sql) else {
            panic!("expected ReindexStmt for {sql}");
        };
        assert_eq!(stmt.kind, kind, "{sql}");
        assert_eq!(stmt.relation.is_some(), has_relation, "{sql}");
        assert_eq!(stmt.name.as_deref(), name, "{sql}");
    }

    let Node::ReindexStmt(options) =
        parse_statement("reindex (verbose, workers -2, mode 'safe') table app.items")
    else {
        panic!("expected ReindexStmt");
    };
    assert!(matches!(
        options.params.as_slice(),
        [Node::DefElem(verbose), Node::DefElem(workers), Node::DefElem(mode)]
            if verbose.arg.is_none()
                && matches!(workers.arg.as_deref(), Some(Node::Integer(value)) if value.ival == -2)
                && matches!(mode.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("safe"))
    ));

    let Node::TruncateStmt(truncate) =
        parse_statement("truncate table only app.items, app.other * restart identity cascade")
    else {
        panic!("expected TruncateStmt");
    };
    assert_eq!(truncate.relations.len(), 2);
    assert!(matches!(
        truncate.relations.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if !first.inh && second.inh
    ));
    assert!(truncate.restart_seqs);
    assert_eq!(truncate.behavior, DropBehavior::Cascade);

    let Node::TruncateStmt(defaults) = parse_statement("truncate app.items") else {
        panic!("expected TruncateStmt");
    };
    assert!(!defaults.restart_seqs);
    assert_eq!(defaults.behavior, DropBehavior::Restrict);

    let Node::TruncateStmt(continue_identity) =
        parse_statement("truncate app.items continue identity restrict")
    else {
        panic!("expected TruncateStmt");
    };
    assert!(!continue_identity.restart_seqs);
    assert_eq!(continue_identity.behavior, DropBehavior::Restrict);
}

#[test]
fn repack_reassign_comment_and_security_label_populate_targets() {
    let Node::RepackStmt(all_relations) = parse_statement("repack") else {
        panic!("expected RepackStmt");
    };
    assert_eq!(all_relations.command, RepackCommand::Repack);
    assert!(all_relations.relation.is_none());
    assert!(!all_relations.usingindex);
    assert!(all_relations.indexname.is_none());

    let Node::RepackStmt(single_relation) = parse_statement("repack app.items") else {
        panic!("expected RepackStmt");
    };
    assert!(single_relation.relation.is_some());
    assert!(!single_relation.usingindex);
    assert!(single_relation.indexname.is_none());

    let Node::RepackStmt(repack) =
        parse_statement("repack (verbose true) app.items(id) using index item_idx")
    else {
        panic!("expected RepackStmt");
    };
    assert_eq!(repack.command, RepackCommand::Repack);
    assert!(repack.usingindex);
    assert_eq!(repack.indexname.as_deref(), Some("item_idx"));
    assert_eq!(repack.params.len(), 1);
    assert_eq!(
        repack
            .relation
            .as_ref()
            .map(|relation| relation.va_cols.len()),
        Some(1)
    );

    let Node::RepackStmt(repack_only) = parse_statement("repack only app.items using index") else {
        panic!("expected RepackStmt");
    };
    assert!(repack_only.usingindex);
    assert!(repack_only.indexname.is_none());
    assert!(
        !repack_only
            .relation
            .as_deref()
            .and_then(|relation| relation.relation.as_deref())
            .expect("relation")
            .inh
    );

    let Node::RepackStmt(all_using_index) = parse_statement("repack using index") else {
        panic!("expected RepackStmt");
    };
    assert!(all_using_index.relation.is_none());
    assert!(all_using_index.usingindex);

    for sql in [
        "cluster",
        "cluster verbose",
        "cluster (verbose true)",
        "cluster app.items",
        "cluster (verbose true) app.items using item_idx",
        "cluster verbose item_idx on app.items",
    ] {
        let Node::RepackStmt(cluster) = parse_statement(sql) else {
            panic!("expected RepackStmt for {sql}");
        };
        assert_eq!(cluster.command, RepackCommand::Cluster, "{sql}");
        assert!(cluster.usingindex, "{sql}");
    }

    let Node::RepackStmt(old_cluster) = parse_statement("cluster verbose item_idx on app.items")
    else {
        panic!("expected RepackStmt");
    };
    assert_eq!(old_cluster.indexname.as_deref(), Some("item_idx"));
    assert_eq!(old_cluster.params.len(), 1);
    assert!(old_cluster.relation.is_some());

    let Node::RepackStmt(option_cluster) =
        parse_statement("cluster (verbose true, workers 2) app.items using item_idx")
    else {
        panic!("expected RepackStmt");
    };
    assert_eq!(option_cluster.params.len(), 2);
    assert_eq!(option_cluster.indexname.as_deref(), Some("item_idx"));
    assert!(option_cluster.relation.is_some());

    let Node::ReassignOwnedStmt(reassign) =
        parse_statement("reassign owned by old_owner, current_user to new_owner")
    else {
        panic!("expected ReassignOwnedStmt");
    };
    assert_eq!(reassign.roles.len(), 2);
    assert!(matches!(
        reassign.roles.as_slice(),
        [Node::RoleSpec(_), Node::RoleSpec(_)]
    ));
    assert!(reassign.newrole.is_some());

    let Node::CommentStmt(comment) =
        parse_statement("comment on table app.items is 'application items'")
    else {
        panic!("expected CommentStmt");
    };
    assert_eq!(comment.objtype, ObjectType::Table);
    assert!(comment.object.is_some());
    assert_eq!(comment.comment.as_deref(), Some("application items"));

    let Node::SecLabelStmt(label) = parse_statement(
        "security label for selinux on table app.items is 'system_u:object_r:table_t:s0'",
    ) else {
        panic!("expected SecLabelStmt");
    };
    assert_eq!(label.provider.as_deref(), Some("selinux"));
    assert_eq!(label.objtype, ObjectType::Table);
    assert!(label.object.is_some());
    assert!(label.label.is_some());
}

#[test]
fn comment_and_security_label_build_object_type_specific_identities() {
    let Node::CommentStmt(function) =
        parse_statement("comment on function app.normalize(int, text) is 'normalizer'")
    else {
        panic!("expected function CommentStmt");
    };
    assert_eq!(function.objtype, ObjectType::Function);
    assert!(matches!(
        function.object.as_deref(),
        Some(Node::ObjectWithArgs(object))
            if object.objname.len() == 2 && object.objargs.len() == 2
    ));

    let Node::CommentStmt(cast) =
        parse_statement("comment on cast (int as text) is 'integer to text'")
    else {
        panic!("expected cast CommentStmt");
    };
    assert_eq!(cast.objtype, ObjectType::Cast);
    assert!(matches!(
        cast.object.as_deref(),
        Some(Node::AArrayExpr(types))
            if types.elements.iter().all(|node| matches!(node, Node::TypeName(_)))
    ));

    let Node::CommentStmt(table_constraint) =
        parse_statement("comment on constraint positive_amount on app.orders is 'positive amount'")
    else {
        panic!("expected table constraint CommentStmt");
    };
    assert_eq!(table_constraint.objtype, ObjectType::Tabconstraint);
    assert!(matches!(
        table_constraint.object.as_deref(),
        Some(Node::AArrayExpr(identity)) if identity.elements.len() == 3
    ));

    let Node::CommentStmt(domain_constraint) = parse_statement(
        "comment on constraint valid_value on domain app.positive_int is 'valid value'",
    ) else {
        panic!("expected domain constraint CommentStmt");
    };
    assert_eq!(domain_constraint.objtype, ObjectType::Domconstraint);
    assert!(matches!(
        domain_constraint.object.as_deref(),
        Some(Node::AArrayExpr(identity))
            if matches!(identity.elements.first(), Some(Node::TypeName(_)))
    ));

    let Node::CommentStmt(trigger) =
        parse_statement("comment on trigger audit on app.orders is null")
    else {
        panic!("expected trigger CommentStmt");
    };
    assert_eq!(trigger.objtype, ObjectType::Trigger);
    assert!(trigger.comment.is_none());

    let Node::CommentStmt(opclass) =
        parse_statement("comment on operator class app.int_ops using btree is 'integer ops'")
    else {
        panic!("expected operator class CommentStmt");
    };
    assert_eq!(opclass.objtype, ObjectType::Opclass);
    assert!(matches!(
        opclass.object.as_deref(),
        Some(Node::AArrayExpr(identity)) if identity.elements.len() == 3
    ));

    let Node::CommentStmt(operator) =
        parse_statement("comment on operator app.-(none, int) is 'integer negation'")
    else {
        panic!("expected operator CommentStmt");
    };
    let Some(Node::ObjectWithArgs(signature)) = operator.object.as_deref() else {
        panic!("expected operator signature");
    };
    assert!(matches!(
        signature.objargs.as_slice(),
        [None, Some(Node::TypeName(_))]
    ));

    let Node::CommentStmt(transform) = parse_statement(
        "comment on transform for app.custom_type language plpgsql is 'custom transform'",
    ) else {
        panic!("expected transform CommentStmt");
    };
    assert_eq!(transform.objtype, ObjectType::Transform);
    assert!(matches!(
        transform.object.as_deref(),
        Some(Node::AArrayExpr(identity))
            if matches!(identity.elements.first(), Some(Node::TypeName(_)))
    ));

    let Node::SecLabelStmt(function_label) = parse_statement(
        "security label for 'selinux' on function app.normalize(int, text) is 'system_u:object_r:function_t:s0'",
    ) else {
        panic!("expected function SecLabelStmt");
    };
    assert_eq!(function_label.provider.as_deref(), Some("selinux"));
    assert_eq!(function_label.objtype, ObjectType::Function);
    assert!(matches!(
        function_label.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));
}

#[test]
fn comment_and_security_label_cover_every_grammar_object_family() {
    let common_objects = [
        ("table app.items", ObjectType::Table),
        ("sequence app.item_ids", ObjectType::Sequence),
        ("view app.item_view", ObjectType::View),
        ("materialized view app.item_cache", ObjectType::Matview),
        ("index app.item_idx", ObjectType::Index),
        ("foreign table app.remote_items", ObjectType::ForeignTable),
        ("property graph app.item_graph", ObjectType::Propgraph),
        ("collation app.item_collation", ObjectType::Collation),
        ("conversion app.item_conversion", ObjectType::Conversion),
        ("statistics app.item_stats", ObjectType::StatisticExt),
        ("text search parser app.item_parser", ObjectType::Tsparser),
        (
            "text search dictionary app.item_dictionary",
            ObjectType::Tsdictionary,
        ),
        (
            "text search template app.item_template",
            ObjectType::Tstemplate,
        ),
        (
            "text search configuration app.item_configuration",
            ObjectType::Tsconfiguration,
        ),
        ("column app.items.name", ObjectType::Column),
        ("access method item_am", ObjectType::AccessMethod),
        ("event trigger item_ddl", ObjectType::EventTrigger),
        ("extension item_extension", ObjectType::Extension),
        ("foreign data wrapper item_fdw", ObjectType::Fdw),
        ("procedural language item_lang", ObjectType::Language),
        ("language sql", ObjectType::Language),
        ("publication item_publication", ObjectType::Publication),
        ("schema app", ObjectType::Schema),
        ("server item_server", ObjectType::ForeignServer),
        ("database item_database", ObjectType::Database),
        ("role item_role", ObjectType::Role),
        ("subscription item_subscription", ObjectType::Subscription),
        ("tablespace item_tablespace", ObjectType::Tablespace),
        ("type app.item_type", ObjectType::Type),
        ("domain app.item_domain", ObjectType::Domain),
        ("aggregate app.item_agg(*)", ObjectType::Aggregate),
        ("function app.item_fn()", ObjectType::Function),
        ("procedure app.item_proc(int)", ObjectType::Procedure),
        ("routine app.item_routine(text)", ObjectType::Routine),
        ("large object 42", ObjectType::Largeobject),
    ];

    for (object, expected_type) in common_objects {
        let Node::CommentStmt(comment) =
            parse_statement(&format!("comment on {object} is 'comment'"))
        else {
            panic!("expected CommentStmt for {object}");
        };
        assert_eq!(comment.objtype, expected_type, "COMMENT ON {object}");
        assert!(comment.object.is_some(), "COMMENT ON {object}");

        let Node::SecLabelStmt(label) =
            parse_statement(&format!("security label on {object} is 'label'"))
        else {
            panic!("expected SecLabelStmt for {object}");
        };
        assert_eq!(label.objtype, expected_type, "SECURITY LABEL ON {object}");
        assert!(label.object.is_some(), "SECURITY LABEL ON {object}");
    }

    let comment_only_objects = [
        ("operator app.+(int, int)", ObjectType::Operator),
        (
            "constraint positive on app.items",
            ObjectType::Tabconstraint,
        ),
        (
            "constraint positive on domain app.item_domain",
            ObjectType::Domconstraint,
        ),
        ("policy item_policy on app.items", ObjectType::Policy),
        ("rule item_rule on app.items", ObjectType::Rule),
        ("trigger item_trigger on app.items", ObjectType::Trigger),
        (
            "transform for app.item_type language sql",
            ObjectType::Transform,
        ),
        (
            "operator class app.item_ops using btree",
            ObjectType::Opclass,
        ),
        (
            "operator family app.item_ops using btree",
            ObjectType::Opfamily,
        ),
        ("cast (int as text)", ObjectType::Cast),
    ];
    for (object, expected_type) in comment_only_objects {
        let Node::CommentStmt(comment) =
            parse_statement(&format!("comment on {object} is 'comment'"))
        else {
            panic!("expected CommentStmt for {object}");
        };
        assert_eq!(comment.objtype, expected_type, "COMMENT ON {object}");
        assert!(comment.object.is_some(), "COMMENT ON {object}");
    }
}

#[test]
fn import_do_return_and_wait_populate_all_clauses() {
    let Node::ImportForeignSchemaStmt(import) = parse_statement(
        "import foreign schema remote limit to (items, app.events) from server foreign_srv into staging options (case 'lower')",
    ) else {
        panic!("expected ImportForeignSchemaStmt");
    };
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

    let Node::ImportForeignSchemaStmt(import) = parse_statement(
        "import foreign schema remote except (only items, app.events *) from server foreign_srv into staging",
    ) else {
        panic!("expected ImportForeignSchemaStmt");
    };
    assert_eq!(import.list_type, ImportForeignSchemaType::Except);
    assert!(matches!(
        import.table_list.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if !first.inh && second.inh
    ));

    let Node::DoStmt(do_stmt) = parse_statement("do language plpgsql 'begin perform 1; end'")
    else {
        panic!("expected DoStmt");
    };
    assert_eq!(do_stmt.args.len(), 2);

    let Node::DoStmt(language_only) = parse_statement("do language plpgsql") else {
        panic!("expected DoStmt");
    };
    assert!(matches!(
        language_only.args.as_slice(),
        [Node::DefElem(option)] if option.defname.as_deref() == Some("language")
    ));

    let Node::WaitStmt(wait) = parse_statement("wait for lsn '0/16B6C50' with (timeout 1000)")
    else {
        panic!("expected WaitStmt");
    };
    assert_eq!(wait.lsn_literal.as_deref(), Some("0/16B6C50"));
    assert_eq!(wait.options.len(), 1);

    let Node::WaitStmt(wait) = parse_statement(
        "wait for lsn '0/16B6C51' with (timeout -1.5, polling true, mode 'strict', trace)",
    ) else {
        panic!("expected WaitStmt with every utility option argument shape");
    };
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

    let Node::WaitStmt(wait) = parse_statement("wait for lsn '0/16B6C52'") else {
        panic!("expected optionless WaitStmt");
    };
    assert!(wait.options.is_empty());
}

#[test]
fn cursor_statements_populate_options_direction_counts_and_names() {
    let Node::DeclareCursorStmt(declare) = parse_statement(
        "declare item_cursor binary scroll cursor with hold for select id from app.items",
    ) else {
        panic!("expected DeclareCursorStmt");
    };
    assert_eq!(declare.portalname.as_deref(), Some("item_cursor"));
    assert_eq!(
        declare.options,
        CURSOR_OPT_BINARY | CURSOR_OPT_SCROLL | CURSOR_OPT_HOLD | CURSOR_OPT_FAST_PLAN
    );
    assert!(matches!(
        declare.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    let Node::FetchStmt(fetch) = parse_statement("fetch backward all from item_cursor") else {
        panic!("expected FetchStmt");
    };
    assert_eq!(fetch.direction, FetchDirection::Backward);
    assert_eq!(fetch.how_many, i64::MAX);
    assert_eq!(fetch.direction_keyword, FetchDirectionKeywords::BackwardAll);
    assert!(!fetch.ismove);
    assert_eq!(fetch.location, -1);

    let Node::FetchStmt(movement) = parse_statement("move absolute -2 in item_cursor") else {
        panic!("expected FetchStmt");
    };
    assert_eq!(movement.direction, FetchDirection::Absolute);
    assert_eq!(movement.how_many, -2);
    assert!(movement.ismove);
    assert_eq!(movement.location, 14);

    let Node::ClosePortalStmt(close) = parse_statement("close all") else {
        panic!("expected ClosePortalStmt");
    };
    assert!(close.portalname.is_none());

    let Node::ClosePortalStmt(close) = parse_statement("close item_cursor") else {
        panic!("expected ClosePortalStmt");
    };
    assert_eq!(close.portalname.as_deref(), Some("item_cursor"));

    let Node::DeclareCursorStmt(all_modes) = parse_statement(
        "declare mode_cursor no scroll insensitive asensitive cursor without hold for select 1",
    ) else {
        panic!("expected DeclareCursorStmt");
    };
    assert_eq!(
        all_modes.options,
        CURSOR_OPT_NO_SCROLL
            | CURSOR_OPT_INSENSITIVE
            | CURSOR_OPT_ASENSITIVE
            | CURSOR_OPT_FAST_PLAN
    );

    for sql in [
        "declare c cursor for with source as (select 1 as id) select id from source",
        "declare c cursor for with source as (select 1 as id) (select id from source)",
        "declare c cursor for (select 1 union select 2)",
        "declare c cursor for values (1), (2)",
        "declare c cursor for table app.items",
    ] {
        let Node::DeclareCursorStmt(stmt) = parse_statement(sql) else {
            panic!("expected DeclareCursorStmt for {sql}");
        };
        assert!(
            matches!(stmt.query.as_deref(), Some(Node::SelectStmt(_))),
            "{sql}"
        );
    }
}

#[test]
fn fetch_and_move_locations_follow_fetch_args_productions() {
    let cases = [
        ("fetch item_cursor", FetchDirectionKeywords::None, 1, -1),
        (
            "fetch from item_cursor",
            FetchDirectionKeywords::None,
            1,
            -1,
        ),
        (
            "fetch next from item_cursor",
            FetchDirectionKeywords::Next,
            1,
            -1,
        ),
        (
            "fetch prior in item_cursor",
            FetchDirectionKeywords::Prior,
            1,
            -1,
        ),
        (
            "fetch first item_cursor",
            FetchDirectionKeywords::First,
            1,
            -1,
        ),
        (
            "fetch last from item_cursor",
            FetchDirectionKeywords::Last,
            -1,
            -1,
        ),
        (
            "fetch all from item_cursor",
            FetchDirectionKeywords::All,
            i64::MAX,
            -1,
        ),
        (
            "fetch forward from item_cursor",
            FetchDirectionKeywords::Forward,
            1,
            -1,
        ),
        (
            "fetch forward all from item_cursor",
            FetchDirectionKeywords::ForwardAll,
            i64::MAX,
            -1,
        ),
        (
            "move backward in item_cursor",
            FetchDirectionKeywords::Backward,
            1,
            -1,
        ),
    ];
    for (sql, keyword, count, location) in cases {
        let Node::FetchStmt(stmt) = parse_statement(sql) else {
            panic!("expected FetchStmt for {sql}");
        };
        assert_eq!(stmt.direction_keyword, keyword, "{sql}");
        assert_eq!(stmt.how_many, count, "{sql}");
        assert_eq!(stmt.location, location, "{sql}");
    }

    for (sql, location) in [
        ("fetch 3 from item_cursor", 6),
        ("fetch absolute -2 from item_cursor", 15),
        ("fetch relative +4 from item_cursor", 15),
        ("move forward 7 in item_cursor", 13),
        ("move backward -8 item_cursor", 14),
    ] {
        let Node::FetchStmt(stmt) = parse_statement(sql) else {
            panic!("expected FetchStmt for {sql}");
        };
        assert_eq!(stmt.location, location, "{sql}");
    }

    for (sql, direction, count, keyword) in [
        (
            "fetch prior c",
            FetchDirection::Backward,
            1,
            FetchDirectionKeywords::Prior,
        ),
        (
            "fetch first c",
            FetchDirection::Absolute,
            1,
            FetchDirectionKeywords::First,
        ),
        (
            "fetch relative -3 c",
            FetchDirection::Relative,
            -3,
            FetchDirectionKeywords::Relative,
        ),
        (
            "move forward +5 c",
            FetchDirection::Forward,
            5,
            FetchDirectionKeywords::Forward,
        ),
        (
            "move backward 6 c",
            FetchDirection::Backward,
            6,
            FetchDirectionKeywords::Backward,
        ),
    ] {
        let Node::FetchStmt(stmt) = parse_statement(sql) else {
            panic!("expected FetchStmt for {sql}");
        };
        assert_eq!(stmt.direction, direction, "{sql}");
        assert_eq!(stmt.how_many, count, "{sql}");
        assert_eq!(stmt.direction_keyword, keyword, "{sql}");
    }
}

#[test]
fn show_and_transaction_statements_populate_special_names_modes_and_identifiers() {
    let Node::VariableSetStmt(timezone) =
        parse_statement("set time zone interval '02:30' hour to minute")
    else {
        panic!("expected VariableSetStmt");
    };
    assert_eq!(timezone.kind, VariableSetKind::SetValue);
    assert_eq!(timezone.name.as_deref(), Some("timezone"));
    assert!(matches!(timezone.args.as_slice(), [Node::TypeCast(_)]));

    let Node::VariableShowStmt(show) = parse_statement("show transaction isolation level") else {
        panic!("expected VariableShowStmt");
    };
    assert_eq!(show.name.as_deref(), Some("transaction_isolation"));

    let Node::TransactionStmt(begin) = parse_statement(
        "begin transaction isolation level repeatable read, read only not deferrable",
    ) else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(begin.kind, TransactionStmtKind::Begin);
    assert_eq!(begin.options.len(), 3);
    assert_eq!(begin.location, -1);
    let Node::DefElem(isolation) = &begin.options[0] else {
        panic!("expected isolation DefElem");
    };
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

    let Node::TransactionStmt(commit) = parse_statement("commit work and chain") else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(commit.kind, TransactionStmtKind::Commit);
    assert!(commit.chain);
    assert_eq!(commit.location, -1);

    let rollback_sql = "rollback transaction to savepoint s1";
    let Node::TransactionStmt(rollback) = parse_statement(rollback_sql) else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(rollback.kind, TransactionStmtKind::RollbackTo);
    assert_eq!(rollback.savepoint_name.as_deref(), Some("s1"));
    assert_eq!(rollback.location, rollback_sql.find("s1").unwrap() as i32);

    let prepare_sql = "prepare transaction 'gid-1'";
    let Node::TransactionStmt(prepare) = parse_statement(prepare_sql) else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(
        prepare.location,
        prepare_sql.find("'gid-1'").unwrap() as i32
    );
    assert_eq!(prepare.kind, TransactionStmtKind::Prepare);
    assert_eq!(prepare.gid.as_deref(), Some("gid-1"));

    let Node::TransactionStmt(commit_prepared) = parse_statement("commit prepared 'gid-1'") else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(commit_prepared.kind, TransactionStmtKind::CommitPrepared);
    assert_eq!(commit_prepared.gid.as_deref(), Some("gid-1"));

    let Node::TransactionStmt(start) =
        parse_statement("start transaction isolation level read uncommitted read write deferrable")
    else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(start.kind, TransactionStmtKind::Start);
    assert_eq!(start.options.len(), 3);

    for (level, expected) in [
        ("read uncommitted", "read uncommitted"),
        ("read committed", "read committed"),
        ("repeatable read", "repeatable read"),
        ("serializable", "serializable"),
    ] {
        let sql = format!("begin isolation level {level}");
        let Node::TransactionStmt(stmt) = parse_statement(&sql) else {
            panic!("expected TransactionStmt for {sql}");
        };
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
        let Node::TransactionStmt(stmt) = parse_statement(&sql) else {
            panic!("expected TransactionStmt for {sql}");
        };
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
        let Node::TransactionStmt(stmt) = parse_statement(sql) else {
            panic!("expected TransactionStmt for {sql}");
        };
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
        let Node::TransactionStmt(stmt) = parse_statement(sql) else {
            panic!("expected TransactionStmt for {sql}");
        };
        assert_eq!(stmt.kind, expected_kind, "{sql}");
        assert_eq!(stmt.savepoint_name.as_deref(), Some(name), "{sql}");
        assert_eq!(stmt.location, sql.find(name).unwrap() as i32, "{sql}");
    }

    let rollback_prepared_sql = "rollback prepared 'gid-2'";
    let Node::TransactionStmt(rollback_prepared) = parse_statement(rollback_prepared_sql) else {
        panic!("expected TransactionStmt");
    };
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
        let Node::VariableSetStmt(stmt) = parse_statement(sql) else {
            panic!("expected VariableSetStmt for {sql}");
        };
        assert_eq!(stmt.location, location, "{sql}");
    }

    let Node::VariableSetStmt(xml) = parse_statement("set xml option document") else {
        panic!("expected VariableSetStmt");
    };
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
        let Node::VariableSetStmt(stmt) = parse_statement(sql) else {
            panic!("expected VariableSetStmt for {sql}");
        };
        assert_eq!(stmt.name.as_deref(), Some(name), "{sql}");
        assert_eq!(stmt.kind, expected_kind, "{sql}");
        assert_eq!(stmt.is_local, is_local, "{sql}");
    }

    let Node::VariableSetStmt(values) = parse_statement("set local app.work_mem to '4MB', 8, on")
    else {
        panic!("expected generic VariableSetStmt");
    };
    assert_eq!(values.args.len(), 3);

    let Node::VariableSetStmt(reset) = parse_statement("reset app.work_mem") else {
        panic!("expected qualified RESET");
    };
    assert_eq!(reset.name.as_deref(), Some("app.work_mem"));
    assert_eq!(reset.kind, VariableSetKind::Reset);

    let Node::VariableShowStmt(show) = parse_statement("show app.work_mem") else {
        panic!("expected qualified SHOW");
    };
    assert_eq!(show.name.as_deref(), Some("app.work_mem"));

    let Node::VariableShowStmt(show_all) = parse_statement("show all") else {
        panic!("expected SHOW ALL");
    };
    assert_eq!(show_all.name.as_deref(), Some("all"));
}

#[test]
fn prepare_execute_and_deallocate_require_and_store_complete_payloads() {
    let Node::PrepareStmt(prepare) =
        parse_statement("prepare find_order (int, text) as select * from orders where id = $1")
    else {
        panic!("expected PrepareStmt");
    };
    assert_eq!(prepare.name.as_deref(), Some("find_order"));
    assert_eq!(prepare.argtypes.len(), 2);
    assert!(matches!(
        prepare.query.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    let Node::ExecuteStmt(execute) = parse_statement("execute find_order(7, 'open')") else {
        panic!("expected ExecuteStmt");
    };
    assert_eq!(execute.name.as_deref(), Some("find_order"));
    assert_eq!(execute.params.len(), 2);

    let Node::DeallocateStmt(named) = parse_statement("deallocate prepare find_order") else {
        panic!("expected DeallocateStmt");
    };
    assert_eq!(named.name.as_deref(), Some("find_order"));
    assert!(!named.isall);
    assert_eq!(named.location, 19);

    let Node::DeallocateStmt(all) = parse_statement("deallocate all") else {
        panic!("expected DeallocateStmt");
    };
    assert!(all.name.is_none());
    assert!(all.isall);
    assert_eq!(all.location, -1);

    let Node::DeallocateStmt(prepared_all) = parse_statement("deallocate prepare all") else {
        panic!("expected DeallocateStmt");
    };
    assert!(prepared_all.name.is_none());
    assert!(prepared_all.isall);
    assert_eq!(prepared_all.location, -1);

    let Node::DeallocateStmt(named) = parse_statement("deallocate find_order") else {
        panic!("expected DeallocateStmt");
    };
    assert_eq!(named.location, 11);

    for (sql, expected_query) in [
        ("prepare p as (select 1)", "SelectStmt"),
        ("prepare p as values (1)", "SelectStmt"),
        ("prepare p as insert into t values (1)", "InsertStmt"),
        ("prepare p as update t set id = 1", "UpdateStmt"),
        ("prepare p as delete from t", "DeleteStmt"),
        (
            "prepare p as merge into t using s on true when matched then do nothing",
            "MergeStmt",
        ),
    ] {
        let Node::PrepareStmt(prepare) = parse_statement(sql) else {
            panic!("expected PrepareStmt for {sql}");
        };
        let actual_query = match prepare.query.as_deref() {
            Some(Node::SelectStmt(_)) => "SelectStmt",
            Some(Node::InsertStmt(_)) => "InsertStmt",
            Some(Node::UpdateStmt(_)) => "UpdateStmt",
            Some(Node::DeleteStmt(_)) => "DeleteStmt",
            Some(Node::MergeStmt(_)) => "MergeStmt",
            other => panic!("unexpected prepared query for {sql}: {other:?}"),
        };
        assert_eq!(actual_query, expected_query, "{sql}");
    }
}

#[test]
fn utility_statement_names_follow_colid_categories() {
    let Node::VariableSetStmt(setting) = parse_statement("set \"select\" to 'value'") else {
        panic!("expected VariableSetStmt");
    };
    assert_eq!(setting.name.as_deref(), Some("select"));

    let Node::TransactionStmt(savepoint) = parse_statement("savepoint \"select\"") else {
        panic!("expected TransactionStmt");
    };
    assert_eq!(savepoint.savepoint_name.as_deref(), Some("select"));

    let Node::PrepareStmt(prepared) = parse_statement("prepare \"select\" as select 1") else {
        panic!("expected PrepareStmt");
    };
    assert_eq!(prepared.name.as_deref(), Some("select"));

    let Node::ListenStmt(listen) = parse_statement("listen \"select\"") else {
        panic!("expected ListenStmt");
    };
    assert_eq!(listen.conditionname.as_deref(), Some("select"));

    let Node::NotifyStmt(notify) = parse_statement("notify \"select\", ''") else {
        panic!("expected NotifyStmt");
    };
    assert_eq!(notify.conditionname.as_deref(), Some("select"));
    assert_eq!(notify.payload.as_deref(), Some(""));

    let Node::UnlistenStmt(unlisten) = parse_statement("unlisten \"select\"") else {
        panic!("expected UnlistenStmt");
    };
    assert_eq!(unlisten.conditionname.as_deref(), Some("select"));

    let Node::DeclareCursorStmt(cursor) = parse_statement("declare \"select\" cursor for select 1")
    else {
        panic!("expected DeclareCursorStmt");
    };
    assert_eq!(cursor.portalname.as_deref(), Some("select"));

    let Node::ImportForeignSchemaStmt(import) =
        parse_statement("import foreign schema \"select\" from server \"from\" into \"where\"")
    else {
        panic!("expected ImportForeignSchemaStmt");
    };
    assert_eq!(import.remote_schema.as_deref(), Some("select"));
    assert_eq!(import.server_name.as_deref(), Some("from"));
    assert_eq!(import.local_schema.as_deref(), Some("where"));
}
