use pg_parser::NodeTag;

use super::common::{StatementCase, assert_statement_cases};

pub const CASES: &[StatementCase] = &[
    StatementCase {
        expected: NodeTag::AlterCollationStmt,
        sql: "alter collation c refresh version",
    },
    StatementCase {
        expected: NodeTag::AlterDatabaseRefreshCollStmt,
        sql: "alter database d refresh collation version",
    },
    StatementCase {
        expected: NodeTag::AlterDatabaseSetStmt,
        sql: "alter database d set search_path to public",
    },
    StatementCase {
        expected: NodeTag::AlterDatabaseStmt,
        sql: "alter database d allow_connections true",
    },
    StatementCase {
        expected: NodeTag::AlterDefaultPrivilegesStmt,
        sql: "alter default privileges grant select on tables to r",
    },
    StatementCase {
        expected: NodeTag::AlterDomainStmt,
        sql: "alter domain d set default 1",
    },
    StatementCase {
        expected: NodeTag::AlterEnumStmt,
        sql: "alter type mood add value 'ok'",
    },
    StatementCase {
        expected: NodeTag::AlterEventTrigStmt,
        sql: "alter event trigger et disable",
    },
    StatementCase {
        expected: NodeTag::AlterExtensionContentsStmt,
        sql: "alter extension e add table t",
    },
    StatementCase {
        expected: NodeTag::AlterExtensionStmt,
        sql: "alter extension e update",
    },
    StatementCase {
        expected: NodeTag::AlterFdwStmt,
        sql: "alter foreign data wrapper fdw options (foo 'bar')",
    },
    StatementCase {
        expected: NodeTag::AlterForeignServerStmt,
        sql: "alter server s options (foo 'bar')",
    },
    StatementCase {
        expected: NodeTag::AlterFunctionStmt,
        sql: "alter function f() stable",
    },
    StatementCase {
        expected: NodeTag::AlterObjectDependsStmt,
        sql: "alter function f() depends on extension e",
    },
    StatementCase {
        expected: NodeTag::AlterObjectSchemaStmt,
        sql: "alter table t set schema s",
    },
    StatementCase {
        expected: NodeTag::AlterOpFamilyStmt,
        sql: "alter operator family opf using btree add operator 1 =(int,int)",
    },
    StatementCase {
        expected: NodeTag::AlterOperatorStmt,
        sql: "alter operator +(int, int) set (commutator = +)",
    },
    StatementCase {
        expected: NodeTag::AlterOwnerStmt,
        sql: "alter schema s owner to r",
    },
    StatementCase {
        expected: NodeTag::AlterPolicyStmt,
        sql: "alter policy p on t using (true)",
    },
    StatementCase {
        expected: NodeTag::AlterPropGraphStmt,
        sql: "alter property graph g add vertex tables (t)",
    },
    StatementCase {
        expected: NodeTag::AlterPublicationStmt,
        sql: "alter publication p set table t",
    },
    StatementCase {
        expected: NodeTag::AlterRoleSetStmt,
        sql: "alter role r set search_path to public",
    },
    StatementCase {
        expected: NodeTag::AlterRoleStmt,
        sql: "alter group g add user u",
    },
    StatementCase {
        expected: NodeTag::AlterSeqStmt,
        sql: "alter sequence s restart",
    },
    StatementCase {
        expected: NodeTag::AlterStatsStmt,
        sql: "alter statistics st set statistics 10",
    },
    StatementCase {
        expected: NodeTag::AlterSubscriptionStmt,
        sql: "alter subscription s refresh publication",
    },
    StatementCase {
        expected: NodeTag::AlterSystemStmt,
        sql: "alter system set work_mem = '4MB'",
    },
    StatementCase {
        expected: NodeTag::AlterTableMoveAllStmt,
        sql: "alter table all in tablespace old_space set tablespace new_space",
    },
    StatementCase {
        expected: NodeTag::AlterTableSpaceOptionsStmt,
        sql: "alter tablespace ts set (random_page_cost = 2)",
    },
    StatementCase {
        expected: NodeTag::AlterTableStmt,
        sql: "alter table t add column c int",
    },
    StatementCase {
        expected: NodeTag::AlterTsConfigurationStmt,
        sql: "alter text search configuration c add mapping for asciiword with simple",
    },
    StatementCase {
        expected: NodeTag::AlterTsDictionaryStmt,
        sql: "alter text search dictionary d (template = simple)",
    },
    StatementCase {
        expected: NodeTag::AlterTypeStmt,
        sql: "alter type t set (receive = r)",
    },
    StatementCase {
        expected: NodeTag::AlterUserMappingStmt,
        sql: "alter user mapping for u server s options (foo 'bar')",
    },
    StatementCase {
        expected: NodeTag::CallStmt,
        sql: "call f(1)",
    },
    StatementCase {
        expected: NodeTag::CheckPointStmt,
        sql: "checkpoint",
    },
    StatementCase {
        expected: NodeTag::ClosePortalStmt,
        sql: "close c",
    },
    StatementCase {
        expected: NodeTag::CommentStmt,
        sql: "comment on table t is 'x'",
    },
    StatementCase {
        expected: NodeTag::CompositeTypeStmt,
        sql: "create type ct as (a int)",
    },
    StatementCase {
        expected: NodeTag::ConstraintsSetStmt,
        sql: "set constraints all deferred",
    },
    StatementCase {
        expected: NodeTag::CopyStmt,
        sql: "copy t from 'file.csv'",
    },
    StatementCase {
        expected: NodeTag::CreateAmStmt,
        sql: "create access method am type table handler h",
    },
    StatementCase {
        expected: NodeTag::CreateCastStmt,
        sql: "create cast (int as text) without function",
    },
    StatementCase {
        expected: NodeTag::CreateConversionStmt,
        sql: "create conversion conv for 'utf8' to 'latin1' from f",
    },
    StatementCase {
        expected: NodeTag::CreateDomainStmt,
        sql: "create domain d as int",
    },
    StatementCase {
        expected: NodeTag::CreateEnumStmt,
        sql: "create type mood as enum ('sad','ok')",
    },
    StatementCase {
        expected: NodeTag::CreateEventTrigStmt,
        sql: "create event trigger et on ddl_command_start execute function f()",
    },
    StatementCase {
        expected: NodeTag::CreateExtensionStmt,
        sql: "create extension e",
    },
    StatementCase {
        expected: NodeTag::CreateFdwStmt,
        sql: "create foreign data wrapper fdw",
    },
    StatementCase {
        expected: NodeTag::CreateForeignServerStmt,
        sql: "create server if not exists s type 't' version '1' foreign data wrapper fdw options (host 'x')",
    },
    StatementCase {
        expected: NodeTag::CreateForeignTableStmt,
        sql: "create foreign table ft (id int) server s",
    },
    StatementCase {
        expected: NodeTag::CreateFunctionStmt,
        sql: "create function f() returns int language sql as 'select 1'",
    },
    StatementCase {
        expected: NodeTag::CreateOpClassStmt,
        sql: "create operator class opc for type int using btree as operator 1 =",
    },
    StatementCase {
        expected: NodeTag::CreateOpFamilyStmt,
        sql: "create operator family opf using btree",
    },
    StatementCase {
        expected: NodeTag::CreatePLangStmt,
        sql: "create language plpgsql handler plpgsql_call_handler",
    },
    StatementCase {
        expected: NodeTag::CreatePolicyStmt,
        sql: "create policy p on t using (true)",
    },
    StatementCase {
        expected: NodeTag::CreatePropGraphStmt,
        sql: "create property graph g vertex tables (t)",
    },
    StatementCase {
        expected: NodeTag::CreatePublicationStmt,
        sql: "create publication p",
    },
    StatementCase {
        expected: NodeTag::CreateRangeStmt,
        sql: "create type int4range as range (subtype = int4)",
    },
    StatementCase {
        expected: NodeTag::CreateRoleStmt,
        sql: "create role r",
    },
    StatementCase {
        expected: NodeTag::CreateSchemaStmt,
        sql: "create schema s",
    },
    StatementCase {
        expected: NodeTag::CreateSeqStmt,
        sql: "create sequence s",
    },
    StatementCase {
        expected: NodeTag::CreateStatsStmt,
        sql: "create statistics st on a from t",
    },
    StatementCase {
        expected: NodeTag::CreateStmt,
        sql: "create table t (id int)",
    },
    StatementCase {
        expected: NodeTag::CreateSubscriptionStmt,
        sql: "create subscription s connection 'x' publication p",
    },
    StatementCase {
        expected: NodeTag::CreateTableAsStmt,
        sql: "create materialized view mv as select 1",
    },
    StatementCase {
        expected: NodeTag::CreateTableSpaceStmt,
        sql: "create tablespace ts location '/tmp'",
    },
    StatementCase {
        expected: NodeTag::CreateTransformStmt,
        sql: "create transform for int language plpgsql (from sql with function f(int))",
    },
    StatementCase {
        expected: NodeTag::CreateTrigStmt,
        sql: "create trigger tr before insert on t execute function f()",
    },
    StatementCase {
        expected: NodeTag::CreateUserMappingStmt,
        sql: "create user mapping for u server s",
    },
    StatementCase {
        expected: NodeTag::CreatedbStmt,
        sql: "create database d",
    },
    StatementCase {
        expected: NodeTag::DeallocateStmt,
        sql: "deallocate q",
    },
    StatementCase {
        expected: NodeTag::DeclareCursorStmt,
        sql: "declare c cursor for select 1",
    },
    StatementCase {
        expected: NodeTag::DefineStmt,
        sql: "create aggregate agg(int) (sfunc = f, stype = int)",
    },
    StatementCase {
        expected: NodeTag::DeleteStmt,
        sql: "delete from t where id = 1",
    },
    StatementCase {
        expected: NodeTag::DiscardStmt,
        sql: "discard all",
    },
    StatementCase {
        expected: NodeTag::DoStmt,
        sql: "do 'begin end'",
    },
    StatementCase {
        expected: NodeTag::DropOwnedStmt,
        sql: "drop owned by r",
    },
    StatementCase {
        expected: NodeTag::DropRoleStmt,
        sql: "drop role if exists r",
    },
    StatementCase {
        expected: NodeTag::DropStmt,
        sql: "drop aggregate if exists agg(int)",
    },
    StatementCase {
        expected: NodeTag::DropSubscriptionStmt,
        sql: "drop subscription if exists sub",
    },
    StatementCase {
        expected: NodeTag::DropTableSpaceStmt,
        sql: "drop tablespace if exists ts",
    },
    StatementCase {
        expected: NodeTag::DropUserMappingStmt,
        sql: "drop user mapping if exists for u server s",
    },
    StatementCase {
        expected: NodeTag::DropdbStmt,
        sql: "drop database if exists d",
    },
    StatementCase {
        expected: NodeTag::ExecuteStmt,
        sql: "execute q",
    },
    StatementCase {
        expected: NodeTag::ExplainStmt,
        sql: "explain select 1",
    },
    StatementCase {
        expected: NodeTag::FetchStmt,
        sql: "fetch next from c",
    },
    StatementCase {
        expected: NodeTag::GrantRoleStmt,
        sql: "grant r to u",
    },
    StatementCase {
        expected: NodeTag::GrantStmt,
        sql: "grant select on table t to r",
    },
    StatementCase {
        expected: NodeTag::ImportForeignSchemaStmt,
        sql: "import foreign schema s from server srv into public",
    },
    StatementCase {
        expected: NodeTag::IndexStmt,
        sql: "create index idx on t (id)",
    },
    StatementCase {
        expected: NodeTag::InsertStmt,
        sql: "insert into t values (1)",
    },
    StatementCase {
        expected: NodeTag::ListenStmt,
        sql: "listen ch",
    },
    StatementCase {
        expected: NodeTag::LoadStmt,
        sql: "load 'x'",
    },
    StatementCase {
        expected: NodeTag::LockStmt,
        sql: "lock table t",
    },
    StatementCase {
        expected: NodeTag::MergeStmt,
        sql: "merge into t using s on t.id = s.id when matched then update set id = s.id",
    },
    StatementCase {
        expected: NodeTag::NotifyStmt,
        sql: "notify ch, 'payload'",
    },
    StatementCase {
        expected: NodeTag::PrepareStmt,
        sql: "prepare q as select 1",
    },
    StatementCase {
        expected: NodeTag::ReassignOwnedStmt,
        sql: "reassign owned by r to u",
    },
    StatementCase {
        expected: NodeTag::RefreshMatViewStmt,
        sql: "refresh materialized view mv",
    },
    StatementCase {
        expected: NodeTag::ReindexStmt,
        sql: "reindex table t",
    },
    StatementCase {
        expected: NodeTag::RenameStmt,
        sql: "alter table t rename to t2",
    },
    StatementCase {
        expected: NodeTag::RepackStmt,
        sql: "repack t using index idx",
    },
    StatementCase {
        expected: NodeTag::RuleStmt,
        sql: "create rule r as on update to t do notify ch",
    },
    StatementCase {
        expected: NodeTag::SecLabelStmt,
        sql: "security label on table t is 'x'",
    },
    StatementCase {
        expected: NodeTag::SelectStmt,
        sql: "select 1",
    },
    StatementCase {
        expected: NodeTag::TransactionStmt,
        sql: "begin",
    },
    StatementCase {
        expected: NodeTag::TruncateStmt,
        sql: "truncate table t",
    },
    StatementCase {
        expected: NodeTag::UnlistenStmt,
        sql: "unlisten *",
    },
    StatementCase {
        expected: NodeTag::UpdateStmt,
        sql: "update t set id = 2",
    },
    StatementCase {
        expected: NodeTag::VacuumStmt,
        sql: "analyze t",
    },
    StatementCase {
        expected: NodeTag::VariableSetStmt,
        sql: "reset search_path",
    },
    StatementCase {
        expected: NodeTag::VariableShowStmt,
        sql: "show search_path",
    },
    StatementCase {
        expected: NodeTag::ViewStmt,
        sql: "create view v as select 1",
    },
    StatementCase {
        expected: NodeTag::WaitStmt,
        sql: "wait for lsn '0/0'",
    },
];

#[test]
fn registered_top_level_statements_parse_to_expected_tags() {
    assert_statement_cases(CASES);
}
