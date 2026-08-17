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

/// String stability alone cannot detect semantics lost by the first deparse
/// (e.g. a dropped `IGNORE NULLS`). This gate compares the original parse tree
/// against the tree of the deparsed SQL, ignoring `ParseLoc` fields.
fn assert_semantics_preserved(sql: &str) {
    let first_tree = parse(sql).unwrap_or_else(|error| panic!("failed to parse {sql:?}: {error}"));
    let first_sql =
        deparse(&first_tree).unwrap_or_else(|error| panic!("failed to deparse {sql:?}: {error}"));
    let second_tree = parse(&first_sql)
        .unwrap_or_else(|error| panic!("failed to parse deparsed SQL {first_sql:?}: {error}"));
    assert_eq!(
        erase_parse_locs(&format!("{first_tree:#?}")),
        erase_parse_locs(&format!("{second_tree:#?}")),
        "deparse changed the parse tree semantics for {sql:?}"
    );
}

fn erase_parse_locs(debug: &str) -> std::string::String {
    const FIELDS: [&str; 9] = [
        "parse_loc",
        "stmt_parse_loc",
        "stmt_len",
        "target_parse_loc",
        "name_parse_loc",
        "list_start_parse_loc",
        "list_end_parse_loc",
        "rexpr_list_start_parse_loc",
        "rexpr_list_end_parse_loc",
    ];
    let bytes = debug.as_bytes();
    let mut out = std::string::String::with_capacity(debug.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let word = &debug[start..index];
            if FIELDS.contains(&word) && debug[index..].starts_with(": ") {
                out.push_str(word);
                out.push_str(": #");
                index += 2;
                while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'-')
                {
                    index += 1;
                }
                continue;
            }
            out.push_str(word);
            continue;
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
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
fn deparses_partitioned_tables_and_storage_options() {
    for sql in [
        "create table child partition of parent for values from (1) to (10)",
        "create table child partition of parent for values in (1, 2)",
        "create table child partition of parent for values with (modulus 4, remainder 1)",
        "create table child partition of parent default",
        "create table child partition of parent (id with options not null) for values from (1) to (10)",
        "create table child partition of parent for values from (1) to (10) using heap with (fillfactor = 70)",
        "create table metrics (id integer) partition by range (id)",
        "create table plain (id integer) using heap with (fillfactor = 70)",
        "create table snapshot using heap with (fillfactor = 70) tablespace fast as select 1 with no data",
        "create temporary table temp_snapshot on commit drop as select 1",
        "create materialized view fast_view with (fillfactor = 70) tablespace fast as select 1 with no data",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_constraint_attributes_and_exclusion() {
    for sql in [
        "create table t (a integer, b integer, unique (a) include (b) with (fillfactor = 70) using index tablespace fast deferrable initially deferred)",
        "create table t (a integer unique nulls not distinct)",
        "create table t (a integer, primary key (a) using index tablespace fast)",
        "create table t (a integer, constraint pk primary key using index existing_idx)",
        "create table t (a integer, b integer, foreign key (a) references parent (id) on delete cascade deferrable initially deferred)",
        "create table t (a integer, foreign key (a) references parent (id) match full on update restrict on delete cascade)",
        "create table t (a integer check (a > 0) not enforced)",
        "create table t (r int4range, exclude using gist (r with &&) where (r is not null))",
        "create table t (r int4range, tag text, exclude (r with &&, tag with =) include (tag))",
        "create table t (a text, exclude (a text_ops (deduplicate_items = false) with =))",
        "create index i on t (a text_ops (deduplicate_items = false))",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_function_options_and_xml_expressions() {
    for sql in [
        "create function f() returns integer language sql immutable strict security definer as 'select 1'",
        "create function f() returns integer language sql not leakproof parallel safe cost 5 rows 10 as 'select 1'",
        "create function f() returns integer language internal called on null input security invoker as 'f', 'f_sym'",
        "create function f() returns integer language sql support my_support set work_mem = '16MB' as 'select 1'",
        "create function f() returns integer language sql transform for type integer as 'select 1'",
        "create procedure p() language plpgsql security invoker as 'begin end'",
        "select xmlparse(document '<a/>' preserve whitespace)",
        "select xmlparse(content '<a/>' strip whitespace)",
        "select xmlparse(document '<a/>')",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_sql_standard_function_bodies_and_return_tables() {
    for sql in [
        "create function f() returns int language sql begin atomic select 1; end",
        "create procedure p() language sql begin atomic insert into t values (1); end",
        "create function f() returns int language sql return 1",
        "create function f(x text) returns table(id bigint, label text) language sql as 'select 1'",
        "create procedure p(out x int) language sql as 'call x'",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_object_identities_by_object_type() {
    for sql in [
        "drop operator class app.text_ops using btree",
        "drop operator family app.text_ops using btree",
        "drop trigger tr on app.items",
        "drop policy p on app.t cascade",
        "drop rule r on app.t",
        "drop operator -(none, integer)",
        "drop aggregate app.count_rows(*)",
        "drop aggregate app.total(integer)",
        "drop aggregate app.percentile(float8 order by float8)",
        "drop aggregate app.percentile(order by float8)",
        "drop aggregate app.f(variadic int[] order by variadic int[])",
        "drop aggregate app.f(int, variadic int[] order by variadic int[])",
        "drop aggregate app.f(variadic int[])",
        "comment on aggregate app.f(variadic int[] order by variadic int[]) is 'x'",
        "security label for selinux on aggregate app.f(variadic int[] order by variadic int[]) is 'x'",
        "drop cast (int as text)",
        "comment on constraint c on app.t is 'x'",
        "comment on constraint c on domain app.d is 'x'",
        "comment on operator class app.text_ops using btree is 'x'",
        "comment on trigger tr on app.items is 'x'",
        "comment on cast (int as text) is 'x'",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_special_set_statements() {
    for sql in [
        "set time zone interval '02:30' hour to minute",
        "set time zone interval '1' hour",
        "set time zone 'UTC'",
        "set time zone default",
        "set local time zone 'UTC'",
        "set transaction isolation level serializable",
        "set transaction read only, deferrable",
        "set session characteristics as transaction read only",
        "set transaction snapshot '00000003-0000001F-1'",
        "set session authorization 'bob'",
        "set session authorization default",
        "set role 'bob'",
        "set xml option content",
        "set xml option document",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_indirection_with_parenthesized_bases() {
    for sql in [
        "select (row(a, b)).field from t",
        "select (f())[1]",
        "select (cast(a as int[]))[1] from t",
        "select (t.*)[1] from t",
        "select a[1:2], b.c, d[1].e from t",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_window_null_treatment_and_named_windows() {
    for sql in [
        "select lag(x) ignore nulls over () from t",
        "select lag(x) respect nulls over () from t",
        "select lag(x) over w from t window w as (order by x)",
        "select lag(x) over w from t window w as ()",
        "select a, count(*) from t group by all a",
        "select a from t group by distinct a",
        "create temp view local_view as select 1",
        "create or replace unlogged view v as select 1",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn object_with_args_default_is_not_an_ordered_aggregate() {
    let default = pg_parser::ObjectWithArgs::default();
    assert_eq!(default.agg_signature, pg_parser::AggregateSignature::None);
}

#[test]
fn deparse_rejects_inconsistent_ordered_set_signatures() {
    use pg_parser::AggregateSignature;
    use pg_parser::DropStmt;
    use pg_parser::FunctionParameter;
    use pg_parser::FunctionParameterMode;
    use pg_parser::ObjectType;
    use pg_parser::ObjectWithArgs;
    use pg_parser::RawStmt;
    use pg_parser::String as PgString;
    use pg_parser::TypeName;

    let name = || {
        Node::String(PgString {
            sval: Some("f".to_owned()),
        })
    };
    let int4 = || TypeName {
        names: vec![Node::String(PgString {
            sval: Some("int4".to_owned()),
        })],
        ..TypeName::default()
    };
    let parameter = |mode| {
        Node::FunctionParameter(FunctionParameter {
            arg_type: Some(Box::new(int4())),
            mode,
            ..FunctionParameter::default()
        })
    };
    let deparse_drop = |object: ObjectWithArgs| {
        deparse(&[RawStmt {
            stmt: Some(Box::new(Node::DropStmt(DropStmt {
                objects: vec![Node::ObjectWithArgs(object)],
                remove_type: ObjectType::Aggregate,
                ..DropStmt::default()
            }))),
            ..RawStmt::default()
        }])
    };

    // A consistent shared-VARIADIC signature renders the boundary.
    let shared = deparse_drop(ObjectWithArgs {
        objname: vec![name()],
        objfuncargs: vec![parameter(FunctionParameterMode::Variadic)],
        agg_signature: AggregateSignature::OrderedSet {
            direct_args: 1,
            shared_variadic: true,
        },
        ..ObjectWithArgs::default()
    });
    assert_eq!(
        shared.expect("consistent shared VARIADIC signature"),
        "DROP AGGREGATE f(VARIADIC int4 ORDER BY VARIADIC int4)"
    );

    // shared_variadic with extra ordered arguments is inconsistent.
    let extra_ordered = deparse_drop(ObjectWithArgs {
        objname: vec![name()],
        objfuncargs: vec![
            parameter(FunctionParameterMode::Variadic),
            parameter(FunctionParameterMode::In),
        ],
        agg_signature: AggregateSignature::OrderedSet {
            direct_args: 1,
            shared_variadic: true,
        },
        ..ObjectWithArgs::default()
    });
    assert!(extra_ordered.is_err());

    // shared_variadic without a trailing VARIADIC direct argument is
    // inconsistent.
    let not_variadic = deparse_drop(ObjectWithArgs {
        objname: vec![name()],
        objfuncargs: vec![parameter(FunctionParameterMode::In)],
        agg_signature: AggregateSignature::OrderedSet {
            direct_args: 1,
            shared_variadic: true,
        },
        ..ObjectWithArgs::default()
    });
    assert!(not_variadic.is_err());

    // A non-shared boundary must leave at least one ordered argument.
    let no_ordered = deparse_drop(ObjectWithArgs {
        objname: vec![name()],
        objfuncargs: vec![parameter(FunctionParameterMode::In)],
        agg_signature: AggregateSignature::OrderedSet {
            direct_args: 1,
            shared_variadic: false,
        },
        ..ObjectWithArgs::default()
    });
    assert!(no_ordered.is_err());

    // A non-aggregate default-constructed identity renders as a plain call.
    let plain = deparse(&[RawStmt {
        stmt: Some(Box::new(Node::DropStmt(DropStmt {
            objects: vec![Node::ObjectWithArgs(ObjectWithArgs {
                objname: vec![name()],
                objargs: vec![Some(Node::TypeName(int4()))],
                ..ObjectWithArgs::default()
            })],
            remove_type: ObjectType::Function,
            ..DropStmt::default()
        }))),
        ..RawStmt::default()
    }])
    .expect("default ObjectWithArgs is a plain signature");
    assert_eq!(plain, "DROP FUNCTION f(int4)");
}

#[test]
fn deparse_node_rejects_object_with_args_without_object_type() {
    let tree = parse("drop aggregate app.count_rows(*)").expect("parse drop aggregate");
    let Node::DropStmt(statement) = tree[0].stmt.as_deref().expect("statement") else {
        panic!("expected DropStmt");
    };
    let error = deparse_node(&statement.objects[0]).unwrap_err();
    assert!(error.to_string().contains("object type"));
}

#[test]
fn deparses_for_portion_of_with_alias() {
    for sql in [
        "update items for portion of valid_time from '2020-01-01' to '2021-01-01' as cur set a = 1",
        "delete from items for portion of valid_time from '2020-01-01' to '2021-01-01' as cur",
        "update items for portion of valid_time from '2020-01-01' to '2021-01-01' set a = 1",
        "update items as cur set a = 1",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparses_table_not_null_like_and_column_storage() {
    for sql in [
        "create table t (a int, not null a not valid no inherit)",
        "create table t (a int, constraint nn not null a)",
        "create table t (a text storage external compression pglz)",
        "create table t (a text storage default)",
        "create table t (like src including all)",
        "create table t (like src including defaults including indexes)",
        "create table t (a bigint generated always as identity (start with 10 increment by 5))",
        "create table t (a bigint generated by default as identity (minvalue 1 no maxvalue no cycle cache 20))",
        "explain (analyze true, verbose true) select 1",
        "explain (format json, costs off) select 1",
        "explain analyze verbose select 1",
    ] {
        assert_round_trip(sql);
    }
}

#[test]
fn deparse_preserves_tree_semantics() {
    for sql in [
        "select lag(x) ignore nulls over () from t",
        "select lag(x) respect nulls over (partition by g) from t",
        "select lag(x) over w from t window w as (order by x)",
        "create table c partition of p for values from (minvalue) to (maxvalue)",
        "create table c partition of p for values from (minvalue, '2026-01-01') to (maxvalue, '2027-01-01')",
        "update items for portion of valid_time from '2020-01-01' to '2021-01-01' as cur set a = 1",
        "delete from items for portion of valid_time from '2020-01-01' to '2021-01-01' as cur",
        "set time zone interval '02:30' hour to minute",
        "set transaction isolation level serializable",
        "set session characteristics as transaction read only",
        "create table t (a text storage external compression pglz)",
        "create table t (a text, not null a not valid no inherit)",
        "drop operator class app.text_ops using btree",
        "drop aggregate app.count_rows(*)",
        "drop aggregate app.percentile(float8 order by float8)",
        "drop aggregate app.f(variadic int[] order by variadic int[])",
        "drop aggregate app.f(variadic int[])",
        "create temp view local_view as select 1",
        "select a, count(*) from t group by all a",
        "create table t (like src including defaults)",
    ] {
        assert_semantics_preserved(sql);
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
