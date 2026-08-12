use pg_parser::{Node, deparse, deparse_node, deparse_statement, parse, parse_one};

fn assert_round_trip(sql: &str) {
    let first_tree = parse(sql).unwrap_or_else(|error| panic!("failed to parse {sql:?}: {error}"));
    let first_sql =
        deparse(&first_tree).unwrap_or_else(|error| panic!("failed to deparse {sql:?}: {error}"));
    let second_tree = parse(&first_sql)
        .unwrap_or_else(|error| panic!("failed to parse deparsed SQL {first_sql:?}: {error}"));
    let second_sql = deparse(&second_tree)
        .unwrap_or_else(|error| panic!("failed to deparse second tree for {sql:?}: {error}"));
    assert_eq!(
        first_sql, second_sql,
        "deparse output was not stable for {sql:?}"
    );
}

#[test]
fn deparses_query_expressions_and_clauses() {
    for sql in [
        "select distinct a, b + 1 as next_b from app.items where active = true group by a, b having count(*) > 0 order by a desc nulls last limit 10 offset 2",
        "select value::numeric(4, 2), case when active then 'yes' else 'no' end from items",
        "select a from left_table l left join right_table r on l.id = r.id",
        "select count(*) filter (where active) over (partition by category order by id rows between 2 preceding and current row) from items",
        "select * from items where id in (1, 2, 3) and deleted_at is null",
        "with recursive x(n) as (values (1) union all select n + 1 from x where n < 3) select * from x",
        "select 1 union all select 2 intersect select 3",
        "values (1, 'one'), (2, 'two')",
        "select percentile_cont(0.5) within group (order by score) from items",
        "select id from items order by id fetch first 5 rows with ties",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_dml_statements() {
    for sql in [
        "insert into items (id, name) overriding system value values (1, 'one') on conflict (id) where id > 0 do update set name = 'updated' returning with (old as previous, new as current) id",
        "update public.items set name = 'updated', nums[1:3] = array[1, 2, 3] from audit where items.id = audit.id returning items.id",
        "update items set (name, status) = row('updated', 'active')",
        "delete from public.items using audit where items.id = audit.id returning items.id",
        "merge into target t using source s on t.id = s.id when matched then update set name = s.name when not matched by target then insert (id, name) values (s.id, s.name)",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_common_ddl_statements() {
    for sql in [
        "create table app.items (id integer primary key, name text not null, score numeric(5, 2) default 0, constraint positive_score check (score >= 0))",
        "create temporary table if not exists snapshot (id integer) on commit drop",
        "create unique index concurrently if not exists items_name_idx on app.items using btree (name desc nulls last) where active",
        "create view app.active_items (id, name) as select id, name from app.items where active with local check option",
        "create table app.snapshot as select * from app.items with no data",
        "create domain app.positive_int as integer check (value > 0)",
        "create type app.mood as enum ('happy', 'sad')",
        "create type app.float_range as range (subtype = float8)",
        "drop table if exists app.old_items cascade",
        "truncate table app.items, app.audit restart identity cascade",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_common_utility_statements() {
    for sql in [
        "set local app.work_mem to '4MB', 8",
        "reset app.work_mem",
        "show app.work_mem",
        "prepare find_order (int, text) as select * from orders where id = $1",
        "execute find_order(7, 'open')",
        "deallocate find_order",
        "listen item_changes",
        "notify item_changes, 'updated'",
        "unlisten *",
        "begin isolation level serializable, read only",
        "commit and chain",
        "explain (analyze, verbose) select * from items",
        "refresh materialized view concurrently app.summary",
        "comment on table app.items is 'application items'",
        "security label for selinux on table app.items is 'system_u:object_r:sepgsql_table_t:s0'",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_table_functions_and_join_trees() {
    for sql in [
        "select * from generate_series(1, 3)",
        "select * from generate_series(1, 3) with ordinality",
        "select * from generate_series(1, 3) as g (n)",
        "select * from generate_series(1, 3) as (n integer)",
        "select * from rows from (generate_series(1, 3), generate_series(4, 6) as (n integer)) with ordinality as t (a, b)",
        "select * from lateral generate_series(1, 3) as g",
        "select * from a cross join b",
        "select * from a join (b join c on b.id = c.id) on a.id = b.id",
        "select * from (a join b on a.id = b.id) as x join c on x.id = c.id",
        "select * from a natural join b cross join c",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparse_quotes_identifiers_and_literals() {
    assert_round_trip("select \"select\", 'it''s valid' from \"Mixed Case\"");
}

#[test]
fn deparse_node_handles_fragments_and_rejects_analysis_nodes() {
    let raw = parse_one("select 1 + 2").expect("parse select");
    assert_eq!(
        deparse_statement(&raw).expect("deparse statement"),
        "SELECT (1 + 2)"
    );
    let Node::SelectStmt(select) = raw.stmt.as_deref().expect("statement") else {
        panic!("expected SelectStmt");
    };
    let Node::ResTarget(target) = &select.target_list[0] else {
        panic!("expected ResTarget");
    };
    let expression = target.val.as_deref().expect("expression");
    let sql = deparse_node(expression).expect("deparse expression");
    assert_eq!(sql, "(1 + 2)");

    let error = deparse_node(&Node::Query(pg_parser::Query::default())).unwrap_err();
    assert!(error.to_string().contains("analysis tree"));
}
