use pg_parser::{
    CmdType, ConstrType, FunctionParameterMode, Node, NodeTag, PartitionStrategy,
    PropGraphProperties, TableLikeOption, VariableSetKind, ViewCheckOption,
};

use super::common::parse_statement;

#[test]
fn create_regular_and_foreign_tables_accept_empty_optional_element_lists() {
    let Node::CreateStmt(regular) = parse_statement("create table empty_table ()") else {
        panic!("expected regular CreateStmt");
    };
    assert!(regular.table_elts.is_empty());

    let Node::CreateForeignTableStmt(foreign) =
        parse_statement("create foreign table empty_foreign () server foreign_server")
    else {
        panic!("expected CreateForeignTableStmt");
    };
    assert!(foreign.base.table_elts.is_empty());

    let Node::CreateStmt(typed) = parse_statement("create table typed of app.item_type") else {
        panic!("expected typed CreateStmt");
    };
    assert!(typed.table_elts.is_empty());
    assert!(typed.of_typename.is_some());
}

#[test]
fn create_query_statements_accept_the_grammar_valid_empty_select() {
    let Node::ViewStmt(view) = parse_statement("create view empty_view as select") else {
        panic!("expected ViewStmt");
    };
    assert!(matches!(
        view.query.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));

    for sql in [
        "create table empty_ctas as select",
        "create materialized view empty_matview as select",
    ] {
        let Node::CreateTableAsStmt(stmt) = parse_statement(sql) else {
            panic!("expected CreateTableAsStmt for {sql}");
        };
        assert!(matches!(
            stmt.query.as_deref(),
            Some(Node::SelectStmt(select)) if select.target_list.is_empty()
        ));
    }
}

#[test]
fn create_function_stmt_populates_parameters_return_type_and_options() {
    let sql = "create or replace function app.compute(a int, in label text default 'x', out total bigint) returns int language sql immutable strict security definer parallel safe cost 10 rows 1 as 'select 1'";
    let Node::CreateFunctionStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(stmt.replace);
    assert!(!stmt.is_procedure);
    assert_eq!(stmt.funcname.len(), 2);
    assert_eq!(stmt.parameters.len(), 3);
    assert!(stmt.return_type.is_some());
    assert_eq!(stmt.options.len(), 8);

    let Node::FunctionParameter(first) = &stmt.parameters[0] else {
        panic!("expected FunctionParameter");
    };
    assert_eq!(first.name.as_deref(), Some("a"));
    assert_eq!(first.mode, FunctionParameterMode::Default);
    assert_eq!(first.location as usize, sql.find("a int").unwrap());

    let Node::FunctionParameter(with_default) = &stmt.parameters[1] else {
        panic!("expected FunctionParameter");
    };
    assert_eq!(with_default.name.as_deref(), Some("label"));
    assert_eq!(with_default.mode, FunctionParameterMode::In);
    assert!(with_default.defexpr.is_some());
    assert_eq!(
        with_default.location as usize,
        sql.find("in label").unwrap()
    );

    let Node::FunctionParameter(output) = &stmt.parameters[2] else {
        panic!("expected FunctionParameter");
    };
    assert_eq!(output.name.as_deref(), Some("total"));
    assert_eq!(output.mode, FunctionParameterMode::Out);
    assert_eq!(output.location as usize, sql.find("out total").unwrap());
}

#[test]
fn create_procedure_uses_procedure_specific_options() {
    let Node::CreateFunctionStmt(stmt) = parse_statement(
        "create or replace procedure app.refresh(in target text, inout changed integer)
         language sql security definer set search_path to app
         begin atomic
           insert into audit_log values (target);
         end",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(stmt.is_procedure);
    assert!(stmt.replace);
    assert_eq!(stmt.funcname.len(), 2);
    assert_eq!(stmt.parameters.len(), 2);
    assert!(stmt.return_type.is_none());
    assert_eq!(stmt.options.len(), 3);
    assert!(stmt.sql_body.is_some());
}

#[test]
fn create_function_stmt_can_store_a_sql_return_body() {
    let Node::CreateFunctionStmt(stmt) =
        parse_statement("create function increment(a int) returns int return a + 1")
    else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(matches!(
        stmt.sql_body.as_deref(),
        Some(Node::ReturnStmt(_))
    ));
}

#[test]
fn create_function_returns_table_merges_output_columns_and_builds_return_type() {
    let sql = "create function app.expand(prefix text) returns table (id bigint, label text) language sql as 'select 1, prefix'";
    let Node::CreateFunctionStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateFunctionStmt");
    };
    assert_eq!(stmt.parameters.len(), 3);
    for (index, expected_name) in [(1, "id"), (2, "label")] {
        let Node::FunctionParameter(column) = &stmt.parameters[index] else {
            panic!("expected table FunctionParameter");
        };
        assert_eq!(column.name.as_deref(), Some(expected_name));
        assert_eq!(column.mode, FunctionParameterMode::Table);
        assert!(column.arg_type.is_some());
        assert_eq!(column.location as usize, sql.find(expected_name).unwrap());
    }
    let return_type = stmt.return_type.as_deref().expect("table return TypeName");
    assert!(return_type.setof);
    assert!(matches!(
        return_type.names.as_slice(),
        [Node::String(catalog), Node::String(name)]
            if catalog.sval.as_deref() == Some("pg_catalog")
                && name.sval.as_deref() == Some("record")
    ));

    let Node::CreateFunctionStmt(single) = parse_statement(
        "create function app.amount() returns table (value numeric(12, 2)) language sql as 'select 1'",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    let return_type = single
        .return_type
        .as_deref()
        .expect("single table return type");
    assert!(return_type.setof);
    assert_eq!(return_type.typmods.len(), 2);
    assert!(matches!(
        return_type.names.last(),
        Some(Node::String(name)) if name.sval.as_deref() == Some("numeric")
    ));

    let Node::CreateFunctionStmt(quoted) = parse_statement(
        "create function app.quoted_table() returns table (\"select\" integer) language sql as 'select 1'",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(matches!(
        quoted.parameters.as_slice(),
        [Node::FunctionParameter(column)] if column.name.as_deref() == Some("select")
    ));
}

#[test]
fn create_function_populates_all_common_and_create_only_options() {
    let Node::CreateFunctionStmt(stmt) = parse_statement(
        "create function app.optioned(a integer) returns integer
         called on null input returns null on null input
         external security invoker leakproof not leakproof
         transform for type integer, for type text window
         set work_mem to '4MB' reset search_path
         language sql as 'select a'",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    let option = |name: &str, occurrence: usize| {
        stmt.options
            .iter()
            .filter_map(|node| match node {
                Node::DefElem(option) if option.defname.as_deref() == Some(name) => Some(option),
                _ => None,
            })
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {name} option {occurrence}"))
    };
    assert!(matches!(
        option("strict", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if !value.boolval
    ));
    assert!(matches!(
        option("strict", 1).arg.as_deref(),
        Some(Node::Boolean(value)) if value.boolval
    ));
    assert!(matches!(
        option("security", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if !value.boolval
    ));
    let Some(Node::AArrayExpr(transforms)) = option("transform", 0).arg.as_deref() else {
        panic!("expected transform type list");
    };
    assert_eq!(transforms.elements.len(), 2);
    assert!(
        transforms
            .elements
            .iter()
            .all(|node| matches!(node, Node::TypeName(_)))
    );
    assert!(matches!(
        option("window", 0).arg.as_deref(),
        Some(Node::Boolean(value)) if value.boolval
    ));
    let Some(Node::VariableSetStmt(set)) = option("set", 0).arg.as_deref() else {
        panic!("expected SET VariableSetStmt");
    };
    assert_eq!(set.kind, VariableSetKind::SetValue);
    assert_eq!(set.name.as_deref(), Some("work_mem"));
    let Some(Node::VariableSetStmt(reset)) = option("set", 1).arg.as_deref() else {
        panic!("expected RESET VariableSetStmt");
    };
    assert_eq!(reset.kind, VariableSetKind::Reset);
    assert_eq!(reset.name.as_deref(), Some("search_path"));
}

#[test]
fn create_function_begin_atomic_preserves_nested_statement_list() {
    let Node::CreateFunctionStmt(stmt) = parse_statement(
        "create function app.atomic_body(a integer) returns integer
         begin atomic
           insert into audit_log values (a);
           return a + 1;
         end",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    let Some(Node::AArrayExpr(outer)) = stmt.sql_body.as_deref() else {
        panic!("expected outer routine body list");
    };
    let [Node::AArrayExpr(inner)] = outer.elements.as_slice() else {
        panic!("expected nested routine statement list");
    };
    assert!(matches!(
        inner.elements.as_slice(),
        [Node::InsertStmt(_), Node::ReturnStmt(_)]
    ));
    let Node::ReturnStmt(return_stmt) = &inner.elements[1] else {
        panic!("expected ReturnStmt");
    };
    assert!(matches!(
        return_stmt.returnval.as_deref(),
        Some(Node::AExpr(_))
    ));
}

#[test]
fn create_function_preserves_in_out_variadic_defaults_and_percent_type() {
    let sql = "create function app.typed(
             in out first app.source.value%type,
             second inout text,
             variadic rest integer[],
             fallback integer = 42
         ) returns setof app.source.value%type language sql as 'select first'";
    let Node::CreateFunctionStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateFunctionStmt");
    };
    let parameter = |index: usize| {
        let Node::FunctionParameter(parameter) = &stmt.parameters[index] else {
            panic!("expected FunctionParameter");
        };
        parameter
    };
    assert_eq!(parameter(0).name.as_deref(), Some("first"));
    assert_eq!(parameter(0).mode, FunctionParameterMode::Inout);
    assert!(
        parameter(0)
            .arg_type
            .as_deref()
            .expect("first type")
            .pct_type
    );
    assert_eq!(parameter(1).mode, FunctionParameterMode::Inout);
    assert_eq!(parameter(2).mode, FunctionParameterMode::Variadic);
    assert_eq!(
        parameter(2)
            .arg_type
            .as_deref()
            .expect("variadic type")
            .array_bounds
            .len(),
        1
    );
    assert!(parameter(3).defexpr.is_some());

    let return_type = stmt.return_type.as_deref().expect("return type");
    assert!(return_type.pct_type);
    assert!(return_type.setof);
    assert_eq!(return_type.names.len(), 3);
    assert_eq!(
        return_type.location as usize,
        sql.rfind("app.source.value%type").unwrap()
    );

    let alternatives_sql = "create function app.parameter_alternatives(
        in named integer,
        named_mode inout text,
        plain bigint,
        in integer,
        text
    ) returns integer language sql as 'select 1'";
    let Node::CreateFunctionStmt(alternatives) = parse_statement(alternatives_sql) else {
        panic!("expected CreateFunctionStmt");
    };
    let parameter = |index: usize| {
        let Node::FunctionParameter(parameter) = &alternatives.parameters[index] else {
            panic!("expected FunctionParameter");
        };
        parameter
    };
    assert_eq!(parameter(0).name.as_deref(), Some("named"));
    assert_eq!(parameter(0).mode, FunctionParameterMode::In);
    assert_eq!(parameter(1).name.as_deref(), Some("named_mode"));
    assert_eq!(parameter(1).mode, FunctionParameterMode::Inout);
    assert_eq!(parameter(2).name.as_deref(), Some("plain"));
    assert_eq!(parameter(2).mode, FunctionParameterMode::Default);
    assert!(parameter(3).name.is_none());
    assert_eq!(parameter(3).mode, FunctionParameterMode::In);
    assert!(parameter(4).name.is_none());
    assert_eq!(parameter(4).mode, FunctionParameterMode::Default);
    for (index, needle) in [
        (0, "in named"),
        (1, "named_mode"),
        (2, "plain bigint"),
        (3, "in integer"),
    ] {
        assert_eq!(
            parameter(index).location as usize,
            alternatives_sql.find(needle).unwrap(),
            "parameter alternative {index}"
        );
    }
    assert_eq!(
        parameter(4).location as usize,
        alternatives_sql.rfind("text").unwrap()
    );
}

#[test]
fn create_function_preserves_special_set_and_reset_options() {
    let sql = "create function app.configured() returns integer
         set time zone 'UTC'
         set schema 'app'
         set names 'UTF8'
         set role app_user
         set session authorization default
         set xml option document
         set transaction snapshot '00000003-0000001B-1'
         reset time zone
         reset transaction isolation level
         reset session authorization
         language sql as 'select 1'";
    let Node::CreateFunctionStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateFunctionStmt");
    };
    let settings = stmt
        .options
        .iter()
        .filter_map(|node| match node {
            Node::DefElem(option) if option.defname.as_deref() == Some("set") => {
                match option.arg.as_deref() {
                    Some(Node::VariableSetStmt(setting)) => Some(setting),
                    other => panic!("expected VariableSetStmt, got {other:?}"),
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(settings.len(), 10);
    assert_eq!(settings[0].name.as_deref(), Some("timezone"));
    assert!(settings[0].jumble_args);
    assert_eq!(settings[1].name.as_deref(), Some("search_path"));
    assert_eq!(settings[2].name.as_deref(), Some("client_encoding"));
    assert_eq!(settings[3].name.as_deref(), Some("role"));
    assert_eq!(settings[4].kind, VariableSetKind::SetDefault);
    assert_eq!(settings[4].name.as_deref(), Some("session_authorization"));
    assert_eq!(settings[5].name.as_deref(), Some("xmloption"));
    assert_eq!(settings[6].kind, VariableSetKind::SetMulti);
    assert_eq!(settings[6].name.as_deref(), Some("TRANSACTION SNAPSHOT"));
    assert_eq!(settings[7].name.as_deref(), Some("timezone"));
    assert_eq!(settings[8].name.as_deref(), Some("transaction_isolation"));
    assert_eq!(settings[9].name.as_deref(), Some("session_authorization"));
    assert_eq!(settings[0].location, -1);
    assert_eq!(settings[1].location, sql.find("'app'").unwrap() as i32);
    assert_eq!(settings[2].location, sql.find("'UTF8'").unwrap() as i32);
    assert_eq!(settings[3].location, sql.find("app_user").unwrap() as i32);
    assert_eq!(settings[4].location, -1);
    assert_eq!(settings[5].location, -1);
    assert_eq!(
        settings[6].location,
        sql.find("'00000003-0000001B-1'").unwrap() as i32
    );
    assert!(
        settings[7..]
            .iter()
            .all(|setting| setting.kind == VariableSetKind::Reset)
    );
    assert!(settings[7..].iter().all(|setting| setting.location == -1));
}

#[test]
fn create_stats_stmt_wraps_columns_and_expressions_in_stats_elems() {
    let Node::CreateStatsStmt(stmt) = parse_statement(
        "create statistics if not exists app.item_stats (ndistinct, dependencies) on category, lower(name), cast(price as bigint), (price * quantity) from app.items",
    ) else {
        panic!("expected CreateStatsStmt");
    };
    assert!(stmt.if_not_exists);
    assert_eq!(stmt.defnames.len(), 2);
    assert_eq!(stmt.stat_types.len(), 2);
    assert_eq!(stmt.exprs.len(), 4);
    assert_eq!(stmt.relations.len(), 1);
    assert!(stmt.stxcomment.is_none());
    assert!(!stmt.transformed);

    let Node::StatsElem(column) = &stmt.exprs[0] else {
        panic!("expected StatsElem");
    };
    assert_eq!(column.name.as_deref(), Some("category"));
    assert!(column.expr.is_none());

    let Node::StatsElem(expression) = &stmt.exprs[1] else {
        panic!("expected StatsElem");
    };
    assert!(expression.name.is_none());
    assert!(expression.expr.is_some());

    let Node::StatsElem(cast) = &stmt.exprs[2] else {
        panic!("expected CAST StatsElem");
    };
    assert!(matches!(cast.expr.as_deref(), Some(Node::TypeCast(_))));

    let Node::StatsElem(parenthesized) = &stmt.exprs[3] else {
        panic!("expected parenthesized StatsElem");
    };
    assert!(parenthesized.name.is_none());
    assert!(matches!(
        parenthesized.expr.as_deref(),
        Some(Node::AExpr(_))
    ));

    let Node::CreateStatsStmt(anonymous) =
        parse_statement("create statistics on category from app.items")
    else {
        panic!("expected anonymous CreateStatsStmt");
    };
    assert!(anonymous.defnames.is_empty());
}

#[test]
fn create_table_stmt_populates_like_inheritance_partition_and_storage_clauses() {
    let sql = "create table events (like event_template including defaults excluding indexes, id int) inherits (base_events) partition by range (created_at collate pg_catalog.\"C\" app.timestamp_ops, lower(id), (id + 1)) using heap with (fillfactor = 80) tablespace fast_space";
    let Node::CreateStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateStmt");
    };
    assert_eq!(stmt.table_elts.len(), 2);
    assert_eq!(stmt.inh_relations.len(), 1);
    assert!(stmt.nnconstraints.is_empty());
    assert_eq!(stmt.access_method.as_deref(), Some("heap"));
    assert_eq!(stmt.options.len(), 1);
    assert_eq!(stmt.tablespacename.as_deref(), Some("fast_space"));

    let Node::TableLikeClause(like) = &stmt.table_elts[0] else {
        panic!("expected TableLikeClause");
    };
    assert!(like.relation.is_some());
    assert_ne!(like.options, 0);

    let partspec = stmt.partspec.expect("PartitionSpec");
    assert_eq!(partspec.strategy, PartitionStrategy::Range);
    assert_eq!(
        partspec.location as usize,
        sql.find("partition by").unwrap()
    );
    assert_eq!(partspec.part_params.len(), 3);
    let Node::PartitionElem(partition) = &partspec.part_params[0] else {
        panic!("expected PartitionElem");
    };
    assert_eq!(partition.name.as_deref(), Some("created_at"));
    assert_eq!(partition.collation.len(), 2);
    assert_eq!(partition.opclass.len(), 2);
    assert_eq!(partition.location as usize, sql.find("created_at").unwrap());
    let Node::PartitionElem(function) = &partspec.part_params[1] else {
        panic!("expected function PartitionElem");
    };
    assert!(matches!(function.expr.as_deref(), Some(Node::FuncCall(_))));
    assert_eq!(function.location as usize, sql.find("lower(id)").unwrap());
    let Node::PartitionElem(expression) = &partspec.part_params[2] else {
        panic!("expected expression PartitionElem");
    };
    assert!(matches!(expression.expr.as_deref(), Some(Node::AExpr(_))));
    assert_eq!(expression.location as usize, sql.find("(id + 1)").unwrap());
}

#[test]
fn create_table_like_options_follow_ordered_bitmask_semantics() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table copied (like source including all excluding indexes excluding storage, like fallback excluding all including defaults)",
    ) else {
        panic!("expected CreateStmt");
    };
    let [
        Node::TableLikeClause(source),
        Node::TableLikeClause(fallback),
    ] = stmt.table_elts.as_slice()
    else {
        panic!("expected two TableLikeClause nodes");
    };
    let all = TableLikeOption::All as u32;
    assert_eq!(
        source.options,
        all & !(TableLikeOption::Indexes as u32) & !(TableLikeOption::Storage as u32)
    );
    assert_eq!(fallback.options, TableLikeOption::Defaults as u32);
    assert_eq!(source.relation_oid, 0);
    assert!(matches!(
        source.relation.as_deref(),
        Some(relation) if relation.relname.as_deref() == Some("source") && relation.inh
    ));
}

#[test]
fn create_typed_table_preserves_type_column_options_and_on_commit() {
    let sql = "create temporary table typed_items of app.item_type (name with options not null, constraint typed_name_check check (name <> '')) on commit preserve rows";
    let Node::CreateStmt(stmt) = parse_statement(sql) else {
        panic!("expected CreateStmt");
    };
    let type_name = stmt.of_typename.as_deref().expect("typed table type");
    assert_eq!(
        type_name
            .names
            .iter()
            .map(|name| match name {
                Node::String(name) => name.sval.as_deref().expect("type name"),
                other => panic!("expected String, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        ["app", "item_type"]
    );
    assert_eq!(
        type_name.location as usize,
        sql.find("app.item_type").unwrap()
    );
    assert_eq!(stmt.oncommit, pg_parser::OnCommitAction::PreserveRows);
    assert_eq!(stmt.table_elts.len(), 2);
    let Node::ColumnDef(column) = &stmt.table_elts[0] else {
        panic!("expected typed ColumnDef");
    };
    assert_eq!(column.colname.as_deref(), Some("name"));
    assert!(column.type_name.is_none());
    assert!(column.is_local);
    assert_eq!(column.inhcount, 0);
    assert!(!column.is_from_type);
    assert!(column.cooked_default.is_none());
    assert!(column.identity_sequence.is_none());
    assert_eq!(column.coll_oid, 0);
    assert!(matches!(
        column.constraints.as_slice(),
        [Node::Constraint(constraint)] if constraint.contype == ConstrType::Notnull
    ));
    assert!(matches!(
        stmt.table_elts.as_slice(),
        [_, Node::Constraint(constraint)]
            if constraint.contype == ConstrType::Check
                && constraint.conname.as_deref() == Some("typed_name_check")
    ));
}

#[test]
fn create_regular_and_typed_tables_parse_unnamed_table_not_null_constraints() {
    let Node::CreateStmt(regular) =
        parse_statement("create table regular_not_null (id int, not null id not valid no inherit)")
    else {
        panic!("expected CreateStmt");
    };
    assert!(matches!(
        regular.table_elts.as_slice(),
        [Node::ColumnDef(_), Node::Constraint(constraint)]
            if constraint.contype == ConstrType::Notnull
                && constraint.conname.is_none()
                && constraint.keys.len() == 1
                && constraint.skip_validation
                && constraint.is_no_inherit
    ));

    let Node::CreateStmt(typed) = parse_statement(
        "create table typed_not_null of app.item_type (name with options collate pg_catalog.\"C\", not null name)",
    ) else {
        panic!("expected CreateStmt");
    };
    assert!(matches!(
        typed.table_elts.as_slice(),
        [Node::ColumnDef(column), Node::Constraint(constraint)]
            if column.type_name.is_none()
                && column.coll_clause.is_some()
                && constraint.contype == ConstrType::Notnull
                && constraint.keys.len() == 1
    ));
}

#[test]
fn create_table_populates_column_and_table_constraint_payloads() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table orders (id bigint generated always as identity (start with 10 increment by 5) primary key, account_id bigint constraint orders_account_fk references accounts(id) on update cascade on delete set null, amount numeric(12,2) default 0 check (amount >= 0) not null, slug text collate pg_catalog.c unique nulls not distinct, computed int generated always as (amount::int) stored, constraint orders_amount_check check (amount < 100000) not valid, constraint orders_account_unique unique (account_id, slug) include (amount) with (fillfactor = 80) using index tablespace fast_space, constraint orders_fk foreign key (account_id) references accounts(id) match full on delete cascade deferrable initially deferred)",
    ) else {
        panic!("expected CreateStmt");
    };
    assert_eq!(stmt.table_elts.len(), 8);

    let Node::ColumnDef(id) = &stmt.table_elts[0] else {
        panic!("expected id ColumnDef");
    };
    assert_eq!(id.colname.as_deref(), Some("id"));
    assert_eq!(id.constraints.len(), 2);
    let Node::Constraint(identity) = &id.constraints[0] else {
        panic!("expected identity constraint");
    };
    assert_eq!(identity.contype, ConstrType::Identity);
    assert_eq!(identity.generated_when, b'a');
    assert_eq!(identity.options.len(), 2);
    let Node::Constraint(primary) = &id.constraints[1] else {
        panic!("expected primary-key constraint");
    };
    assert_eq!(primary.contype, ConstrType::Primary);

    let Node::ColumnDef(account_id) = &stmt.table_elts[1] else {
        panic!("expected account_id ColumnDef");
    };
    let Node::Constraint(column_fk) = &account_id.constraints[0] else {
        panic!("expected column foreign key");
    };
    assert_eq!(column_fk.contype, ConstrType::Foreign);
    assert_eq!(column_fk.conname.as_deref(), Some("orders_account_fk"));
    assert!(column_fk.pktable.is_some());
    assert_eq!(column_fk.pk_attrs.len(), 1);
    assert_eq!(column_fk.fk_upd_action, b'c');
    assert_eq!(column_fk.fk_del_action, b'n');

    let Node::ColumnDef(amount) = &stmt.table_elts[2] else {
        panic!("expected amount ColumnDef");
    };
    assert_eq!(amount.constraints.len(), 3);
    assert!(matches!(
        &amount.constraints[0],
        Node::Constraint(c) if c.contype == ConstrType::Default && c.raw_expr.is_some()
    ));
    assert!(matches!(
        &amount.constraints[1],
        Node::Constraint(c) if c.contype == ConstrType::Check && c.raw_expr.is_some()
    ));
    assert!(matches!(
        &amount.constraints[2],
        Node::Constraint(c) if c.contype == ConstrType::Notnull
    ));

    let Node::ColumnDef(slug) = &stmt.table_elts[3] else {
        panic!("expected slug ColumnDef");
    };
    assert!(slug.coll_clause.is_some());
    assert!(matches!(
        &slug.constraints[0],
        Node::Constraint(c) if c.contype == ConstrType::Unique && c.nulls_not_distinct
    ));

    let Node::ColumnDef(computed) = &stmt.table_elts[4] else {
        panic!("expected computed ColumnDef");
    };
    let Node::Constraint(generated) = &computed.constraints[0] else {
        panic!("expected generated constraint");
    };
    assert_eq!(generated.contype, ConstrType::Generated);
    assert_eq!(generated.generated_kind, b's');
    assert!(generated.raw_expr.is_some());

    let Node::Constraint(check) = &stmt.table_elts[5] else {
        panic!("expected table check constraint");
    };
    assert_eq!(check.conname.as_deref(), Some("orders_amount_check"));
    assert_eq!(check.contype, ConstrType::Check);
    assert!(check.skip_validation);
    assert!(!check.initially_valid);

    let Node::Constraint(unique) = &stmt.table_elts[6] else {
        panic!("expected table unique constraint");
    };
    assert_eq!(unique.contype, ConstrType::Unique);
    assert_eq!(unique.keys.len(), 2);
    assert_eq!(unique.including.len(), 1);
    assert_eq!(unique.options.len(), 1);
    assert_eq!(unique.indexspace.as_deref(), Some("fast_space"));

    let Node::Constraint(foreign) = &stmt.table_elts[7] else {
        panic!("expected table foreign-key constraint");
    };
    assert_eq!(foreign.contype, ConstrType::Foreign);
    assert_eq!(foreign.fk_attrs.len(), 1);
    assert_eq!(foreign.pk_attrs.len(), 1);
    assert_eq!(foreign.fk_matchtype, b'f');
    assert_eq!(foreign.fk_del_action, b'c');
    assert!(foreign.deferrable);
    assert!(foreign.initdeferred);
}

#[test]
fn create_table_column_defaults_follow_restricted_b_expr_grammar() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table defaults (compared boolean default 1 is not distinct from 2 not null, grouped boolean default (true and false), ordinary int default 1 + 2)",
    ) else {
        panic!("expected CreateStmt");
    };
    let [
        Node::ColumnDef(compared),
        Node::ColumnDef(grouped),
        Node::ColumnDef(ordinary),
    ] = stmt.table_elts.as_slice()
    else {
        panic!("expected three ColumnDef nodes");
    };
    assert!(matches!(
        compared.constraints.as_slice(),
        [Node::Constraint(default), Node::Constraint(not_null)]
            if default.contype == ConstrType::Default
                && matches!(default.raw_expr.as_deref(), Some(Node::AExpr(expr)) if expr.kind == pg_parser::AExprKind::NotDistinct)
                && not_null.contype == ConstrType::Notnull
    ));
    assert!(matches!(
        grouped.constraints.as_slice(),
        [Node::Constraint(default)]
            if matches!(default.raw_expr.as_deref(), Some(Node::BoolExpr(_)))
    ));
    assert!(matches!(
        ordinary.constraints.as_slice(),
        [Node::Constraint(default)]
            if matches!(default.raw_expr.as_deref(), Some(Node::AExpr(_)))
    ));
}

#[test]
fn create_table_identity_and_generated_columns_preserve_generation_modes() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table generated_modes (id bigint generated by default as identity (cache 8), implicit_virtual int generated always as (id + 1), explicit_virtual int generated always as (id + 2) virtual)",
    ) else {
        panic!("expected CreateStmt");
    };
    let columns = stmt
        .table_elts
        .iter()
        .filter_map(|element| match element {
            Node::ColumnDef(column) => Some(column),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Node::Constraint(identity) = &columns[0].constraints[0] else {
        panic!("expected identity Constraint");
    };
    assert_eq!(identity.contype, ConstrType::Identity);
    assert_eq!(identity.generated_when, b'd');
    assert_eq!(identity.options.len(), 1);
    for column in &columns[1..] {
        let Node::Constraint(generated) = &column.constraints[0] else {
            panic!("expected generated Constraint");
        };
        assert_eq!(generated.contype, ConstrType::Generated);
        assert_eq!(generated.generated_when, b'a');
        assert_eq!(generated.generated_kind, b'v');
        assert!(generated.raw_expr.is_some());
    }
}

#[test]
fn create_table_column_constraint_attributes_remain_raw_constraint_nodes() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table raw_attributes (id int unique deferrable initially deferred enforced, parent_id int references parent(id) not deferrable initially immediate not enforced)",
    ) else {
        panic!("expected CreateStmt");
    };
    let columns = stmt
        .table_elts
        .iter()
        .filter_map(|element| match element {
            Node::ColumnDef(column) => Some(column),
            _ => None,
        })
        .collect::<Vec<_>>();
    let first_types = columns[0]
        .constraints
        .iter()
        .map(|node| match node {
            Node::Constraint(constraint) => constraint.contype,
            _ => panic!("expected Constraint"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        first_types,
        [
            ConstrType::Unique,
            ConstrType::AttrDeferrable,
            ConstrType::AttrDeferred,
            ConstrType::AttrEnforced,
        ]
    );
    let second_types = columns[1]
        .constraints
        .iter()
        .map(|node| match node {
            Node::Constraint(constraint) => constraint.contype,
            _ => panic!("expected Constraint"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        second_types,
        [
            ConstrType::Foreign,
            ConstrType::AttrNotDeferrable,
            ConstrType::AttrImmediate,
            ConstrType::AttrNotEnforced,
        ]
    );
}

#[test]
fn create_table_constraint_attributes_follow_process_cas_bits() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table child (
             id integer,
             parent_id integer,
             constraint child_parent_fk foreign key (parent_id)
                 references parent(id) initially deferred,
             constraint positive_id check (id > 0) not enforced,
             constraint present_parent not null parent_id not valid no inherit
         )",
    ) else {
        panic!("expected CreateStmt");
    };
    let constraints = stmt
        .table_elts
        .iter()
        .filter_map(|node| match node {
            Node::Constraint(constraint) => Some(constraint),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(constraints.len(), 3);
    assert_eq!(constraints[0].contype, ConstrType::Foreign);
    assert!(constraints[0].deferrable);
    assert!(constraints[0].initdeferred);
    assert_eq!(constraints[1].contype, ConstrType::Check);
    assert!(!constraints[1].is_enforced);
    assert!(constraints[1].skip_validation);
    assert!(!constraints[1].initially_valid);
    assert_eq!(constraints[2].contype, ConstrType::Notnull);
    assert!(constraints[2].skip_validation);
    assert!(constraints[2].is_no_inherit);
}

#[test]
fn create_table_foreign_keys_preserve_period_columns() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table child (
             id integer,
             valid_at daterange,
             foreign key (id, period valid_at)
                 references parent (id, period valid_at)
         )",
    ) else {
        panic!("expected CreateStmt");
    };
    let constraint = stmt
        .table_elts
        .iter()
        .find_map(|node| match node {
            Node::Constraint(constraint) if constraint.contype == ConstrType::Foreign => {
                Some(constraint)
            }
            _ => None,
        })
        .expect("foreign key Constraint");
    assert!(constraint.fk_with_period);
    assert!(constraint.pk_with_period);
    assert_eq!(constraint.fk_attrs.len(), 2);
    assert_eq!(constraint.pk_attrs.len(), 2);
    assert!(matches!(
        constraint.fk_attrs.last(),
        Some(Node::String(name)) if name.sval.as_deref() == Some("valid_at")
    ));
}

#[test]
fn create_table_preserves_without_overlaps_and_foreign_key_set_columns() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table child (tenant_id int, parent_id int, valid_at daterange, unique (tenant_id, valid_at without overlaps), foreign key (tenant_id, parent_id) references parent (tenant_id, id) on delete set null (parent_id))",
    ) else {
        panic!("expected CreateStmt");
    };
    let constraints = stmt
        .table_elts
        .iter()
        .filter_map(|element| match element {
            Node::Constraint(constraint) => Some(constraint),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(constraints.len(), 2);
    assert_eq!(constraints[0].contype, ConstrType::Unique);
    assert!(constraints[0].without_overlaps);
    assert_eq!(constraints[1].contype, ConstrType::Foreign);
    assert_eq!(constraints[1].fk_del_action, b'n');
    assert!(matches!(
        constraints[1].fk_del_set_cols.as_slice(),
        [Node::String(column)] if column.sval.as_deref() == Some("parent_id")
    ));
}

#[test]
fn create_table_existing_index_constraints_preserve_index_names_and_attributes() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table indexed_constraints (id int, code int, constraint indexed_unique unique using index existing_unique deferrable initially deferred, constraint indexed_primary primary key using index existing_primary not deferrable initially immediate)",
    ) else {
        panic!("expected CreateStmt");
    };
    let constraints = stmt
        .table_elts
        .iter()
        .filter_map(|element| match element {
            Node::Constraint(constraint) => Some(constraint),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(constraints.len(), 2);
    assert_eq!(constraints[0].contype, ConstrType::Unique);
    assert_eq!(constraints[0].indexname.as_deref(), Some("existing_unique"));
    assert!(constraints[0].keys.is_empty());
    assert!(constraints[0].deferrable);
    assert!(constraints[0].initdeferred);
    assert_eq!(constraints[1].contype, ConstrType::Primary);
    assert_eq!(
        constraints[1].indexname.as_deref(),
        Some("existing_primary")
    );
    assert!(!constraints[1].deferrable);
    assert!(!constraints[1].initdeferred);
}

#[test]
fn create_table_relation_names_follow_colid_and_collabel_categories() {
    let Node::CreateStmt(qualified) = parse_statement("create table app.select (id integer)")
    else {
        panic!("expected CreateStmt");
    };
    let relation = qualified.relation.as_deref().expect("RangeVar");
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("select"));

    let Node::CreateStmt(catalog_qualified) =
        parse_statement("create table current_db.app.items (id integer)")
    else {
        panic!("expected CreateStmt");
    };
    let relation = catalog_qualified.relation.as_deref().expect("RangeVar");
    assert_eq!(relation.catalogname.as_deref(), Some("current_db"));
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("items"));

    let Node::CreateStmt(quoted) = parse_statement("create table \"select\" (id integer)") else {
        panic!("expected CreateStmt");
    };
    assert_eq!(
        quoted
            .relation
            .as_deref()
            .and_then(|range| range.relname.as_deref()),
        Some("select")
    );

    let Node::CreateStmt(quoted_column) = parse_statement(
        "create table names (\"select\" integer storage default compression default)",
    ) else {
        panic!("expected CreateStmt");
    };
    let [Node::ColumnDef(column)] = quoted_column.table_elts.as_slice() else {
        panic!("expected quoted ColumnDef");
    };
    assert_eq!(column.colname.as_deref(), Some("select"));
    assert_eq!(column.storage_name.as_deref(), Some("default"));
    assert_eq!(column.compression.as_deref(), Some("default"));
}

#[test]
fn create_object_names_and_option_names_follow_exact_keyword_categories() {
    let Node::CreateFdwStmt(fdw) = parse_statement(
        "create foreign data wrapper \"select\" options (select 'allowed-as-collabel')",
    ) else {
        panic!("expected CreateFdwStmt");
    };
    assert_eq!(fdw.fdwname.as_deref(), Some("select"));
    assert!(matches!(
        fdw.options.as_slice(),
        [Node::DefElem(option)] if option.defname.as_deref() == Some("select")
    ));

    let Node::CreatedbStmt(database) =
        parse_statement("create database \"select\" with encoding = 'UTF8' \"from\" = 'custom'")
    else {
        panic!("expected CreatedbStmt");
    };
    assert_eq!(database.dbname.as_deref(), Some("select"));
    assert!(database.options.iter().any(
        |node| matches!(node, Node::DefElem(option) if option.defname.as_deref() == Some("from"))
    ));

    let Node::CreateSchemaStmt(schema) = parse_statement("create schema \"select\"") else {
        panic!("expected CreateSchemaStmt");
    };
    assert_eq!(schema.schemaname.as_deref(), Some("select"));

    let Node::IndexStmt(index) = parse_statement("create index \"select\" on items (id)") else {
        panic!("expected IndexStmt");
    };
    assert_eq!(index.idxname.as_deref(), Some("select"));
}

#[test]
fn qualified_any_names_and_function_names_use_distinct_first_token_categories() {
    let Node::CreateDomainStmt(domain) = parse_statement("create domain app.select as integer")
    else {
        panic!("expected CreateDomainStmt");
    };
    assert!(matches!(
        domain.domainname.as_slice(),
        [Node::String(schema), Node::String(name)]
            if schema.sval.as_deref() == Some("app") && name.sval.as_deref() == Some("select")
    ));

    let Node::CreateFunctionStmt(type_keyword) = parse_statement(
        "create function authorization() returns integer language sql as 'select 1'",
    ) else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(matches!(
        type_keyword.funcname.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("authorization")
    ));

    let Node::CreateFunctionStmt(quoted) =
        parse_statement("create function \"select\"() returns integer language sql as 'select 1'")
    else {
        panic!("expected CreateFunctionStmt");
    };
    assert!(matches!(
        quoted.funcname.as_slice(),
        [Node::String(name)] if name.sval.as_deref() == Some("select")
    ));
}

#[test]
fn create_table_exclusion_constraint_preserves_index_payload() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table reservations (room int, during tstzrange, constraint no_overlap exclude using gist (lower(room) collate pg_catalog.\"C\" app.text_ops desc nulls last with =, during with operator(pg_catalog.&&)) include (room) with (fillfactor = 80) using index tablespace fast_space where (room > 0) deferrable initially immediate)",
    ) else {
        panic!("expected CreateStmt");
    };
    let Node::Constraint(exclusion) = &stmt.table_elts[2] else {
        panic!("expected exclusion Constraint");
    };
    assert_eq!(exclusion.contype, ConstrType::Exclusion);
    assert_eq!(exclusion.conname.as_deref(), Some("no_overlap"));
    assert_eq!(exclusion.access_method.as_deref(), Some("gist"));
    assert_eq!(exclusion.exclusions.len(), 2);
    assert!(
        exclusion
            .exclusions
            .iter()
            .all(|item| matches!(item, Node::AArrayExpr(pair) if pair.elements.len() == 2))
    );
    let Node::AArrayExpr(first_pair) = &exclusion.exclusions[0] else {
        panic!("expected exclusion pair");
    };
    let Node::IndexElem(first_element) = &first_pair.elements[0] else {
        panic!("expected exclusion IndexElem");
    };
    assert!(matches!(
        first_element.expr.as_deref(),
        Some(Node::FuncCall(_))
    ));
    assert_eq!(first_element.collation.len(), 2);
    assert_eq!(first_element.opclass.len(), 2);
    assert_eq!(first_element.ordering, pg_parser::SortByDir::Desc);
    assert_eq!(first_element.nulls_ordering, pg_parser::SortByNulls::Last);
    assert_eq!(exclusion.including.len(), 1);
    assert_eq!(exclusion.options.len(), 1);
    assert_eq!(exclusion.indexspace.as_deref(), Some("fast_space"));
    assert!(exclusion.where_clause.is_some());
    assert!(exclusion.deferrable);
    assert!(!exclusion.initdeferred);
}

#[test]
fn create_table_type_names_preserve_canonical_names_modifiers_and_arrays() {
    let Node::CreateStmt(stmt) = parse_statement(
        "create table typed_values (id int, amount numeric(12,2), label character varying(30), flags bit varying(8), created timestamp(3) with time zone, tags app.tag_type[][], numbers int array[4])",
    ) else {
        panic!("expected CreateStmt");
    };
    assert_eq!(stmt.table_elts.len(), 7);

    let type_name = |index: usize| {
        let Node::ColumnDef(column) = &stmt.table_elts[index] else {
            panic!("expected ColumnDef");
        };
        column.type_name.as_deref().expect("TypeName")
    };
    let names = |index: usize| {
        type_name(index)
            .names
            .iter()
            .map(|node| match node {
                Node::String(value) => value.sval.as_deref().expect("type name"),
                other => panic!("expected String type name, got {other:?}"),
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(names(0), ["pg_catalog", "int4"]);
    assert_eq!(names(1), ["pg_catalog", "numeric"]);
    assert_eq!(type_name(1).typmods.len(), 2);
    assert_eq!(names(2), ["pg_catalog", "varchar"]);
    assert_eq!(type_name(2).typmods.len(), 1);
    assert_eq!(names(3), ["pg_catalog", "varbit"]);
    assert_eq!(type_name(3).typmods.len(), 1);
    assert_eq!(names(4), ["pg_catalog", "timestamptz"]);
    assert_eq!(type_name(4).typmods.len(), 1);
    assert_eq!(names(5), ["app", "tag_type"]);
    assert_eq!(type_name(5).array_bounds.len(), 2);
    assert_eq!(names(6), ["pg_catalog", "int4"]);
    assert_eq!(type_name(6).array_bounds.len(), 1);
}

#[test]
fn type_names_preserve_setof_interval_and_default_modifiers() {
    let Node::CreateStmt(table) = parse_statement(
        "create table temporal_values (duration interval day to second(3), local_time time(2) without time zone, fixed_bits bit)",
    ) else {
        panic!("expected CreateStmt");
    };
    let Node::ColumnDef(duration) = &table.table_elts[0] else {
        panic!("expected duration ColumnDef");
    };
    let duration_type = duration.type_name.as_deref().expect("duration TypeName");
    assert_eq!(duration_type.typmods.len(), 2);

    let Node::ColumnDef(local_time) = &table.table_elts[1] else {
        panic!("expected local_time ColumnDef");
    };
    let local_time_type = local_time.type_name.as_deref().expect("time TypeName");
    assert!(matches!(
        local_time_type.names.last(),
        Some(Node::String(name)) if name.sval.as_deref() == Some("time")
    ));
    assert_eq!(local_time_type.typmods.len(), 1);

    let Node::ColumnDef(bits) = &table.table_elts[2] else {
        panic!("expected fixed_bits ColumnDef");
    };
    assert_eq!(
        bits.type_name
            .as_deref()
            .expect("bit TypeName")
            .typmods
            .len(),
        1
    );

    let sql =
        "create function all_items() returns setof app.item_type[] language sql as 'select null'";
    let Node::CreateFunctionStmt(function) = parse_statement(sql) else {
        panic!("expected CreateFunctionStmt");
    };
    let return_type = function.return_type.as_deref().expect("return TypeName");
    assert!(return_type.setof);
    assert_eq!(return_type.array_bounds.len(), 1);
    assert_eq!(
        return_type.location as usize,
        sql.find("app.item_type").unwrap()
    );
}

#[test]
fn create_partition_stmt_populates_range_and_hash_bounds() {
    let range_sql = "create table events_2026 partition of events for values from (minvalue, '2026-01-01') to (maxvalue, '2027-01-01')";
    let Node::CreateStmt(range) = parse_statement(range_sql) else {
        panic!("expected CreateStmt");
    };
    let range_bound = range.partbound.expect("PartitionBoundSpec");
    assert_eq!(range_bound.strategy, b'r');
    assert_eq!(
        range_bound.location as usize,
        range_sql.find("from").unwrap()
    );
    assert_eq!(range_bound.modulus, 0);
    assert_eq!(range_bound.remainder, 0);
    assert!(matches!(
        range_bound.lowerdatums.as_slice(),
        [Node::ColumnRef(minimum), Node::AConst(_)]
            if matches!(minimum.fields.as_slice(), [Node::String(name)]
                if name.sval.as_deref() == Some("minvalue"))
    ));
    assert!(matches!(
        range_bound.upperdatums.as_slice(),
        [Node::ColumnRef(maximum), Node::AConst(_)]
            if matches!(maximum.fields.as_slice(), [Node::String(name)]
                if name.sval.as_deref() == Some("maxvalue"))
    ));

    let hash_sql =
        "create table events_1 partition of events_hash for values with (modulus 4, remainder 1)";
    let Node::CreateStmt(hash) = parse_statement(hash_sql) else {
        panic!("expected CreateStmt");
    };
    let hash_bound = hash.partbound.expect("PartitionBoundSpec");
    assert_eq!(hash_bound.strategy, b'h');
    assert_eq!(hash_bound.location as usize, hash_sql.find("with").unwrap());
    assert_eq!(hash_bound.modulus, 4);
    assert_eq!(hash_bound.remainder, 1);

    let list_sql =
        "create table events_active partition of events_list for values in ('active', 'pending')";
    let Node::CreateStmt(list) = parse_statement(list_sql) else {
        panic!("expected CreateStmt");
    };
    let list_bound = list.partbound.expect("PartitionBoundSpec");
    assert_eq!(list_bound.strategy, b'l');
    assert_eq!(list_bound.location as usize, list_sql.find("in (").unwrap());
    assert_eq!(list_bound.listdatums.len(), 2);
    assert_eq!(list_bound.modulus, 0);
    assert_eq!(list_bound.remainder, 0);

    let default_sql = "create table events_default partition of events_list default";
    let Node::CreateStmt(default) = parse_statement(default_sql) else {
        panic!("expected CreateStmt");
    };
    let default_bound = default.partbound.expect("PartitionBoundSpec");
    assert!(default_bound.is_default);
    assert_eq!(
        default_bound.location as usize,
        default_sql.rfind("default").unwrap()
    );
    assert_eq!(default_bound.modulus, 0);
    assert_eq!(default_bound.remainder, 0);
}

#[test]
fn create_trigger_stmt_populates_transition_relations() {
    let Node::CreateTrigStmt(stmt) = parse_statement(
        "create trigger audit_changes after update on items referencing old table old_rows new table as new_rows for each statement execute function audit_items()",
    ) else {
        panic!("expected CreateTrigStmt");
    };
    assert_eq!(stmt.transition_rels.len(), 2);
    let Node::TriggerTransition(old) = &stmt.transition_rels[0] else {
        panic!("expected TriggerTransition");
    };
    assert!(!old.is_new);
    assert!(old.is_table);
    assert_eq!(old.name.as_deref(), Some("old_rows"));
    let Node::TriggerTransition(new) = &stmt.transition_rels[1] else {
        panic!("expected TriggerTransition");
    };
    assert!(new.is_new);
    assert_eq!(new.name.as_deref(), Some("new_rows"));
}

#[test]
fn create_database_schema_view_and_index_populate_required_fields() {
    let Node::CreatedbStmt(database) =
        parse_statement("create database appdb with encoding 'UTF8' template template0")
    else {
        panic!("expected CreatedbStmt");
    };
    assert_eq!(database.dbname.as_deref(), Some("appdb"));
    assert_eq!(database.options.len(), 2);

    let Node::CreatedbStmt(options) = parse_statement(
        "create database configured with connection limit = -1 encoding 'UTF8' owner app_owner tablespace fast_space template template0 allow_connections on strategy 1.5 locale_provider default",
    ) else {
        panic!("expected CreatedbStmt");
    };
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

    let Node::CreateSchemaStmt(schema) =
        parse_statement("create schema if not exists app authorization app_owner")
    else {
        panic!("expected CreateSchemaStmt");
    };
    assert_eq!(schema.schemaname.as_deref(), Some("app"));
    assert!(schema.authrole.is_some());
    assert!(schema.if_not_exists);

    let Node::ViewStmt(view) = parse_statement(
        "create view app.active_items(id, name) with (security_barrier = true) as select id, name from app.items where active = true",
    ) else {
        panic!("expected ViewStmt");
    };
    assert!(view.view.is_some());
    assert_eq!(view.aliases.len(), 2);
    assert_eq!(view.options.len(), 1);
    assert!(matches!(view.query.as_deref(), Some(Node::SelectStmt(_))));

    let Node::IndexStmt(index) = parse_statement(
        "create unique index concurrently if not exists item_lookup on app.items using btree (id, lower(name)) include (category) nulls not distinct with (fillfactor = 80) tablespace fast_space where active = true",
    ) else {
        panic!("expected IndexStmt");
    };
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
    let Node::IndexStmt(index) = parse_statement(sql) else {
        panic!("expected IndexStmt");
    };
    let Node::IndexElem(name) = &index.index_params[0] else {
        panic!("expected IndexElem");
    };
    assert_eq!(name.name.as_deref(), Some("name"));
    assert_eq!(name.collation.len(), 2);
    assert_eq!(name.opclass.len(), 1);
    assert_eq!(name.opclassopts.len(), 1);
    assert_eq!(name.ordering, pg_parser::SortByDir::Desc);
    assert_eq!(name.nulls_ordering, pg_parser::SortByNulls::First);
    assert_eq!(name.location as usize, sql.find("name collate").unwrap());

    let Node::IndexElem(expression) = &index.index_params[1] else {
        panic!("expected expression IndexElem");
    };
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

    let Node::IndexElem(parenthesized) = &index.index_params[2] else {
        panic!("expected parenthesized IndexElem");
    };
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
    let Node::IndexStmt(index) = parse_statement("create index item_id_idx on items (id)") else {
        panic!("expected IndexStmt");
    };
    assert_eq!(index.access_method.as_deref(), Some("btree"));

    let Node::CreateStmt(table) =
        parse_statement("create table reservations (room int, exclude (room with =))")
    else {
        panic!("expected CreateStmt");
    };
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
    let Node::ViewStmt(local) = parse_statement(
        "create temp view app.local_view as select 1 as id with local check option",
    ) else {
        panic!("expected ViewStmt");
    };
    assert_eq!(local.with_check_option, ViewCheckOption::LocalCheckOption);
    assert_eq!(
        local.view.as_deref().map(|view| view.relpersistence),
        Some(b't')
    );

    let Node::ViewStmt(cascaded) = parse_statement(
        "create or replace unlogged view app.cascaded_view as select 1 as id with check option",
    ) else {
        panic!("expected ViewStmt");
    };
    assert!(cascaded.replace);
    assert_eq!(
        cascaded.with_check_option,
        ViewCheckOption::CascadedCheckOption
    );
    assert_eq!(
        cascaded.view.as_deref().map(|view| view.relpersistence),
        Some(b'u')
    );

    let Node::ViewStmt(recursive) = parse_statement(
        "create recursive view app.numbers(n) as
         values (1) union all select n + 1 from numbers where n < 3",
    ) else {
        panic!("expected ViewStmt");
    };
    let Some(Node::SelectStmt(query)) = recursive.query.as_deref() else {
        panic!("recursive view must be rewritten to SelectStmt");
    };
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
fn create_relation_persistence_modifiers_reach_raw_rangevars() {
    let Node::CreateStmt(temporary) =
        parse_statement("create local temporary table temp_items (id integer)")
    else {
        panic!("expected CreateStmt");
    };
    assert_eq!(
        temporary
            .relation
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b't')
    );

    let Node::CreateStmt(unlogged) =
        parse_statement("create unlogged table staging_items (id integer)")
    else {
        panic!("expected CreateStmt");
    };
    assert_eq!(
        unlogged
            .relation
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b'u')
    );

    let Node::CreateSeqStmt(sequence) =
        parse_statement("create global temp sequence temp_item_seq")
    else {
        panic!("expected CreateSeqStmt");
    };
    assert_eq!(
        sequence
            .sequence
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b't')
    );

    let Node::CreateTableAsStmt(matview) =
        parse_statement("create unlogged materialized view item_ids as select 1 as id")
    else {
        panic!("expected CreateTableAsStmt");
    };
    assert_eq!(
        matview
            .into
            .as_deref()
            .and_then(|into| into.rel.as_deref())
            .map(|range| range.relpersistence),
        Some(b'u')
    );

    let Node::CreateTableAsStmt(ctas) =
        parse_statement("create temp table copied_items as select 1 as id")
    else {
        panic!("expected CreateTableAsStmt");
    };
    assert_eq!(
        ctas.into
            .as_deref()
            .and_then(|into| into.rel.as_deref())
            .map(|range| range.relpersistence),
        Some(b't')
    );
}

#[test]
fn create_table_as_populates_complete_into_clause() {
    let Node::CreateTableAsStmt(stmt) = parse_statement(
        "create temp table if not exists copied_items(id, label)
         using heap with (fillfactor = 80) on commit drop tablespace fast_space
         as select 1, 'item' with no data",
    ) else {
        panic!("expected CreateTableAsStmt");
    };
    assert!(stmt.if_not_exists);
    assert_eq!(stmt.objtype, pg_parser::ObjectType::Table);
    assert!(!stmt.is_select_into);
    let into = stmt.into.as_deref().expect("IntoClause");
    assert_eq!(into.col_names.len(), 2);
    assert_eq!(into.access_method.as_deref(), Some("heap"));
    assert_eq!(into.options.len(), 1);
    assert_eq!(into.on_commit, pg_parser::OnCommitAction::Drop);
    assert_eq!(into.table_space_name.as_deref(), Some("fast_space"));
    assert!(into.skip_data);
    assert_eq!(
        into.rel.as_deref().map(|range| range.relpersistence),
        Some(b't')
    );

    let Node::CreateTableAsStmt(without_oids) =
        parse_statement("create table copied_without_oids without oids as select 1")
    else {
        panic!("expected CreateTableAsStmt");
    };
    assert!(
        without_oids
            .into
            .as_deref()
            .is_some_and(|into| into.options.is_empty() && !into.skip_data)
    );
}

#[test]
fn create_table_as_execute_preserves_nested_execute_and_data_clause() {
    let Node::CreateTableAsStmt(stmt) = parse_statement(
        "create temp table if not exists executed_result(id) as execute prepared_query(1, 'x') with no data",
    ) else {
        panic!("expected CreateTableAsStmt");
    };
    assert!(stmt.if_not_exists);
    assert!(matches!(
        stmt.query.as_deref(),
        Some(Node::ExecuteStmt(execute))
            if execute.name.as_deref() == Some("prepared_query") && execute.params.len() == 2
    ));
    let into = stmt.into.as_deref().expect("IntoClause");
    assert!(into.skip_data);
    assert_eq!(into.col_names.len(), 1);
    assert_eq!(
        into.rel.as_deref().map(|range| range.relpersistence),
        Some(b't')
    );

    let Node::CreateTableAsStmt(with_data) =
        parse_statement("create table executed_result as execute prepared_query with data")
    else {
        panic!("expected CreateTableAsStmt");
    };
    assert!(!with_data.into.as_deref().expect("IntoClause").skip_data);
}

#[test]
fn create_schema_populates_nested_schema_statements() {
    let Node::CreateSchemaStmt(schema) = parse_statement(
        "create schema app
         create table items (id integer)
         create index items_idx on items (id)
         create domain positive_integer as integer check (value > 0)
         create function answer() returns integer language sql as 'select 42'
         create sequence item_seq
         create trigger items_trigger before insert on items for each row execute function answer()
         create type shell_type
         grant select on table items to public
         create view item_ids as select id from items",
    ) else {
        panic!("expected CreateSchemaStmt");
    };
    assert_eq!(schema.schemaname.as_deref(), Some("app"));
    assert_eq!(
        schema.schema_elts.iter().map(Node::tag).collect::<Vec<_>>(),
        [
            NodeTag::CreateStmt,
            NodeTag::IndexStmt,
            NodeTag::CreateDomainStmt,
            NodeTag::CreateFunctionStmt,
            NodeTag::CreateSeqStmt,
            NodeTag::CreateTrigStmt,
            NodeTag::DefineStmt,
            NodeTag::GrantStmt,
            NodeTag::ViewStmt,
        ]
    );

    let Node::CreateSchemaStmt(authorized) =
        parse_statement("create schema analytics authorization current_user")
    else {
        panic!("expected CreateSchemaStmt");
    };
    assert_eq!(authorized.schemaname.as_deref(), Some("analytics"));
    assert!(authorized.authrole.is_some());
    assert!(!authorized.if_not_exists);

    let Node::CreateSchemaStmt(authorization_only) =
        parse_statement("create schema authorization app_owner")
    else {
        panic!("expected CreateSchemaStmt");
    };
    assert!(authorization_only.schemaname.is_none());
    assert!(authorization_only.authrole.is_some());

    let Node::CreateSchemaStmt(if_not_exists) =
        parse_statement("create schema if not exists authorization app_owner")
    else {
        panic!("expected CreateSchemaStmt");
    };
    assert!(if_not_exists.if_not_exists);
    assert!(if_not_exists.schemaname.is_none());
    assert!(if_not_exists.authrole.is_some());
}

#[test]
fn create_schema_keeps_begin_atomic_function_bodies_inside_the_schema_element() {
    let Node::CreateSchemaStmt(schema) = parse_statement(
        "create schema app
         create function answer(flag boolean) returns integer language sql begin atomic
             return case when flag then 42 else 0 end;
         end
         create table answers (value integer)",
    ) else {
        panic!("expected CreateSchemaStmt");
    };
    assert!(matches!(
        schema.schema_elts.as_slice(),
        [Node::CreateFunctionStmt(function), Node::CreateStmt(_)]
            if matches!(function.sql_body.as_deref(), Some(Node::AArrayExpr(_)))
    ));
}

#[test]
fn create_role_sequence_domain_and_type_forms_populate_options() {
    let Node::CreateRoleStmt(role) = parse_statement(
        "create role app_user with login nosuperuser inherit connection limit 5 valid until 'infinity'",
    ) else {
        panic!("expected CreateRoleStmt");
    };
    assert_eq!(role.role.as_deref(), Some("app_user"));
    assert_eq!(role.stmt_type, pg_parser::RoleStmtType::Role);
    assert_eq!(role.options.len(), 5);

    let Node::CreateRoleStmt(user) = parse_statement("create user app_login") else {
        panic!("expected CreateRoleStmt");
    };
    assert_eq!(user.stmt_type, pg_parser::RoleStmtType::User);

    let Node::CreateRoleStmt(group) = parse_statement("create group app_group") else {
        panic!("expected CreateRoleStmt");
    };
    assert_eq!(group.stmt_type, pg_parser::RoleStmtType::Group);

    let Node::CreateRoleStmt(quoted_role) = parse_statement(r#"create role "select""#) else {
        panic!("expected quoted CreateRoleStmt");
    };
    assert_eq!(quoted_role.role.as_deref(), Some("select"));

    for role_name in ["PUBLIC", "NONE"] {
        let sql = format!(r#"create role "{role_name}""#);
        let Node::CreateRoleStmt(role) = parse_statement(&sql) else {
            panic!("expected quoted CreateRoleStmt");
        };
        assert_eq!(role.role.as_deref(), Some(role_name));
    }

    for (sql, defname) in [
        ("create role r1 admin alice", "adminmembers"),
        ("create role r2 role alice, bob", "rolemembers"),
        ("create role r3 user alice", "rolemembers"),
        ("create role r4 in role parent", "addroleto"),
        ("create role r5 in group parent", "addroleto"),
    ] {
        let Node::CreateRoleStmt(role) = parse_statement(sql) else {
            panic!("expected CreateRoleStmt for {sql}");
        };
        let Node::DefElem(option) = &role.options[0] else {
            panic!("expected role option for {sql}");
        };
        assert_eq!(option.defname.as_deref(), Some(defname), "{sql}");
        assert!(matches!(option.arg.as_deref(), Some(Node::AArrayExpr(_))));
    }

    let Node::CreateRoleStmt(legacy) = parse_statement("create role legacy sysid 42") else {
        panic!("expected CreateRoleStmt");
    };
    assert!(matches!(
        legacy.options.as_slice(),
        [Node::DefElem(option)]
            if option.defname.as_deref() == Some("sysid")
                && matches!(option.arg.as_deref(), Some(Node::Integer(value)) if value.ival == 42)
    ));

    let Node::CreateSeqStmt(sequence) = parse_statement(
        "create sequence if not exists app.events_id_seq as bigint increment by 2 minvalue 1 maxvalue 999 start with 10 cache 20 cycle owned by app.events.id",
    ) else {
        panic!("expected CreateSeqStmt");
    };
    assert!(sequence.if_not_exists);
    assert!(sequence.sequence.is_some());
    assert_eq!(sequence.options.len(), 8);
    assert!(!sequence.for_identity);
    let option = |index| {
        let Node::DefElem(option) = &sequence.options[index] else {
            panic!("expected sequence DefElem");
        };
        option
    };
    assert!(matches!(option(0).arg.as_deref(), Some(Node::TypeName(_))));
    for index in 1..=5 {
        assert!(matches!(
            option(index).arg.as_deref(),
            Some(Node::Integer(_))
        ));
    }
    assert!(matches!(option(6).arg.as_deref(), Some(Node::Boolean(value)) if value.boolval));
    assert!(matches!(
        option(7).arg.as_deref(),
        Some(Node::AArrayExpr(_))
    ));

    let Node::CreateSeqStmt(fractional_sequence) =
        parse_statement("create sequence fractional_seq increment by 1.5")
    else {
        panic!("expected CreateSeqStmt");
    };
    let Node::DefElem(increment) = &fractional_sequence.options[0] else {
        panic!("expected increment DefElem");
    };
    assert!(matches!(increment.arg.as_deref(), Some(Node::Float(_))));

    for (sql, expected_names) in [
        ("create sequence keyword_type as cache restart", 1),
        (
            "create sequence qualified_keyword_type as app.restart cache 2",
            2,
        ),
    ] {
        let Node::CreateSeqStmt(sequence) = parse_statement(sql) else {
            panic!("expected CreateSeqStmt for {sql}");
        };
        let Node::DefElem(as_type) = &sequence.options[0] else {
            panic!("expected AS DefElem for {sql}");
        };
        assert!(matches!(
            as_type.arg.as_deref(),
            Some(Node::TypeName(type_name)) if type_name.names.len() == expected_names
        ));
    }

    let Node::CreateSeqStmt(sequence) = parse_statement(
        "create sequence sequence_options no minvalue no maxvalue no cycle logged unlogged restart sequence name app.internal_seq",
    ) else {
        panic!("expected CreateSeqStmt");
    };
    assert!(matches!(
        sequence.options.as_slice(),
        [Node::DefElem(min), Node::DefElem(max), Node::DefElem(cycle), Node::DefElem(logged), Node::DefElem(unlogged), Node::DefElem(restart), Node::DefElem(name)]
            if min.defname.as_deref() == Some("minvalue") && min.arg.is_none()
                && max.defname.as_deref() == Some("maxvalue") && max.arg.is_none()
                && cycle.defname.as_deref() == Some("cycle")
                    && matches!(cycle.arg.as_deref(), Some(Node::Boolean(value)) if !value.boolval)
                && logged.defname.as_deref() == Some("logged") && logged.arg.is_none()
                && unlogged.defname.as_deref() == Some("unlogged") && unlogged.arg.is_none()
                && restart.defname.as_deref() == Some("restart") && restart.arg.is_none()
                && name.defname.as_deref() == Some("sequence_name")
                    && matches!(name.arg.as_deref(), Some(Node::AArrayExpr(_)))
    ));

    let Node::CreateDomainStmt(domain) = parse_statement(
        "create domain app.positive_int as int default 1 not null check (value > 0)",
    ) else {
        panic!("expected CreateDomainStmt");
    };
    assert_eq!(domain.domainname.len(), 2);
    assert!(domain.type_name.is_some());
    assert_eq!(domain.constraints.len(), 3);

    let Node::CreateDomainStmt(collated_domain) =
        parse_statement("create domain app.label as text collate pg_catalog.\"C\"")
    else {
        panic!("expected collated CreateDomainStmt");
    };
    assert!(collated_domain.coll_clause.is_some());

    let Node::CreateEnumStmt(enum_type) =
        parse_statement("create type app.mood as enum ('sad', 'ok', 'happy')")
    else {
        panic!("expected CreateEnumStmt");
    };
    assert_eq!(enum_type.vals.len(), 3);

    let Node::CreateEnumStmt(empty_enum) = parse_statement("create type empty_enum as enum ()")
    else {
        panic!("expected CreateEnumStmt");
    };
    assert!(empty_enum.vals.is_empty());

    let Node::CompositeTypeStmt(empty_composite) =
        parse_statement("create type app.empty_composite as ()")
    else {
        panic!("expected empty CompositeTypeStmt");
    };
    assert!(empty_composite.coldeflist.is_empty());
    assert!(empty_composite.typevar.is_some());

    let Node::CompositeTypeStmt(composite) = parse_statement(
        "create type app.pair as (left_value int collate pg_catalog.default, right_value int)",
    ) else {
        panic!("expected CompositeTypeStmt");
    };
    assert_eq!(
        composite.typevar.as_deref().map(|typevar| typevar.location),
        Some(12)
    );
    let Node::ColumnDef(left) = &composite.coldeflist[0] else {
        panic!("expected ColumnDef");
    };
    assert!(left.coll_clause.is_some());

    let Node::CreateRangeStmt(range_type) = parse_statement(
        "create type app.price_range as range (subtype = int, collation = default)",
    ) else {
        panic!("expected CreateRangeStmt");
    };
    assert_eq!(range_type.params.len(), 2);

    let Node::CompositeTypeStmt(composite) =
        parse_statement("create type app.pair as (left_value int, right_value text)")
    else {
        panic!("expected CompositeTypeStmt");
    };
    assert_eq!(composite.coldeflist.len(), 2);
    let typevar = composite
        .typevar
        .as_deref()
        .expect("composite type RangeVar");
    assert_eq!(typevar.schemaname.as_deref(), Some("app"));
    assert_eq!(typevar.relname.as_deref(), Some("pair"));
}

#[test]
fn create_extension_language_and_subscription_follow_raw_grammar_nodes() {
    let Node::CreateExtensionStmt(extension) = parse_statement(
        "create extension if not exists postgis with schema extensions version '3.5' cascade",
    ) else {
        panic!("expected CreateExtensionStmt");
    };
    assert_eq!(extension.extname.as_deref(), Some("postgis"));
    assert!(extension.if_not_exists);
    assert_eq!(extension.options.len(), 3);
    let Node::DefElem(schema_option) = &extension.options[0] else {
        panic!("expected DefElem");
    };
    assert!(schema_option.defnamespace.is_none());

    let Node::CreateExtensionStmt(language_extension) =
        parse_statement("create or replace language plpgsql")
    else {
        panic!("parameterless CREATE LANGUAGE must produce CreateExtensionStmt");
    };
    assert_eq!(language_extension.extname.as_deref(), Some("plpgsql"));
    assert!(language_extension.if_not_exists);

    let Node::CreateExtensionStmt(modified_language_extension) =
        parse_statement("create or replace trusted procedural language plpgsql")
    else {
        panic!("parameterless modified CREATE LANGUAGE must produce CreateExtensionStmt");
    };
    assert_eq!(
        modified_language_extension.extname.as_deref(),
        Some("plpgsql")
    );
    assert!(modified_language_extension.if_not_exists);

    let Node::CreatePLangStmt(language) = parse_statement(
        "create trusted language plsample handler app.plsample_handler inline app.plsample_inline validator app.plsample_validator",
    ) else {
        panic!("expected CreatePLangStmt");
    };
    assert!(language.pltrusted);
    assert_eq!(language.plname.as_deref(), Some("plsample"));
    assert_eq!(language.plhandler.len(), 2);
    assert_eq!(language.plinline.len(), 2);
    assert_eq!(language.plvalidator.len(), 2);

    let Node::CreatePLangStmt(no_validator) = parse_statement(
        "create or replace trusted procedural language plsample handler app.plsample_handler no validator",
    ) else {
        panic!("expected CreatePLangStmt with NO VALIDATOR");
    };
    assert!(no_validator.replace);
    assert!(no_validator.pltrusted);
    assert!(no_validator.plinline.is_empty());
    assert!(no_validator.plvalidator.is_empty());

    let Node::CreateSubscriptionStmt(connection) = parse_statement(
        "create subscription item_sub connection 'host=db.example dbname=app' publication item_pub, audit_pub with (enabled = true)",
    ) else {
        panic!("expected CreateSubscriptionStmt");
    };
    assert_eq!(connection.subname.as_deref(), Some("item_sub"));
    assert!(connection.conninfo.is_some());
    assert!(connection.servername.is_none());
    assert_eq!(connection.publication.len(), 2);
    assert_eq!(connection.options.len(), 1);

    let Node::CreateSubscriptionStmt(server) = parse_statement(
        "create subscription item_server_sub server logical_srv publication item_pub",
    ) else {
        panic!("expected CreateSubscriptionStmt");
    };
    assert_eq!(server.servername.as_deref(), Some("logical_srv"));
    assert!(server.conninfo.is_none());
}

#[test]
fn create_foreign_server_mapping_tablespace_and_access_method_are_strict() {
    let Node::CreateForeignTableStmt(table) = parse_statement(
        "create foreign table if not exists app.remote_orders (
             id bigint options (column_name 'remote_id'),
             payload text
         ) server foreign_srv options (schema_name 'public', table_name 'orders')",
    ) else {
        panic!("expected CreateForeignTableStmt");
    };
    assert!(table.base.if_not_exists);
    assert_eq!(table.base.table_elts.len(), 2);
    assert_eq!(
        table
            .base
            .relation
            .as_deref()
            .and_then(|relation| relation.schemaname.as_deref()),
        Some("app")
    );
    let Node::ColumnDef(id) = &table.base.table_elts[0] else {
        panic!("expected foreign ColumnDef");
    };
    assert_eq!(id.fdwoptions.len(), 1);
    assert_eq!(table.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(table.options.len(), 2);

    let Node::CreateForeignServerStmt(server) = parse_statement(
        "create server if not exists foreign_srv type 'postgres_fdw' version '16' foreign data wrapper postgres_fdw options (host 'db.example', port '5432')",
    ) else {
        panic!("expected CreateForeignServerStmt");
    };
    assert_eq!(server.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(server.servertype.as_deref(), Some("postgres_fdw"));
    assert_eq!(server.version.as_deref(), Some("16"));
    assert_eq!(server.fdwname.as_deref(), Some("postgres_fdw"));
    assert_eq!(server.options.len(), 2);

    let Node::CreateUserMappingStmt(mapping) = parse_statement(
        "create user mapping if not exists for current_user server foreign_srv options (user 'remote_user', password 'secret')",
    ) else {
        panic!("expected CreateUserMappingStmt");
    };
    assert!(mapping.if_not_exists);
    assert!(mapping.user.is_some());
    assert_eq!(mapping.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(mapping.options.len(), 2);

    let Node::CreateTableSpaceStmt(tablespace) = parse_statement(
        "create tablespace fast_space owner app_owner location '/srv/postgres/fast' with (random_page_cost = 1.1, storage.provider = custom)",
    ) else {
        panic!("expected CreateTableSpaceStmt");
    };
    assert_eq!(tablespace.tablespacename.as_deref(), Some("fast_space"));
    assert!(tablespace.owner.is_some());
    assert_eq!(tablespace.location.as_deref(), Some("/srv/postgres/fast"));
    assert_eq!(tablespace.options.len(), 2);
    assert!(matches!(
        tablespace.options.as_slice(),
        [Node::DefElem(cost), Node::DefElem(provider)]
            if cost.defnamespace.is_none()
                && cost.defname.as_deref() == Some("random_page_cost")
                && matches!(cost.arg.as_deref(), Some(Node::Float(_)))
                && provider.defnamespace.as_deref() == Some("storage")
                && provider.defname.as_deref() == Some("provider")
    ));

    let Node::CreateAmStmt(access_method) =
        parse_statement("create access method app_heap type table handler app.heap_handler")
    else {
        panic!("expected CreateAmStmt");
    };
    assert_eq!(access_method.amname.as_deref(), Some("app_heap"));
    assert_eq!(access_method.amtype, b't');
    assert_eq!(access_method.handler_name.len(), 2);

    let Node::CreateAmStmt(quoted) =
        parse_statement("create access method \"select\" type table handler app.select")
    else {
        panic!("expected CreateAmStmt");
    };
    assert_eq!(quoted.amname.as_deref(), Some("select"));
    assert!(matches!(
        quoted.handler_name.as_slice(),
        [Node::String(schema), Node::String(name)]
            if schema.sval.as_deref() == Some("app") && name.sval.as_deref() == Some("select")
    ));

    let Node::CreateAmStmt(index_am) =
        parse_statement("create access method app_index type index handler app.index_handler")
    else {
        panic!("expected index CreateAmStmt");
    };
    assert_eq!(index_am.amtype, b'i');
}

#[test]
fn create_fdw_cast_conversion_and_transform_populate_all_fields() {
    let Node::CreateFdwStmt(fdw) = parse_statement(
        "create foreign data wrapper app_fdw handler app.fdw_handler validator app.fdw_validator no connection options (host 'db.example', fetch_size '1000')",
    ) else {
        panic!("expected CreateFdwStmt");
    };
    assert_eq!(fdw.fdwname.as_deref(), Some("app_fdw"));
    assert_eq!(fdw.func_options.len(), 3);
    assert_eq!(fdw.options.len(), 2);

    let Node::CreateCastStmt(cast) = parse_statement(
        "create cast (app.source_value as app.target_value) with function app.cast_value(app.source_value) as assignment",
    ) else {
        panic!("expected CreateCastStmt");
    };
    assert!(cast.sourcetype.is_some());
    assert!(cast.targettype.is_some());
    assert!(cast.func.is_some());
    assert_eq!(cast.context, pg_parser::CoercionContext::Assignment);
    assert!(!cast.inout);

    let Node::CreateCastStmt(unspecified_cast) = parse_statement(
        "create cast (app.source_value as app.target_value) with function app.cast_value as implicit",
    ) else {
        panic!("expected CreateCastStmt with an unspecified function signature");
    };
    assert!(
        unspecified_cast
            .func
            .as_deref()
            .expect("cast function")
            .args_unspecified
    );

    let Node::CreateCastStmt(inout) =
        parse_statement("create cast (json as jsonb) with inout as implicit")
    else {
        panic!("expected CreateCastStmt");
    };
    assert!(inout.inout);
    assert_eq!(inout.context, pg_parser::CoercionContext::Implicit);

    let Node::CreateCastStmt(without_function) =
        parse_statement("create cast (app.binary_value as bytea) without function")
    else {
        panic!("expected WITHOUT FUNCTION CreateCastStmt");
    };
    assert!(without_function.func.is_none());
    assert!(!without_function.inout);
    assert_eq!(
        without_function.context,
        pg_parser::CoercionContext::Explicit
    );

    let Node::CreateConversionStmt(conversion) = parse_statement(
        "create default conversion app.utf8_to_latin for 'UTF8' to 'LATIN1' from app.convert_encoding",
    ) else {
        panic!("expected CreateConversionStmt");
    };
    assert!(conversion.def);
    assert_eq!(conversion.conversion_name.len(), 2);
    assert_eq!(conversion.for_encoding_name.as_deref(), Some("UTF8"));
    assert_eq!(conversion.to_encoding_name.as_deref(), Some("LATIN1"));
    assert_eq!(conversion.func_name.len(), 2);

    let Node::CreateTransformStmt(transform) = parse_statement(
        "create or replace transform for app.custom_type language plpgsql (from sql with function app.from_sql(app.custom_type), to sql with function app.to_sql(app.custom_type))",
    ) else {
        panic!("expected CreateTransformStmt");
    };
    assert!(transform.replace);
    assert!(transform.type_name.is_some());
    assert_eq!(transform.lang.as_deref(), Some("plpgsql"));
    assert!(transform.fromsql.is_some());
    assert!(transform.tosql.is_some());

    let Node::CreateTransformStmt(unspecified_transform) = parse_statement(
        "create transform for app.unspecified_type language sql
         (from sql with function app.from_sql)",
    ) else {
        panic!("expected transform with an unspecified function signature");
    };
    assert!(
        unspecified_transform
            .fromsql
            .as_deref()
            .expect("FROM SQL function")
            .args_unspecified
    );

    let Node::CreateTransformStmt(from_only) = parse_statement(
        "create transform for app.from_only language sql (from sql with function app.from_sql(app.from_only))",
    ) else {
        panic!("expected FROM-only CreateTransformStmt");
    };
    assert!(from_only.fromsql.is_some());
    assert!(from_only.tosql.is_none());

    let Node::CreateTransformStmt(to_only) = parse_statement(
        "create transform for app.to_only language sql (to sql with function app.to_sql(app.to_only))",
    ) else {
        panic!("expected TO-only CreateTransformStmt");
    };
    assert!(to_only.fromsql.is_none());
    assert!(to_only.tosql.is_some());

    let Node::CreateTransformStmt(reverse_order) = parse_statement(
        "create transform for app.reverse_type language sql (to sql with function app.to_sql(app.reverse_type), from sql with function app.from_sql(app.reverse_type))",
    ) else {
        panic!("expected reverse-order CreateTransformStmt");
    };
    assert!(reverse_order.fromsql.is_some());
    assert!(reverse_order.tosql.is_some());
}

#[test]
fn create_operator_class_family_and_rule_populate_nested_nodes() {
    let Node::CreateOpClassStmt(opclass) = parse_statement(
        "create operator class app.int_ops default for type int using btree family app.int_family as operator 1 = for search, operator 2 <(int, int) for order by app.int_family, operator 3 >(int, int), function 1 app.compare_int(int, int), function 2 (int, bigint) app.compare_mixed(int, bigint), storage bigint",
    ) else {
        panic!("expected CreateOpClassStmt");
    };
    assert!(opclass.is_default);
    assert_eq!(opclass.opclassname.len(), 2);
    assert_eq!(opclass.opfamilyname.len(), 2);
    assert_eq!(opclass.amname.as_deref(), Some("btree"));
    assert!(opclass.datatype.is_some());
    assert_eq!(opclass.items.len(), 6);
    assert!(
        opclass
            .items
            .iter()
            .all(|item| matches!(item, Node::CreateOpClassItem(_)))
    );
    let Node::CreateOpClassItem(search) = &opclass.items[0] else {
        panic!("expected search CreateOpClassItem");
    };
    assert!(matches!(
        search.name.as_deref(),
        Some(name) if !name.args_unspecified && name.objargs.is_empty()
    ));
    let Node::CreateOpClassItem(ordering) = &opclass.items[1] else {
        panic!("expected ordering CreateOpClassItem");
    };
    assert_eq!(ordering.itemtype, 1);
    assert_eq!(ordering.number, 2);
    assert!(matches!(
        ordering.order_family.as_slice(),
        [Node::String(schema), Node::String(family)]
            if schema.sval.as_deref() == Some("app")
                && family.sval.as_deref() == Some("int_family")
    ));
    let Node::CreateOpClassItem(no_purpose) = &opclass.items[2] else {
        panic!("expected no-purpose CreateOpClassItem");
    };
    assert!(no_purpose.order_family.is_empty());

    let Node::CreateOpClassItem(class_args) = &opclass.items[4] else {
        panic!("expected class-args CreateOpClassItem");
    };
    assert_eq!(class_args.itemtype, 2);
    assert_eq!(class_args.class_args.len(), 2);

    let Node::CreateOpClassItem(storage) = &opclass.items[5] else {
        panic!("expected storage CreateOpClassItem");
    };
    assert!(storage.storedtype.is_some());

    let Node::CreateOpFamilyStmt(family) =
        parse_statement("create operator family app.int_family using btree")
    else {
        panic!("expected CreateOpFamilyStmt");
    };
    assert_eq!(family.opfamilyname.len(), 2);
    assert_eq!(family.amname.as_deref(), Some("btree"));

    let Node::RuleStmt(rule) = parse_statement(
        "create or replace rule audit_items as on update to app.items where old.id > 0 do instead (notify audit_channel, 'updated'; update app.audit set item_id = new.id)",
    ) else {
        panic!("expected RuleStmt");
    };
    assert!(rule.replace);
    assert_eq!(rule.rulename.as_deref(), Some("audit_items"));
    assert!(rule.relation.is_some());
    assert!(rule.where_clause.is_some());
    assert!(rule.instead);
    assert_eq!(rule.event, CmdType::Update);
    assert_eq!(rule.actions.len(), 2);

    let Node::RuleStmt(empty_actions) =
        parse_statement("create rule no_actions as on insert to app.items do ()")
    else {
        panic!("expected empty-action RuleStmt");
    };
    assert!(!empty_actions.instead);
    assert!(empty_actions.actions.is_empty());

    let Node::RuleStmt(single_select) = parse_statement(
        "create rule select_action as on select to app.items do also select * from app.audit",
    ) else {
        panic!("expected SELECT-action RuleStmt");
    };
    assert_eq!(single_select.actions.len(), 1);
    assert!(matches!(single_select.actions[0], Node::SelectStmt(_)));

    let Node::RuleStmt(mixed_actions) = parse_statement(
        "create rule mixed_actions as on insert to app.items do (;; insert into app.audit values (new.id);; delete from app.pending where id = new.id;;)",
    ) else {
        panic!("expected mixed-action RuleStmt");
    };
    assert!(matches!(
        mixed_actions.actions.as_slice(),
        [Node::InsertStmt(_), Node::DeleteStmt(_)]
    ));
}

#[test]
fn define_stmt_populates_aggregate_operator_type_collation_and_text_search() {
    let Node::DefineStmt(aggregate) = parse_statement(
        "create or replace aggregate app.sum2(int) (sfunc = app.sum_state, stype = bigint, initcond = '0')",
    ) else {
        panic!("expected aggregate DefineStmt");
    };
    assert_eq!(aggregate.kind, pg_parser::ObjectType::Aggregate);
    assert!(aggregate.replace);
    assert!(!aggregate.oldstyle);
    assert_eq!(aggregate.args.len(), 2);
    assert_eq!(aggregate.definition.len(), 3);

    let Node::DefineStmt(old_aggregate) = parse_statement(
        "create aggregate app.old_sum (sfunc = app.sum_state, basetype = int, stype = bigint, initcond = '0', finalfunc = none)",
    ) else {
        panic!("expected old-style aggregate DefineStmt");
    };
    assert_eq!(old_aggregate.kind, pg_parser::ObjectType::Aggregate);
    assert!(old_aggregate.oldstyle);
    assert!(old_aggregate.args.is_empty());
    assert_eq!(old_aggregate.definition.len(), 5);
    let Node::DefElem(finalfunc) = &old_aggregate.definition[4] else {
        panic!("expected finalfunc DefElem");
    };
    assert!(
        matches!(finalfunc.arg.as_deref(), Some(Node::String(value)) if value.sval.as_deref() == Some("none"))
    );

    let Node::DefineStmt(operator) = parse_statement(
        "create operator === (leftarg = int, rightarg = int, function = app.eq_int)",
    ) else {
        panic!("expected operator DefineStmt");
    };
    assert_eq!(operator.kind, pg_parser::ObjectType::Operator);
    assert_eq!(operator.defnames.len(), 1);
    assert_eq!(operator.definition.len(), 3);
    let Node::DefElem(leftarg) = &operator.definition[0] else {
        panic!("expected leftarg DefElem");
    };
    assert!(matches!(leftarg.arg.as_deref(), Some(Node::TypeName(_))));
    let Node::DefElem(function) = &operator.definition[2] else {
        panic!("expected function DefElem");
    };
    assert!(matches!(function.arg.as_deref(), Some(Node::TypeName(_))));

    let Node::DefineStmt(base_type) = parse_statement(
        "create type app.custom_value (input = app.custom_in, output = app.custom_out)",
    ) else {
        panic!("expected type DefineStmt");
    };
    assert_eq!(base_type.kind, pg_parser::ObjectType::Type);
    assert_eq!(base_type.definition.len(), 2);

    let Node::DefineStmt(collation) =
        parse_statement("create collation if not exists app.english from pg_catalog.english")
    else {
        panic!("expected collation DefineStmt");
    };
    assert_eq!(collation.kind, pg_parser::ObjectType::Collation);
    assert!(collation.if_not_exists);
    assert_eq!(collation.definition.len(), 1);

    let Node::DefineStmt(search) = parse_statement(
        "create text search configuration app.english_search (parser = pg_catalog.default)",
    ) else {
        panic!("expected text search DefineStmt");
    };
    assert_eq!(search.kind, pg_parser::ObjectType::Tsconfiguration);
    assert_eq!(search.definition.len(), 1);

    for (sql, expected) in [
        (
            "create text search parser app.custom_parser (start = app.parser_start)",
            pg_parser::ObjectType::Tsparser,
        ),
        (
            "create text search dictionary app.custom_dictionary (template = pg_catalog.simple)",
            pg_parser::ObjectType::Tsdictionary,
        ),
        (
            "create text search template app.custom_template (lexize = app.template_lexize)",
            pg_parser::ObjectType::Tstemplate,
        ),
    ] {
        let Node::DefineStmt(stmt) = parse_statement(sql) else {
            panic!("expected text search DefineStmt");
        };
        assert_eq!(stmt.kind, expected);
        assert_eq!(stmt.definition.len(), 1);
    }
}

#[test]
fn create_operator_definition_preserves_explicit_qualified_operator_values() {
    let Node::DefineStmt(operator) = parse_statement(
        "create operator app.=== (
            leftarg = integer,
            rightarg = integer,
            function = app.equal_integer,
            commutator = operator(app.===)
        )",
    ) else {
        panic!("expected operator DefineStmt");
    };
    assert!(matches!(
        operator.defnames.as_slice(),
        [Node::String(schema), Node::String(operator)]
            if schema.sval.as_deref() == Some("app")
                && operator.sval.as_deref() == Some("===")
    ));
    let commutator = operator
        .definition
        .iter()
        .find_map(|node| match node {
            Node::DefElem(def) if def.defname.as_deref() == Some("commutator") => Some(def),
            _ => None,
        })
        .expect("commutator DefElem");
    assert!(matches!(
        commutator.arg.as_deref(),
        Some(Node::AArrayExpr(names))
            if matches!(names.elements.as_slice(),
                [Node::String(schema), Node::String(operator)]
                    if schema.sval.as_deref() == Some("app")
                        && operator.sval.as_deref() == Some("===")
            )
    ));
}

#[test]
fn create_ordered_aggregate_preserves_direct_count_and_variadic_normalization() {
    let Node::DefineStmt(aggregate) = parse_statement(
        "create aggregate app.collect(
             variadic direct_values integer[]
             order by variadic ordered_values integer[]
         ) (sfunc = app.collect_state, stype = bigint)",
    ) else {
        panic!("expected aggregate DefineStmt");
    };
    let [Node::AArrayExpr(parameters), Node::Integer(direct_count)] = aggregate.args.as_slice()
    else {
        panic!("expected aggregate argument pair");
    };
    assert_eq!(direct_count.ival, 1);
    assert_eq!(parameters.elements.len(), 1);
    let Node::FunctionParameter(parameter) = &parameters.elements[0] else {
        panic!("expected FunctionParameter");
    };
    assert_eq!(parameter.mode, FunctionParameterMode::Variadic);
    assert_eq!(parameter.name.as_deref(), Some("direct_values"));

    let Node::DefineStmt(ordered_only) = parse_statement(
        "create aggregate app.rank_value(order by value integer)
         (sfunc = app.rank_state, stype = bigint)",
    ) else {
        panic!("expected aggregate DefineStmt");
    };
    assert!(matches!(
        ordered_only.args.as_slice(),
        [Node::AArrayExpr(parameters), Node::Integer(count)]
            if parameters.elements.len() == 1 && count.ival == 0
    ));
}

#[test]
fn create_policy_trigger_constraint_trigger_and_event_trigger_are_complete() {
    let Node::CreatePolicyStmt(policy) = parse_statement(
        "create policy restricted_items on app.items as restrictive for update to app_user, auditor using (owner_id = current_user) with check (active = true)",
    ) else {
        panic!("expected CreatePolicyStmt");
    };
    assert!(!policy.permissive);
    assert_eq!(policy.policy_name.as_deref(), Some("restricted_items"));
    assert_eq!(
        policy
            .table
            .as_deref()
            .and_then(|table| table.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(policy.cmd_name.as_deref(), Some("update"));
    assert_eq!(policy.roles.len(), 2);
    assert!(policy.qual.is_some());
    assert!(policy.with_check.is_some());

    let Node::CreatePolicyStmt(default_policy) =
        parse_statement("create policy visible_items on app.items")
    else {
        panic!("expected default CreatePolicyStmt");
    };
    assert!(default_policy.permissive);
    assert_eq!(default_policy.cmd_name.as_deref(), Some("all"));
    assert!(matches!(
        default_policy.roles.as_slice(),
        [Node::RoleSpec(role)]
            if role.roletype == pg_parser::RoleSpecType::Public
                && role.rolename.is_none()
                && role.location == -1
    ));

    let Node::CreateTrigStmt(trigger) = parse_statement(
        "create trigger audit_columns before update of name, status or insert on app.items for each row when (new.active is true) execute function app.audit_trigger(7, 1.5, plain, 'change')",
    ) else {
        panic!("expected CreateTrigStmt");
    };
    assert!(!trigger.isconstraint);
    assert!(!trigger.replace);
    assert_eq!(
        trigger
            .relation
            .as_deref()
            .and_then(|relation| relation.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(trigger.timing, 2);
    assert_eq!(trigger.events, 20);
    assert_eq!(trigger.columns.len(), 2);
    assert!(matches!(
        trigger.args.as_slice(),
        [Node::String(integer), Node::String(float), Node::String(label), Node::String(string)]
            if integer.sval.as_deref() == Some("7")
                && float.sval.as_deref() == Some("1.5")
                && label.sval.as_deref() == Some("plain")
                && string.sval.as_deref() == Some("change")
    ));
    assert!(trigger.row);
    assert!(trigger.when_clause.is_some());

    let Node::CreateTrigStmt(for_row) = parse_statement(
        "create trigger row_without_each after insert on app.items for row execute function app.audit_trigger()",
    ) else {
        panic!("expected FOR ROW CreateTrigStmt");
    };
    assert!(for_row.row);

    let Node::CreateTrigStmt(constraint_trigger) = parse_statement(
        "create constraint trigger check_parent after insert or update on app.children from app.parents deferrable initially deferred for each row execute function app.check_parent()",
    ) else {
        panic!("expected constraint CreateTrigStmt");
    };
    assert!(constraint_trigger.isconstraint);
    assert!(constraint_trigger.constrrel.is_some());
    assert!(constraint_trigger.deferrable);
    assert!(constraint_trigger.initdeferred);
    assert!(constraint_trigger.row);

    let Node::CreateTrigStmt(enforced_constraint) = parse_statement(
        "create constraint trigger enforced_parent after insert on app.children enforced for row execute function app.check_parent()",
    ) else {
        panic!("expected ENFORCED constraint CreateTrigStmt");
    };
    assert!(enforced_constraint.isconstraint);
    assert!(enforced_constraint.row);

    let Node::CreateTrigStmt(implied_deferrable) = parse_statement(
        "create constraint trigger deferred_parent after insert on app.children initially deferred for each row execute function app.check_parent()",
    ) else {
        panic!("expected implied-deferrable CreateTrigStmt");
    };
    assert!(implied_deferrable.deferrable);
    assert!(implied_deferrable.initdeferred);

    let Node::CreateEventTrigStmt(event_trigger) = parse_statement(
        "create event trigger ddl_audit on ddl_command_end when tag in ('CREATE TABLE', 'ALTER TABLE') and schema in ('public') execute function app.audit_ddl()",
    ) else {
        panic!("expected CreateEventTrigStmt");
    };
    assert_eq!(event_trigger.trigname.as_deref(), Some("ddl_audit"));
    assert_eq!(event_trigger.eventname.as_deref(), Some("ddl_command_end"));
    assert_eq!(event_trigger.whenclause.len(), 2);
    assert_eq!(event_trigger.funcname.len(), 2);

    let Node::CreateEventTrigStmt(no_when) = parse_statement(
        "create event trigger ddl_simple on ddl_command_start execute procedure app.audit_ddl()",
    ) else {
        panic!("expected no-WHEN CreateEventTrigStmt");
    };
    assert!(no_when.whenclause.is_empty());
    assert_eq!(no_when.funcname.len(), 2);
}

#[test]
fn create_materialized_view_populates_into_clause_and_data_option() {
    let Node::CreateTableAsStmt(stmt) = parse_statement(
        "create materialized view if not exists app.item_summary(id) using heap with (fillfactor = 80) tablespace fast_space as select id from app.items with no data",
    ) else {
        panic!("expected CreateTableAsStmt");
    };
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
fn create_property_graph_populates_keys_references_labels_and_properties() {
    let Node::CreatePropGraphStmt(graph) = parse_statement(
        "create property graph app.social vertex tables (app.users as u key (id) label person properties (name as display_name, age)) edge tables (app.follows as f key (id) source key (source_id) references u (id) destination key (target_id) references u (id) label follows properties all columns)",
    ) else {
        panic!("expected CreatePropGraphStmt");
    };
    assert_eq!(
        graph
            .pgname
            .as_deref()
            .and_then(|name| name.schemaname.as_deref()),
        Some("app")
    );
    assert_eq!(graph.vertex_tables.len(), 1);
    assert_eq!(graph.edge_tables.len(), 1);

    let Node::PropGraphVertex(vertex) = &graph.vertex_tables[0] else {
        panic!("expected PropGraphVertex");
    };
    assert_eq!(
        vertex
            .vtable
            .as_deref()
            .and_then(|table| table.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("u")
    );
    assert_eq!(vertex.vkey.len(), 1);
    let Node::PropGraphLabelAndProperties(label) = &vertex.labels[0] else {
        panic!("expected PropGraphLabelAndProperties");
    };
    assert_eq!(label.label.as_deref(), Some("person"));
    let properties: &PropGraphProperties = label.properties.as_deref().expect("properties");
    assert!(!properties.all);
    assert_eq!(
        label
            .properties
            .as_deref()
            .expect("properties")
            .properties
            .len(),
        2
    );

    let Node::PropGraphEdge(edge) = &graph.edge_tables[0] else {
        panic!("expected PropGraphEdge");
    };
    assert_eq!(edge.ekey.len(), 1);
    assert_eq!(edge.esrckey.len(), 1);
    assert_eq!(edge.esrcvertex.as_deref(), Some("u"));
    assert_eq!(edge.esrcvertexcols.len(), 1);
    assert_eq!(edge.edestkey.len(), 1);
    assert_eq!(edge.edestvertex.as_deref(), Some("u"));
    assert_eq!(edge.edestvertexcols.len(), 1);

    let Node::CreatePropGraphStmt(default_graph) =
        parse_statement("create property graph g vertex tables (t)")
    else {
        panic!("expected CreatePropGraphStmt");
    };
    let Node::PropGraphVertex(vertex) = &default_graph.vertex_tables[0] else {
        panic!("expected PropGraphVertex");
    };
    let Node::PropGraphLabelAndProperties(defaults) = &vertex.labels[0] else {
        panic!("expected PropGraphLabelAndProperties");
    };
    assert_eq!(defaults.location, -1);
    assert_eq!(
        defaults.properties.as_deref().expect("properties").location,
        -1
    );

    let sql = "create property graph g vertex tables (t label item)";
    let Node::CreatePropGraphStmt(labeled_graph) = parse_statement(sql) else {
        panic!("expected CreatePropGraphStmt");
    };
    let Node::PropGraphVertex(vertex) = &labeled_graph.vertex_tables[0] else {
        panic!("expected PropGraphVertex");
    };
    let Node::PropGraphLabelAndProperties(label) = &vertex.labels[0] else {
        panic!("expected PropGraphLabelAndProperties");
    };
    assert_eq!(label.location, sql.find("label").unwrap() as i32);
    assert_eq!(
        label.properties.as_deref().expect("properties").location,
        -1
    );

    for (modifier, expected) in [
        ("temp", b't'),
        ("temporary", b't'),
        ("local temp", b't'),
        ("global temporary", b't'),
        ("unlogged", b'u'),
    ] {
        let Node::CreatePropGraphStmt(graph) =
            parse_statement(&format!("create {modifier} property graph g"))
        else {
            panic!("expected CreatePropGraphStmt");
        };
        assert_eq!(
            graph.pgname.as_deref().map(|name| name.relpersistence),
            Some(expected)
        );
    }
}
