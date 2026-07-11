use super::*;

fn first_node(sql: &str) -> Node {
    let stmt = parse_one(sql).unwrap();
    *stmt.stmt.unwrap()
}

#[test]
fn parses_basic_select_insert_update_delete() {
    assert!(matches!(
        first_node("select a, b from t where id = 1"),
        Node::SelectStmt(_)
    ));
    assert!(matches!(
        first_node("insert into t (a) values (1) returning a"),
        Node::InsertStmt(_)
    ));
    assert!(matches!(
        first_node("update t set a = 1 where id = 2"),
        Node::UpdateStmt(_)
    ));
    assert!(matches!(
        first_node("delete from t where id = 3"),
        Node::DeleteStmt(_)
    ));
}

#[test]
fn parses_multiple_raw_statements() {
    let stmts = parse("select 1; select 2;").unwrap();
    assert_eq!(stmts.len(), 2);
    assert!(matches!(
        *stmts[0].stmt.clone().unwrap(),
        Node::SelectStmt(_)
    ));
    assert!(matches!(
        *stmts[1].stmt.clone().unwrap(),
        Node::SelectStmt(_)
    ));
}

#[test]
fn parses_common_create_alter_drop_forms() {
    assert!(matches!(
        first_node("create table s.t (id int, name text)"),
        Node::CreateStmt(_)
    ));
    assert!(matches!(
        first_node("create unique index idx on t (id)"),
        Node::IndexStmt(_)
    ));
    assert!(matches!(
        first_node("create view v as select 1"),
        Node::ViewStmt(_)
    ));
    assert!(matches!(
        first_node("alter table t add column x int"),
        Node::AlterTableStmt(_)
    ));
    assert!(matches!(
        first_node("drop table if exists t cascade"),
        Node::DropStmt(_)
    ));
}

#[test]
fn parses_utility_statements() {
    let cases = [
        ("set search_path to public", "set"),
        ("show search_path", "show"),
        ("begin", "begin"),
        ("commit", "commit"),
        ("prepare q as select 1", "prepare"),
        ("execute q", "execute"),
        ("deallocate q", "deallocate"),
        ("explain select 1", "explain"),
        ("copy t from 'file.csv'", "copy"),
        ("vacuum t", "vacuum"),
        ("call f(1)", "call"),
        ("listen chan", "listen"),
        ("notify chan, 'payload'", "notify"),
    ];
    for (sql, label) in cases {
        parse_one(sql).unwrap_or_else(|err| panic!("{label}: {err}"));
    }
}

#[test]
fn dispatches_broad_statement_family() {
    let cases = [
        "create schema s",
        "create database d",
        "create extension e",
        "create role r",
        "create sequence s",
        "create domain d as int",
        "create type mood as enum ('sad','ok')",
        "create publication p",
        "create subscription s connection 'x' publication p",
        "drop database if exists d",
        "drop role if exists r",
        "drop owned by r",
        "truncate table t",
        "comment on table t is 'x'",
        "security label on table t is 'x'",
        "grant select on table t to r",
        "revoke select on table t from r",
        "refresh materialized view mv",
        "reindex table t",
        "discard all",
        "lock table t",
        "load 'x'",
        "wait for lsn '0/0'",
    ];
    for sql in cases {
        parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
    }
}

#[test]
fn builds_expression_ast_for_common_precedence() {
    let Node::SelectStmt(stmt) = first_node("select a + 1 * 2 from t where b::int >= 3 and not c")
    else {
        panic!("expected select");
    };
    let Node::ResTarget(target) = &stmt.target_list[0] else {
        panic!("expected target");
    };
    assert!(matches!(target.val.as_deref(), Some(Node::AExpr(_))));
    assert!(matches!(
        stmt.where_clause.as_deref(),
        Some(Node::BoolExpr(_))
    ));
}

#[test]
fn dispatches_official_top_level_statement_families() {
    let cases = [
        "alter event trigger et disable",
        "alter collation c refresh version",
        "alter database d refresh collation version",
        "alter database d set search_path to public",
        "alter default privileges grant select on tables to r",
        "alter domain d set default 1",
        "alter type mood add value 'ok'",
        "alter extension e add table t",
        "alter foreign data wrapper fdw options (foo 'bar')",
        "alter server s options (foo 'bar')",
        "alter function f() stable",
        "alter group g add user u",
        "alter function f() depends on extension e",
        "alter table t set schema s",
        "alter table t owner to r",
        "alter operator +(int, int) set (commutator = +)",
        "alter type t set (receive = r)",
        "alter policy p on t using (true)",
        "alter property graph g add vertex tables (t)",
        "alter sequence s restart",
        "alter system set work_mem = '4MB'",
        "alter table t add column c int",
        "alter tablespace ts set (random_page_cost = 2)",
        "alter type ct add attribute a int",
        "alter publication p set table t",
        "alter role r set search_path to public",
        "alter subscription s refresh publication",
        "alter statistics st set statistics 10",
        "alter text search dictionary d (template = simple)",
        "alter user mapping for u server s options (foo 'bar')",
        "analyze t",
        "call f(1)",
        "checkpoint",
        "close c",
        "comment on table t is 'x'",
        "set constraints all deferred",
        "copy t from 'file.csv'",
        "create access method am type table handler h",
        "create table ct_as as select 1",
        "create cast (int as text) without function",
        "create conversion conv for 'utf8' to 'latin1' from f",
        "create domain d as int",
        "create extension e",
        "create foreign data wrapper fdw",
        "create server s foreign data wrapper fdw",
        "create foreign table ft (id int) server s",
        "create function f() returns int language sql as 'select 1'",
        "create group g",
        "create materialized view mv as select 1",
        "create operator class opc for type int using btree as operator 1 =",
        "create operator family opf using btree",
        "alter operator family opf using btree add operator 1 =(int,int)",
        "create policy p on t using (true)",
        "create language plpgsql handler plpgsql_call_handler",
        "create property graph g vertex tables (t)",
        "create schema s",
        "create sequence seq",
        "create table t (id int)",
        "create subscription sub connection 'c' publication p",
        "create statistics st on a from t",
        "create tablespace ts location '/tmp'",
        "create transform for int language plpgsql (from sql with function f(int))",
        "create trigger tr before insert on t execute function f()",
        "create event trigger et on ddl_command_start execute function f()",
        "create role r",
        "create user u",
        "create user mapping for u server s",
        "create database d",
        "deallocate q",
        "declare c cursor for select 1",
        "create aggregate agg(int) (sfunc = f, stype = int)",
        "delete from t where id = 1",
        "discard all",
        "do 'begin end'",
        "drop cast (int as text)",
        "drop operator class opc using btree",
        "drop operator family opf using btree",
        "drop owned by r",
        "drop table if exists t",
        "drop subscription if exists sub",
        "drop tablespace if exists ts",
        "drop transform for int language plpgsql",
        "drop role if exists r",
        "drop user mapping if exists for u server s",
        "drop database if exists d",
        "execute q",
        "explain select 1",
        "fetch next from c",
        "grant select on table t to r",
        "grant r to u",
        "import foreign schema s from server srv into public",
        "create index idx on t (id)",
        "insert into t values (1)",
        "listen ch",
        "refresh materialized view mv",
        "load 'x'",
        "lock table t",
        "merge into t using s on t.id = s.id when matched then update set id = s.id",
        "notify ch, 'payload'",
        "prepare q as select 1",
        "reassign owned by r to u",
        "reindex table t",
        "drop aggregate if exists agg(int)",
        "drop function if exists f()",
        "drop operator if exists +(int, int)",
        "alter table t rename to t2",
        "repack t using index idx",
        "revoke select on table t from r",
        "revoke r from u",
        "create rule r as on update to t do notify ch",
        "security label on table t is 'x'",
        "select 1",
        "begin",
        "truncate table t",
        "unlisten *",
        "update t set id = 2",
        "vacuum t",
        "reset search_path",
        "set search_path to public",
        "show search_path",
        "create view v as select 1",
        "wait for lsn '0/0'",
    ];

    for sql in cases {
        parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
    }
}

#[test]
fn dispatches_specific_extended_statement_nodes() {
    assert!(matches!(
        first_node("create table t as select 1"),
        Node::CreateTableAsStmt(_)
    ));
    assert!(matches!(
        first_node("create foreign data wrapper fdw"),
        Node::CreateFdwStmt(_)
    ));
    assert!(matches!(
        first_node("create property graph g vertex tables (t)"),
        Node::CreatePropGraphStmt(_)
    ));
    assert!(matches!(
        first_node("alter extension e add table t"),
        Node::AlterExtensionContentsStmt(_)
    ));
    assert!(matches!(
        first_node("alter table t set schema s"),
        Node::AlterObjectSchemaStmt(_)
    ));
    assert!(matches!(
        first_node("alter table t owner to r"),
        Node::AlterTableStmt(AlterTableStmt { cmds, .. })
            if matches!(cmds.first(), Some(Node::AlterTableCmd(AlterTableCmd {
                subtype: AlterTableType::ChangeOwner,
                ..
            })))
    ));
    assert!(matches!(
        first_node("alter role r set search_path to public"),
        Node::AlterRoleSetStmt(_)
    ));
    assert!(matches!(
        first_node("alter type ct add attribute a int"),
        Node::AlterTableStmt(AlterTableStmt {
            objtype: ObjectType::Type,
            ..
        })
    ));
    assert!(matches!(
        first_node("drop cast (int as text)"),
        Node::DropStmt(DropStmt {
            remove_type: ObjectType::Cast,
            ..
        })
    ));
    assert!(matches!(
        first_node("create rule r as on update to t do notify ch"),
        Node::RuleStmt(_)
    ));
    assert!(matches!(first_node("repack t"), Node::RepackStmt(_)));
    assert!(matches!(
        first_node("create recursive view v (n) as select 1"),
        Node::ViewStmt(_)
    ));
}

#[test]
fn fills_complex_create_and_alter_fields() {
    let Node::CreateCastStmt(cast) =
        first_node("create cast (int as text) with inout as assignment")
    else {
        panic!("expected cast");
    };
    assert!(cast.sourcetype.is_some());
    assert!(cast.targettype.is_some());
    assert!(cast.inout);
    assert_eq!(cast.context, CoercionContext::Assignment);

    let Node::CreateForeignServerStmt(server) = first_node(
        "create server if not exists s type 't' version '1' foreign data wrapper fdw options (host 'x')",
    ) else {
        panic!("expected server");
    };
    assert_eq!(server.servername.as_deref(), Some("s"));
    assert_eq!(server.fdwname.as_deref(), Some("fdw"));
    assert!(server.if_not_exists);
    assert!(!server.options.is_empty());

    let Node::CreatePolicyStmt(policy) =
        first_node("create policy p on t for select to r using (id > 0) with check (id > 0)")
    else {
        panic!("expected policy");
    };
    assert_eq!(policy.policy_name.as_deref(), Some("p"));
    assert!(policy.table.is_some());
    assert!(policy.qual.is_some());
    assert!(policy.with_check.is_some());

    let Node::AlterPolicyStmt(policy) = first_node("alter policy p on t to r using (id > 1)")
    else {
        panic!("expected alter policy");
    };
    assert_eq!(policy.policy_name.as_deref(), Some("p"));
    assert!(policy.table.is_some());
    assert!(policy.qual.is_some());

    let Node::SelectStmt(select) = first_node(
        "select * from (select 1) s join f(1) g on true window w as (partition by a order by b) order by a fetch first 2 rows with ties for update of s nowait",
    ) else {
        panic!("expected select");
    };
    assert!(matches!(
        select.from_clause.first(),
        Some(Node::JoinExpr(_))
    ));
    assert!(!select.window_clause.is_empty());
    assert!(!select.locking_clause.is_empty());
    assert_eq!(select.limit_option, LimitOption::WithTies);

    let Node::AlterTableStmt(alter) = first_node(
        "alter table t add column c int, alter column c set default 1, drop column if exists d cascade",
    ) else {
        panic!("expected alter table");
    };
    assert_eq!(alter.cmds.len(), 3);
    assert!(matches!(
        alter.cmds.first(),
        Some(Node::AlterTableCmd(AlterTableCmd {
            subtype: AlterTableType::AddColumn,
            ..
        }))
    ));
}
