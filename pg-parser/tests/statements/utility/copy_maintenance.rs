use super::*;

#[test]
fn copy_stmt_populates_relation_query_columns_program_options_and_filter() {
    let table_copy = parse_node!(
        "copy app.items (id, name) from program 'cat data.csv' with (format csv, header true, delimiter ',') where id > 0",
        CopyStmt
    );
    assert!(table_copy.relation.is_some());
    assert!(table_copy.query.is_none());
    assert_eq!(table_copy.attlist.len(), 2);
    assert!(table_copy.is_from);
    assert!(table_copy.is_program);
    assert_eq!(table_copy.filename.as_deref(), Some("cat data.csv"));
    assert_eq!(table_copy.options.len(), 3);
    assert!(table_copy.where_clause.is_some());

    let query_copy = parse_node!(
        "copy (select id from app.items) to stdout with (format csv)",
        CopyStmt
    );
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
        let copy = parse_node!(sql, CopyStmt);
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
    let copy = parse_node!(
        "copy app.items to stdout with (header, format csv, freeze true, reject_limit 12, null default, force_quote *, force_not_null (id, name))",
        CopyStmt
    );
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

    let legacy = parse_node!(
        "copy binary app.items from stdin using delimiters '|' with null as 'N'",
        CopyStmt
    );
    assert_eq!(legacy.options.len(), 3);
    assert_eq!(def(&legacy.options[0]).defname.as_deref(), Some("format"));
    assert_eq!(
        def(&legacy.options[1]).defname.as_deref(),
        Some("delimiter")
    );
    assert_eq!(def(&legacy.options[2]).defname.as_deref(), Some("null"));

    let force = parse_node!(
        "copy app.items to stdout csv force quote id, name force not null * force null archived_at encoding 'UTF8'",
        CopyStmt
    );
    assert_eq!(force.options.len(), 5);
    assert!(matches!(
        def(&force.options[1]).arg.as_deref(),
        Some(Node::AArrayExpr(columns)) if columns.elements.len() == 2
    ));

    let literal_columns = parse_node!(
        "copy app.items to stdout force null 1, 2.5, 'legacy_name'",
        CopyStmt
    );
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
fn vacuum_and_analyze_populate_options_relations_and_columns() {
    let vacuum = parse_node!(
        "vacuum (full true, analyze true) app.items(id, name), app.other",
        VacuumStmt
    );
    assert!(vacuum.is_vacuumcmd);
    assert_eq!(vacuum.options.len(), 2);
    assert_eq!(vacuum.rels.len(), 2);
    let first = expect_node!(&vacuum.rels[0], VacuumRelation);
    assert_eq!(first.va_cols.len(), 2);
    assert!(first.relation.as_deref().expect("relation").inh);

    let legacy = parse_node!(
        "vacuum full freeze verbose analyze only app.items, app.other *",
        VacuumStmt
    );
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

    let analyze = parse_node!("analyze verbose app.items(id)", VacuumStmt);
    assert!(!analyze.is_vacuumcmd);
    assert_eq!(analyze.options.len(), 1);
    assert_eq!(analyze.rels.len(), 1);

    let british = parse_node!(
        "analyse (verbose, buffer_usage_limit '8MB', sample_rate 0.25) app.items",
        VacuumStmt
    );
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
    let explain = parse_node!(
        "explain (analyze true, verbose true) select * from app.items",
        ExplainStmt
    );
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
        let explain = parse_node!(sql, ExplainStmt);
        assert_eq!(
            explain.query.as_deref().map(Node::tag),
            Some(expected_tag),
            "{sql}"
        );
    }

    let bare_checkpoint = parse_node!("checkpoint", CheckPointStmt);
    assert!(bare_checkpoint.options.is_empty());

    let checkpoint = parse_node!("checkpoint (fast true)", CheckPointStmt);
    assert_eq!(checkpoint.options.len(), 1);

    for (sql, expected) in [
        ("discard all", DiscardMode::All),
        ("discard plans", DiscardMode::Plans),
        ("discard sequences", DiscardMode::Sequences),
        ("discard temp", DiscardMode::Temp),
        ("discard temporary", DiscardMode::Temp),
    ] {
        let discard = parse_node!(sql, DiscardStmt);
        assert_eq!(discard.target, expected, "{sql}");
    }
}

#[test]
fn refresh_reindex_and_truncate_populate_all_raw_fields() {
    let refresh = parse_node!(
        "refresh materialized view concurrently app.summary with no data",
        RefreshMatViewStmt
    );
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
        let stmt = parse_node!(sql, RefreshMatViewStmt);
        assert_eq!(stmt.concurrent, concurrent, "{sql}");
        assert_eq!(stmt.skip_data, skip_data, "{sql}");
    }

    let reindex = parse_node!(
        "reindex (verbose true) table concurrently app.items",
        ReindexStmt
    );
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
        let stmt = parse_node!(sql, ReindexStmt);
        assert_eq!(stmt.kind, kind, "{sql}");
        assert_eq!(stmt.relation.is_some(), has_relation, "{sql}");
        assert_eq!(stmt.name.as_deref(), name, "{sql}");
    }

    let options = parse_node!(
        "reindex (verbose, workers -2, mode 'safe') table app.items",
        ReindexStmt
    );
    assert!(matches!(
        options.params.as_slice(),
        [Node::DefElem(verbose), Node::DefElem(workers), Node::DefElem(mode)]
            if verbose.arg.is_none()
                && matches!(workers.arg.as_deref(), Some(Node::Integer(value)) if value.ival == -2)
                && matches!(mode.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("safe"))
    ));

    let truncate = parse_node!(
        "truncate table only app.items, app.other * restart identity cascade",
        TruncateStmt
    );
    assert_eq!(truncate.relations.len(), 2);
    assert!(matches!(
        truncate.relations.as_slice(),
        [Node::RangeVar(first), Node::RangeVar(second)] if !first.inh && second.inh
    ));
    assert!(truncate.restart_seqs);
    assert_eq!(truncate.behavior, DropBehavior::Cascade);

    let defaults = parse_node!("truncate app.items", TruncateStmt);
    assert!(!defaults.restart_seqs);
    assert_eq!(defaults.behavior, DropBehavior::Restrict);

    let continue_identity = parse_node!(
        "truncate app.items continue identity restrict",
        TruncateStmt
    );
    assert!(!continue_identity.restart_seqs);
    assert_eq!(continue_identity.behavior, DropBehavior::Restrict);
}
