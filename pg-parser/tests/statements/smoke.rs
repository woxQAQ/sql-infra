use pg_parser::Node;

use super::common::StatementCase;
use super::common::assert_statement_cases;

pub const CASES: &[StatementCase] = &[
    StatementCase {
        expected: |node| matches!(node, Node::AlterCollationStmt(_)),
        sql: "alter collation c refresh version",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterDatabaseRefreshCollStmt(_)),
        sql: "alter database d refresh collation version",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterDatabaseSetStmt(_)),
        sql: "alter database d set search_path to public",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterDatabaseStmt(_)),
        sql: "alter database d allow_connections true",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterDefaultPrivilegesStmt(_)),
        sql: "alter default privileges grant select on tables to r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterDomainStmt(_)),
        sql: "alter domain d set default 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterEnumStmt(_)),
        sql: "alter type mood add value 'ok'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterEventTrigStmt(_)),
        sql: "alter event trigger et disable",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterExtensionContentsStmt(_)),
        sql: "alter extension e add table t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterExtensionStmt(_)),
        sql: "alter extension e update",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterFdwStmt(_)),
        sql: "alter foreign data wrapper fdw options (foo 'bar')",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterForeignServerStmt(_)),
        sql: "alter server s options (foo 'bar')",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterFunctionStmt(_)),
        sql: "alter function f() stable",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterObjectDependsStmt(_)),
        sql: "alter function f() depends on extension e",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterObjectSchemaStmt(_)),
        sql: "alter table t set schema s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterOpFamilyStmt(_)),
        sql: "alter operator family opf using btree add operator 1 =(int,int)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterOperatorStmt(_)),
        sql: "alter operator +(int, int) set (commutator = +)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterOwnerStmt(_)),
        sql: "alter schema s owner to r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterPolicyStmt(_)),
        sql: "alter policy p on t using (true)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterPropGraphStmt(_)),
        sql: "alter property graph g add vertex tables (t)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterPublicationStmt(_)),
        sql: "alter publication p set table t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterRoleSetStmt(_)),
        sql: "alter role r set search_path to public",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterRoleStmt(_)),
        sql: "alter group g add user u",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterSeqStmt(_)),
        sql: "alter sequence s restart",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterStatsStmt(_)),
        sql: "alter statistics st set statistics 10",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterSubscriptionStmt(_)),
        sql: "alter subscription s refresh publication",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterSystemStmt(_)),
        sql: "alter system set work_mem = '4MB'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTableMoveAllStmt(_)),
        sql: "alter table all in tablespace old_space set tablespace new_space",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTableSpaceOptionsStmt(_)),
        sql: "alter tablespace ts set (random_page_cost = 2)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTableStmt(_)),
        sql: "alter table t add column c int",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTsConfigurationStmt(_)),
        sql: "alter text search configuration c add mapping for asciiword with simple",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTsDictionaryStmt(_)),
        sql: "alter text search dictionary d (template = simple)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterTypeStmt(_)),
        sql: "alter type t set (receive = r)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::AlterUserMappingStmt(_)),
        sql: "alter user mapping for u server s options (foo 'bar')",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CallStmt(_)),
        sql: "call f(1)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CheckPointStmt(_)),
        sql: "checkpoint",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ClosePortalStmt(_)),
        sql: "close c",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CommentStmt(_)),
        sql: "comment on table t is 'x'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CompositeTypeStmt(_)),
        sql: "create type ct as (a int)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ConstraintsSetStmt(_)),
        sql: "set constraints all deferred",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CopyStmt(_)),
        sql: "copy t from 'file.csv'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateAmStmt(_)),
        sql: "create access method am type table handler h",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateCastStmt(_)),
        sql: "create cast (int as text) without function",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateConversionStmt(_)),
        sql: "create conversion conv for 'utf8' to 'latin1' from f",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateDomainStmt(_)),
        sql: "create domain d as int",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateEnumStmt(_)),
        sql: "create type mood as enum ('sad','ok')",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateEventTrigStmt(_)),
        sql: "create event trigger et on ddl_command_start execute function f()",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateExtensionStmt(_)),
        sql: "create extension e",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateFdwStmt(_)),
        sql: "create foreign data wrapper fdw",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateForeignServerStmt(_)),
        sql: "create server if not exists s type 't' version '1' foreign data wrapper fdw options (host 'x')",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateForeignTableStmt(_)),
        sql: "create foreign table ft (id int) server s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateFunctionStmt(_)),
        sql: "create function f() returns int language sql as 'select 1'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateOpClassStmt(_)),
        sql: "create operator class opc for type int using btree as operator 1 =",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateOpFamilyStmt(_)),
        sql: "create operator family opf using btree",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreatePLangStmt(_)),
        sql: "create language plpgsql handler plpgsql_call_handler",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreatePolicyStmt(_)),
        sql: "create policy p on t using (true)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreatePropGraphStmt(_)),
        sql: "create property graph g vertex tables (t)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreatePublicationStmt(_)),
        sql: "create publication p",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateRangeStmt(_)),
        sql: "create type int4range as range (subtype = int4)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateRoleStmt(_)),
        sql: "create role r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateSchemaStmt(_)),
        sql: "create schema s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateSeqStmt(_)),
        sql: "create sequence s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateStatsStmt(_)),
        sql: "create statistics st on a from t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateStmt(_)),
        sql: "create table t (id int)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateSubscriptionStmt(_)),
        sql: "create subscription s connection 'x' publication p",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateTableAsStmt(_)),
        sql: "create materialized view mv as select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateTableSpaceStmt(_)),
        sql: "create tablespace ts location '/tmp'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateTransformStmt(_)),
        sql: "create transform for int language plpgsql (from sql with function f(int))",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateTrigStmt(_)),
        sql: "create trigger tr before insert on t execute function f()",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreateUserMappingStmt(_)),
        sql: "create user mapping for u server s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::CreatedbStmt(_)),
        sql: "create database d",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DeallocateStmt(_)),
        sql: "deallocate q",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DeclareCursorStmt(_)),
        sql: "declare c cursor for select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DefineStmt(_)),
        sql: "create aggregate agg(int) (sfunc = f, stype = int)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DeleteStmt(_)),
        sql: "delete from t where id = 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DiscardStmt(_)),
        sql: "discard all",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DoStmt(_)),
        sql: "do 'begin end'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropOwnedStmt(_)),
        sql: "drop owned by r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropRoleStmt(_)),
        sql: "drop role if exists r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropStmt(_)),
        sql: "drop aggregate if exists agg(int)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropSubscriptionStmt(_)),
        sql: "drop subscription if exists sub",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropTableSpaceStmt(_)),
        sql: "drop tablespace if exists ts",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropUserMappingStmt(_)),
        sql: "drop user mapping if exists for u server s",
    },
    StatementCase {
        expected: |node| matches!(node, Node::DropdbStmt(_)),
        sql: "drop database if exists d",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ExecuteStmt(_)),
        sql: "execute q",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ExplainStmt(_)),
        sql: "explain select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::FetchStmt(_)),
        sql: "fetch next from c",
    },
    StatementCase {
        expected: |node| matches!(node, Node::GrantRoleStmt(_)),
        sql: "grant r to u",
    },
    StatementCase {
        expected: |node| matches!(node, Node::GrantStmt(_)),
        sql: "grant select on table t to r",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ImportForeignSchemaStmt(_)),
        sql: "import foreign schema s from server srv into public",
    },
    StatementCase {
        expected: |node| matches!(node, Node::IndexStmt(_)),
        sql: "create index idx on t (id)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::InsertStmt(_)),
        sql: "insert into t values (1)",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ListenStmt(_)),
        sql: "listen ch",
    },
    StatementCase {
        expected: |node| matches!(node, Node::LoadStmt(_)),
        sql: "load 'x'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::LockStmt(_)),
        sql: "lock table t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::MergeStmt(_)),
        sql: "merge into t using s on t.id = s.id when matched then update set id = s.id",
    },
    StatementCase {
        expected: |node| matches!(node, Node::NotifyStmt(_)),
        sql: "notify ch, 'payload'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::PrepareStmt(_)),
        sql: "prepare q as select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ReassignOwnedStmt(_)),
        sql: "reassign owned by r to u",
    },
    StatementCase {
        expected: |node| matches!(node, Node::RefreshMatViewStmt(_)),
        sql: "refresh materialized view mv",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ReindexStmt(_)),
        sql: "reindex table t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::RenameStmt(_)),
        sql: "alter table t rename to t2",
    },
    StatementCase {
        expected: |node| matches!(node, Node::RepackStmt(_)),
        sql: "repack t using index idx",
    },
    StatementCase {
        expected: |node| matches!(node, Node::RuleStmt(_)),
        sql: "create rule r as on update to t do notify ch",
    },
    StatementCase {
        expected: |node| matches!(node, Node::SecLabelStmt(_)),
        sql: "security label on table t is 'x'",
    },
    StatementCase {
        expected: |node| matches!(node, Node::SelectStmt(_)),
        sql: "select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::TransactionStmt(_)),
        sql: "begin",
    },
    StatementCase {
        expected: |node| matches!(node, Node::TruncateStmt(_)),
        sql: "truncate table t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::UnlistenStmt(_)),
        sql: "unlisten *",
    },
    StatementCase {
        expected: |node| matches!(node, Node::UpdateStmt(_)),
        sql: "update t set id = 2",
    },
    StatementCase {
        expected: |node| matches!(node, Node::VacuumStmt(_)),
        sql: "analyze t",
    },
    StatementCase {
        expected: |node| matches!(node, Node::VariableSetStmt(_)),
        sql: "reset search_path",
    },
    StatementCase {
        expected: |node| matches!(node, Node::VariableShowStmt(_)),
        sql: "show search_path",
    },
    StatementCase {
        expected: |node| matches!(node, Node::ViewStmt(_)),
        sql: "create view v as select 1",
    },
    StatementCase {
        expected: |node| matches!(node, Node::WaitStmt(_)),
        sql: "wait for lsn '0/0'",
    },
];

#[test]
fn every_top_level_raw_statement_has_a_smoke_case() {
    assert_statement_cases(CASES);
}
