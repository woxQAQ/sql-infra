use super::*;

#[test]
fn create_stats_stmt_wraps_columns_and_expressions_in_stats_elems() {
    let stmt = parse_node!(
        "create statistics if not exists app.item_stats (ndistinct, dependencies) on category, lower(name), cast(price as bigint), (price * quantity) from app.items",
        CreateStatsStmt
    );
    assert!(stmt.if_not_exists);
    assert_eq!(stmt.defnames.len(), 2);
    assert_eq!(stmt.stat_types.len(), 2);
    assert_eq!(stmt.exprs.len(), 4);
    assert_eq!(stmt.relations.len(), 1);
    assert!(stmt.stxcomment.is_none());
    assert!(!stmt.transformed);

    let column = expect_node!(&stmt.exprs[0], StatsElem);
    assert_eq!(column.name.as_deref(), Some("category"));
    assert!(column.expr.is_none());

    let expression = expect_node!(&stmt.exprs[1], StatsElem);
    assert!(expression.name.is_none());
    assert!(expression.expr.is_some());

    let cast = expect_node!(&stmt.exprs[2], StatsElem);
    assert!(matches!(cast.expr.as_deref(), Some(Node::TypeCast(_))));

    let parenthesized = expect_node!(&stmt.exprs[3], StatsElem);
    assert!(parenthesized.name.is_none());
    assert!(matches!(
        parenthesized.expr.as_deref(),
        Some(Node::AExpr(_))
    ));

    let anonymous = parse_node!(
        "create statistics on category from app.items",
        CreateStatsStmt
    );
    assert!(anonymous.defnames.is_empty());
}

#[test]
fn create_table_stmt_populates_like_inheritance_partition_and_storage_clauses() {
    let sql = "create table events (like event_template including defaults excluding indexes, id int) inherits (base_events) partition by range (created_at collate pg_catalog.\"C\" app.timestamp_ops, lower(id), (id + 1)) using heap with (fillfactor = 80) tablespace fast_space";
    let stmt = parse_node!(sql, CreateStmt);
    assert_eq!(stmt.table_elts.len(), 2);
    assert_eq!(stmt.inh_relations.len(), 1);
    assert!(stmt.nnconstraints.is_empty());
    assert_eq!(stmt.access_method.as_deref(), Some("heap"));
    assert_eq!(stmt.options.len(), 1);
    assert_eq!(stmt.tablespacename.as_deref(), Some("fast_space"));

    let like = expect_node!(&stmt.table_elts[0], TableLikeClause);
    assert!(like.relation.is_some());
    assert_ne!(like.options, 0);

    let partspec = stmt.partspec.expect("PartitionSpec");
    assert_eq!(partspec.strategy, PartitionStrategy::Range);
    assert_eq!(
        partspec.location as usize,
        sql.find("partition by").unwrap()
    );
    assert_eq!(partspec.part_params.len(), 3);
    let partition = expect_node!(&partspec.part_params[0], PartitionElem);
    assert_eq!(partition.name.as_deref(), Some("created_at"));
    assert_eq!(partition.collation.len(), 2);
    assert_eq!(partition.opclass.len(), 2);
    assert_eq!(partition.location as usize, sql.find("created_at").unwrap());
    let function = expect_node!(&partspec.part_params[1], PartitionElem);
    assert!(matches!(function.expr.as_deref(), Some(Node::FuncCall(_))));
    assert_eq!(function.location as usize, sql.find("lower(id)").unwrap());
    let expression = expect_node!(&partspec.part_params[2], PartitionElem);
    assert!(matches!(expression.expr.as_deref(), Some(Node::AExpr(_))));
    assert_eq!(expression.location as usize, sql.find("(id + 1)").unwrap());
}

#[test]
fn create_table_like_options_follow_ordered_bitmask_semantics() {
    let stmt = parse_node!(
        "create table copied (like source including all excluding indexes excluding storage, like fallback excluding all including defaults)",
        CreateStmt
    );
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
    let stmt = parse_node!(sql, CreateStmt);
    let type_name = stmt.of_typename.as_deref().expect("typed table type");
    assert_eq!(
        type_name
            .names
            .iter()
            .map(|name| {
                expect_node!(name, String)
                    .sval
                    .as_deref()
                    .expect("type name")
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
    let column = expect_node!(&stmt.table_elts[0], ColumnDef);
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
    let regular = parse_node!(
        "create table regular_not_null (id int, not null id not valid no inherit)",
        CreateStmt
    );
    assert!(matches!(
        regular.table_elts.as_slice(),
        [Node::ColumnDef(_), Node::Constraint(constraint)]
            if constraint.contype == ConstrType::Notnull
                && constraint.conname.is_none()
                && constraint.keys.len() == 1
                && constraint.skip_validation
                && constraint.is_no_inherit
    ));

    let typed = parse_node!(
        "create table typed_not_null of app.item_type (name with options collate pg_catalog.\"C\", not null name)",
        CreateStmt
    );
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
    let stmt = parse_node!(
        "create table orders (id bigint generated always as identity (start with 10 increment by 5) primary key, account_id bigint constraint orders_account_fk references accounts(id) on update cascade on delete set null, amount numeric(12,2) default 0 check (amount >= 0) not null, slug text collate pg_catalog.c unique nulls not distinct, computed int generated always as (amount::int) stored, constraint orders_amount_check check (amount < 100000) not valid, constraint orders_account_unique unique (account_id, slug) include (amount) with (fillfactor = 80) using index tablespace fast_space, constraint orders_fk foreign key (account_id) references accounts(id) match full on delete cascade deferrable initially deferred)",
        CreateStmt
    );
    assert_eq!(stmt.table_elts.len(), 8);

    let id = expect_node!(&stmt.table_elts[0], ColumnDef);
    assert_eq!(id.colname.as_deref(), Some("id"));
    assert_eq!(id.constraints.len(), 2);
    let identity = expect_node!(&id.constraints[0], Constraint);
    assert_eq!(identity.contype, ConstrType::Identity);
    assert_eq!(identity.generated_when, b'a');
    assert_eq!(identity.options.len(), 2);
    let primary = expect_node!(&id.constraints[1], Constraint);
    assert_eq!(primary.contype, ConstrType::Primary);

    let account_id = expect_node!(&stmt.table_elts[1], ColumnDef);
    let column_fk = expect_node!(&account_id.constraints[0], Constraint);
    assert_eq!(column_fk.contype, ConstrType::Foreign);
    assert_eq!(column_fk.conname.as_deref(), Some("orders_account_fk"));
    assert!(column_fk.pktable.is_some());
    assert_eq!(column_fk.pk_attrs.len(), 1);
    assert_eq!(column_fk.fk_upd_action, b'c');
    assert_eq!(column_fk.fk_del_action, b'n');

    let amount = expect_node!(&stmt.table_elts[2], ColumnDef);
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

    let slug = expect_node!(&stmt.table_elts[3], ColumnDef);
    assert!(slug.coll_clause.is_some());
    assert!(matches!(
        &slug.constraints[0],
        Node::Constraint(c) if c.contype == ConstrType::Unique && c.nulls_not_distinct
    ));

    let computed = expect_node!(&stmt.table_elts[4], ColumnDef);
    let generated = expect_node!(&computed.constraints[0], Constraint);
    assert_eq!(generated.contype, ConstrType::Generated);
    assert_eq!(generated.generated_kind, b's');
    assert!(generated.raw_expr.is_some());

    let check = expect_node!(&stmt.table_elts[5], Constraint);
    assert_eq!(check.conname.as_deref(), Some("orders_amount_check"));
    assert_eq!(check.contype, ConstrType::Check);
    assert!(check.skip_validation);
    assert!(!check.initially_valid);

    let unique = expect_node!(&stmt.table_elts[6], Constraint);
    assert_eq!(unique.contype, ConstrType::Unique);
    assert_eq!(unique.keys.len(), 2);
    assert_eq!(unique.including.len(), 1);
    assert_eq!(unique.options.len(), 1);
    assert_eq!(unique.indexspace.as_deref(), Some("fast_space"));

    let foreign = expect_node!(&stmt.table_elts[7], Constraint);
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
    let stmt = parse_node!(
        "create table defaults (compared boolean default 1 is not distinct from 2 not null, grouped boolean default (true and false), ordinary int default 1 + 2)",
        CreateStmt
    );
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
    let stmt = parse_node!(
        "create table generated_modes (id bigint generated by default as identity (cache 8), implicit_virtual int generated always as (id + 1), explicit_virtual int generated always as (id + 2) virtual)",
        CreateStmt
    );
    let columns = stmt
        .table_elts
        .iter()
        .filter_map(|element| match element {
            Node::ColumnDef(column) => Some(column),
            _ => None,
        })
        .collect::<Vec<_>>();
    let identity = expect_node!(&columns[0].constraints[0], Constraint);
    assert_eq!(identity.contype, ConstrType::Identity);
    assert_eq!(identity.generated_when, b'd');
    assert_eq!(identity.options.len(), 1);
    for column in &columns[1..] {
        let generated = expect_node!(&column.constraints[0], Constraint);
        assert_eq!(generated.contype, ConstrType::Generated);
        assert_eq!(generated.generated_when, b'a');
        assert_eq!(generated.generated_kind, b'v');
        assert!(generated.raw_expr.is_some());
    }
}

#[test]
fn create_table_column_constraint_attributes_remain_raw_constraint_nodes() {
    let stmt = parse_node!(
        "create table raw_attributes (id int unique deferrable initially deferred enforced, parent_id int references parent(id) not deferrable initially immediate not enforced)",
        CreateStmt
    );
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
        .map(|node| expect_node!(node, Constraint).contype)
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
        .map(|node| expect_node!(node, Constraint).contype)
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
    let stmt = parse_node!(
        "create table child (
             id integer,
             parent_id integer,
             constraint child_parent_fk foreign key (parent_id)
                 references parent(id) initially deferred,
             constraint positive_id check (id > 0) not enforced,
             constraint present_parent not null parent_id not valid no inherit
         )",
        CreateStmt
    );
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
    let stmt = parse_node!(
        "create table child (
             id integer,
             valid_at daterange,
             foreign key (id, period valid_at)
                 references parent (id, period valid_at)
         )",
        CreateStmt
    );
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
    let stmt = parse_node!(
        "create table child (tenant_id int, parent_id int, valid_at daterange, unique (tenant_id, valid_at without overlaps), foreign key (tenant_id, parent_id) references parent (tenant_id, id) on delete set null (parent_id))",
        CreateStmt
    );
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
    let stmt = parse_node!(
        "create table indexed_constraints (id int, code int, constraint indexed_unique unique using index existing_unique deferrable initially deferred, constraint indexed_primary primary key using index existing_primary not deferrable initially immediate)",
        CreateStmt
    );
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
    let qualified = parse_node!("create table app.select (id integer)", CreateStmt);
    let relation = qualified.relation.as_deref().expect("RangeVar");
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("select"));

    let catalog_qualified =
        parse_node!("create table current_db.app.items (id integer)", CreateStmt);
    let relation = catalog_qualified.relation.as_deref().expect("RangeVar");
    assert_eq!(relation.catalogname.as_deref(), Some("current_db"));
    assert_eq!(relation.schemaname.as_deref(), Some("app"));
    assert_eq!(relation.relname.as_deref(), Some("items"));

    let quoted = parse_node!("create table \"select\" (id integer)", CreateStmt);
    assert_eq!(
        quoted
            .relation
            .as_deref()
            .and_then(|range| range.relname.as_deref()),
        Some("select")
    );

    let quoted_column = parse_node!(
        "create table names (\"select\" integer storage default compression default)",
        CreateStmt
    );
    let [Node::ColumnDef(column)] = quoted_column.table_elts.as_slice() else {
        panic!("expected quoted ColumnDef");
    };
    assert_eq!(column.colname.as_deref(), Some("select"));
    assert_eq!(column.storage_name.as_deref(), Some("default"));
    assert_eq!(column.compression.as_deref(), Some("default"));
}

#[test]
fn create_table_exclusion_constraint_preserves_index_payload() {
    let stmt = parse_node!(
        "create table reservations (room int, during tstzrange, constraint no_overlap exclude using gist (lower(room) collate pg_catalog.\"C\" app.text_ops desc nulls last with =, during with operator(pg_catalog.&&)) include (room) with (fillfactor = 80) using index tablespace fast_space where (room > 0) deferrable initially immediate)",
        CreateStmt
    );
    let exclusion = expect_node!(&stmt.table_elts[2], Constraint);
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
    let first_pair = expect_node!(&exclusion.exclusions[0], AArrayExpr);
    let first_element = expect_node!(&first_pair.elements[0], IndexElem);
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
    let stmt = parse_node!(
        "create table typed_values (id int, amount numeric(12,2), label character varying(30), flags bit varying(8), created timestamp(3) with time zone, tags app.tag_type[][], numbers int array[4])",
        CreateStmt
    );
    assert_eq!(stmt.table_elts.len(), 7);

    let type_name = |index: usize| {
        let column = expect_node!(&stmt.table_elts[index], ColumnDef);
        column.type_name.as_deref().expect("TypeName")
    };
    let names = |index: usize| {
        type_name(index)
            .names
            .iter()
            .map(|node| {
                expect_node!(node, String)
                    .sval
                    .as_deref()
                    .expect("type name")
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
fn create_partition_stmt_populates_range_and_hash_bounds() {
    let range_sql = "create table events_2026 partition of events for values from (minvalue, '2026-01-01') to (maxvalue, '2027-01-01')";
    let range = parse_node!(range_sql, CreateStmt);
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
    let hash = parse_node!(hash_sql, CreateStmt);
    let hash_bound = hash.partbound.expect("PartitionBoundSpec");
    assert_eq!(hash_bound.strategy, b'h');
    assert_eq!(hash_bound.location as usize, hash_sql.find("with").unwrap());
    assert_eq!(hash_bound.modulus, 4);
    assert_eq!(hash_bound.remainder, 1);

    let list_sql =
        "create table events_active partition of events_list for values in ('active', 'pending')";
    let list = parse_node!(list_sql, CreateStmt);
    let list_bound = list.partbound.expect("PartitionBoundSpec");
    assert_eq!(list_bound.strategy, b'l');
    assert_eq!(list_bound.location as usize, list_sql.find("in (").unwrap());
    assert_eq!(list_bound.listdatums.len(), 2);
    assert_eq!(list_bound.modulus, 0);
    assert_eq!(list_bound.remainder, 0);

    let default_sql = "create table events_default partition of events_list default";
    let default = parse_node!(default_sql, CreateStmt);
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
fn create_relation_persistence_modifiers_reach_raw_rangevars() {
    let temporary = parse_node!(
        "create local temporary table temp_items (id integer)",
        CreateStmt
    );
    assert_eq!(
        temporary
            .relation
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b't')
    );

    let unlogged = parse_node!(
        "create unlogged table staging_items (id integer)",
        CreateStmt
    );
    assert_eq!(
        unlogged
            .relation
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b'u')
    );

    let sequence = parse_node!("create global temp sequence temp_item_seq", CreateSeqStmt);
    assert_eq!(
        sequence
            .sequence
            .as_deref()
            .map(|range| range.relpersistence),
        Some(b't')
    );

    let matview = parse_node!(
        "create unlogged materialized view item_ids as select 1 as id",
        CreateTableAsStmt
    );
    assert_eq!(
        matview
            .into
            .as_deref()
            .and_then(|into| into.rel.as_deref())
            .map(|range| range.relpersistence),
        Some(b'u')
    );

    let ctas = parse_node!(
        "create temp table copied_items as select 1 as id",
        CreateTableAsStmt
    );
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
    let stmt = parse_node!(
        "create temp table if not exists copied_items(id, label)
         using heap with (fillfactor = 80) on commit drop tablespace fast_space
         as select 1, 'item' with no data",
        CreateTableAsStmt
    );
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

    let without_oids = parse_node!(
        "create table copied_without_oids without oids as select 1",
        CreateTableAsStmt
    );
    assert!(
        without_oids
            .into
            .as_deref()
            .is_some_and(|into| into.options.is_empty() && !into.skip_data)
    );
}

#[test]
fn create_table_as_execute_preserves_nested_execute_and_data_clause() {
    let stmt = parse_node!(
        "create temp table if not exists executed_result(id) as execute prepared_query(1, 'x') with no data",
        CreateTableAsStmt
    );
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

    let with_data = parse_node!(
        "create table executed_result as execute prepared_query with data",
        CreateTableAsStmt
    );
    assert!(!with_data.into.as_deref().expect("IntoClause").skip_data);
}

#[test]
fn create_regular_and_foreign_tables_accept_empty_optional_element_lists() {
    let regular = parse_node!("create table empty_table ()", CreateStmt);
    assert!(regular.table_elts.is_empty());

    let foreign = parse_node!(
        "create foreign table empty_foreign () server foreign_server",
        CreateForeignTableStmt
    );
    assert!(foreign.base.table_elts.is_empty());

    let typed = parse_node!("create table typed of app.item_type", CreateStmt);
    assert!(typed.table_elts.is_empty());
    assert!(typed.of_typename.is_some());
}
