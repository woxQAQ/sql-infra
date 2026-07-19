use super::*;

#[test]
fn create_database_schema_view_and_index_populate_required_fields() {
    let database = parse_node!(
        "create database appdb with encoding 'UTF8' template template0",
        CreatedbStmt
    );
    assert_eq!(database.dbname.as_deref(), Some("appdb"));
    assert_eq!(database.options.len(), 2);

    let options = parse_node!(
        "create database configured with connection limit = -1 encoding 'UTF8' owner app_owner tablespace fast_space template template0 allow_connections on strategy 1.5 locale_provider default",
        CreatedbStmt
    );
    assert!(matches!(
        options.options.as_slice(),
        [
            Node::DefElem(connection),
            Node::DefElem(encoding),
            Node::DefElem(owner),
            Node::DefElem(tablespace),
            Node::DefElem(template),
            Node::DefElem(allow_connections),
            Node::DefElem(strategy),
            Node::DefElem(locale_provider),
        ] if connection.defname.as_deref() == Some("connection_limit")
            && matches!(connection.arg.as_deref(), Some(Node::Integer(value)) if value.ival == -1)
            && matches!(encoding.arg.as_deref(), Some(Node::String(_)))
            && matches!(owner.arg.as_deref(), Some(Node::String(_)))
            && matches!(tablespace.arg.as_deref(), Some(Node::String(_)))
            && matches!(template.arg.as_deref(), Some(Node::String(_)))
            && matches!(allow_connections.arg.as_deref(), Some(Node::String(_)))
            && matches!(strategy.arg.as_deref(), Some(Node::Float(_)))
            && locale_provider.arg.is_none()
    ));

    let schema = parse_node!(
        "create schema if not exists app authorization app_owner",
        CreateSchemaStmt
    );
    assert_eq!(schema.schemaname.as_deref(), Some("app"));
    assert!(schema.authrole.is_some());
    assert!(schema.if_not_exists);

    let view = parse_node!(
        "create view app.active_items(id, name) with (security_barrier = true) as select id, name from app.items where active = true",
        ViewStmt
    );
    assert!(view.view.is_some());
    assert_eq!(view.aliases.len(), 2);
    assert_eq!(view.options.len(), 1);
    assert!(matches!(view.query.as_deref(), Some(Node::SelectStmt(_))));

    let index = parse_node!(
        "create unique index concurrently if not exists item_lookup on app.items using btree (id, lower(name)) include (category) nulls not distinct with (fillfactor = 80) tablespace fast_space where active = true",
        IndexStmt
    );
    assert!(index.unique);
    assert!(index.concurrent);
    assert!(index.if_not_exists);
    assert_eq!(index.idxname.as_deref(), Some("item_lookup"));
    assert_eq!(
        index
            .relation
            .as_deref()
            .and_then(|relation| relation.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(index.access_method.as_deref(), Some("btree"));
    assert_eq!(index.index_params.len(), 2);
    assert_eq!(index.index_including_params.len(), 1);
    assert!(
        index
            .index_params
            .iter()
            .all(|node| matches!(node, Node::IndexElem(_)))
    );
    assert!(
        index
            .index_including_params
            .iter()
            .all(|node| matches!(node, Node::IndexElem(_)))
    );
    assert!(index.nulls_not_distinct);
    assert_eq!(index.options.len(), 1);
    assert_eq!(index.table_space.as_deref(), Some("fast_space"));
    assert!(index.where_clause.is_some());
}

#[test]
fn create_index_populates_all_index_element_options() {
    let sql = "create index item_search on app.items (
             name collate pg_catalog.\"C\" text_pattern_ops (deduplicate_items = false) desc nulls first,
             lower(code) app.custom_ops asc nulls last,
             (id + 1)
         ) include (id int4_ops)";
    let index = parse_node!(sql, IndexStmt);
    let name = expect_node!(&index.index_params[0], IndexElem);
    assert_eq!(name.name.as_deref(), Some("name"));
    assert_eq!(name.collation.len(), 2);
    assert_eq!(name.opclass.len(), 1);
    assert_eq!(name.opclassopts.len(), 1);
    assert_eq!(name.ordering, pg_parser::SortByDir::Desc);
    assert_eq!(name.nulls_ordering, pg_parser::SortByNulls::First);
    assert_eq!(name.location as usize, sql.find("name collate").unwrap());

    let expression = expect_node!(&index.index_params[1], IndexElem);
    assert!(matches!(
        expression.expr.as_deref(),
        Some(Node::FuncCall(_))
    ));
    assert_eq!(expression.opclass.len(), 2);
    assert_eq!(expression.ordering, pg_parser::SortByDir::Asc);
    assert_eq!(expression.nulls_ordering, pg_parser::SortByNulls::Last);
    assert_eq!(
        expression.location as usize,
        sql.find("lower(code)").unwrap()
    );

    let parenthesized = expect_node!(&index.index_params[2], IndexElem);
    assert!(matches!(
        parenthesized.expr.as_deref(),
        Some(Node::AExpr(_))
    ));
    assert_eq!(
        parenthesized.location as usize,
        sql.find("(id + 1)").unwrap()
    );

    let [Node::IndexElem(included)] = index.index_including_params.as_slice() else {
        panic!("expected included IndexElem");
    };
    assert_eq!(included.name.as_deref(), Some("id"));
    assert_eq!(included.opclass.len(), 1);
    assert_eq!(included.location as usize, sql.find("id int4_ops").unwrap());
    assert!(index.exclude_op_names.is_empty());
    assert!(index.idxcomment.is_none());
    assert_eq!(index.index_oid, 0);
    assert_eq!(index.old_number, 0);
    assert_eq!(index.old_create_subid, 0);
    assert_eq!(index.old_first_relfilelocator_subid, 0);
    assert!(!index.primary);
    assert!(!index.isconstraint);
    assert!(!index.iswithoutoverlaps);
    assert!(!index.transformed);
    assert!(!index.reset_default_tblspc);
}

#[test]
fn create_index_and_exclusion_constraints_store_the_default_access_method() {
    let index = parse_node!("create index item_id_idx on items (id)", IndexStmt);
    assert_eq!(index.access_method.as_deref(), Some("btree"));

    let table = parse_node!(
        "create table reservations (room int, exclude (room with =))",
        CreateStmt
    );
    let constraint = table
        .table_elts
        .iter()
        .find_map(|element| match element {
            Node::Constraint(constraint) if constraint.contype == ConstrType::Exclusion => {
                Some(constraint)
            }
            _ => None,
        })
        .expect("exclusion constraint");
    assert_eq!(constraint.access_method.as_deref(), Some("btree"));
}

#[test]
fn create_view_preserves_check_option_and_recursive_raw_rewrite() {
    let local = parse_node!(
        "create temp view app.local_view as select 1 as id with local check option",
        ViewStmt
    );
    assert_eq!(local.with_check_option, ViewCheckOption::LocalCheckOption);
    assert_eq!(
        local.view.as_deref().map(|view| view.relpersistence),
        Some(b't')
    );

    let cascaded = parse_node!(
        "create or replace unlogged view app.cascaded_view as select 1 as id with check option",
        ViewStmt
    );
    assert!(cascaded.replace);
    assert_eq!(
        cascaded.with_check_option,
        ViewCheckOption::CascadedCheckOption
    );
    assert_eq!(
        cascaded.view.as_deref().map(|view| view.relpersistence),
        Some(b'u')
    );

    let recursive = parse_node!(
        "create recursive view app.numbers(n) as
         values (1) union all select n + 1 from numbers where n < 3",
        ViewStmt
    );
    let query = expect_node!(recursive.query.as_deref(), Some(SelectStmt));
    let with = query.with_clause.as_deref().expect("recursive WithClause");
    assert!(with.recursive);
    let [Node::CommonTableExpr(cte)] = with.ctes.as_slice() else {
        panic!("expected recursive CommonTableExpr");
    };
    assert_eq!(cte.ctename.as_deref(), Some("numbers"));
    assert_eq!(cte.aliascolnames.len(), 1);
    assert!(matches!(cte.ctequery.as_deref(), Some(Node::SelectStmt(_))));
    assert!(matches!(
        query.target_list.as_slice(),
        [Node::ResTarget(target)]
            if matches!(target.val.as_deref(), Some(Node::ColumnRef(_)))
    ));
    assert!(matches!(
        query.from_clause.as_slice(),
        [Node::RangeVar(range)] if range.relname.as_deref() == Some("numbers")
    ));
}

#[test]
fn create_materialized_view_populates_into_clause_and_data_option() {
    let stmt = parse_node!(
        "create materialized view if not exists app.item_summary(id) using heap with (fillfactor = 80) tablespace fast_space as select id from app.items with no data",
        CreateTableAsStmt
    );
    assert!(stmt.if_not_exists);
    assert!(matches!(stmt.query.as_deref(), Some(Node::SelectStmt(_))));
    let into = stmt.into.expect("IntoClause");
    assert!(into.rel.is_some());
    assert_eq!(into.col_names.len(), 1);
    assert_eq!(into.access_method.as_deref(), Some("heap"));
    assert_eq!(into.options.len(), 1);
    assert_eq!(into.table_space_name.as_deref(), Some("fast_space"));
    assert!(into.skip_data);
}

#[test]
fn create_query_statements_accept_the_grammar_valid_empty_select() {
    let view = parse_node!("create view empty_view as select", ViewStmt);
    assert!(matches!(
        view.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));

    for sql in [
        "create table empty_ctas as select",
        "create materialized view empty_matview as select",
    ] {
        let stmt = parse_node!(sql, CreateTableAsStmt);
        assert!(matches!(
            stmt.query.as_deref(),
            Some(Node::SelectStmt(select)) if select.target_list.is_empty()
        ));
    }
}
