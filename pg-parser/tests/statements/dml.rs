use pg_parser::CmdType;
use pg_parser::LockClauseStrength;
use pg_parser::MergeMatchKind;
use pg_parser::Node;
use pg_parser::OnConflictAction;
use pg_parser::OverridingKind;
use pg_parser::ReturningOptionKind;

use super::common::parse_statement;

#[test]
fn insert_accepts_the_grammar_valid_empty_select_source() {
    let Node::InsertStmt(stmt) = parse_statement("insert into items select") else {
        panic!("expected InsertStmt");
    };
    assert!(matches!(
        stmt.select_stmt.as_deref(),
        Some(Node::SelectStmt(select)) if select.target_list.is_empty()
    ));
}

#[test]
fn dml_statements_preserve_with_clauses() {
    for sql in [
        "with x as (select 1) insert into t select * from x",
        "with x as (select 1) update t set id = 1 from x",
        "with x as (select 1) delete from t using x",
        "with x as (select 1) merge into t using x on true when matched then do nothing",
    ] {
        let node = parse_statement(sql);
        let with_clause = match &node {
            Node::InsertStmt(stmt) => &stmt.with_clause,
            Node::UpdateStmt(stmt) => &stmt.with_clause,
            Node::DeleteStmt(stmt) => &stmt.with_clause,
            Node::MergeStmt(stmt) => &stmt.with_clause,
            other => panic!("unexpected DML node for {sql}: {other:?}"),
        };
        assert!(with_clause.is_some(), "{sql}");
    }
}

#[test]
fn dml_target_aliases_follow_statement_specific_colid_rules() {
    let Node::UpdateStmt(update) = parse_statement("update items abort set id = 1") else {
        panic!("expected UpdateStmt");
    };
    assert_eq!(
        update
            .relation
            .as_deref()
            .and_then(|relation| relation.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("abort")
    );

    let Node::UpdateStmt(set_column) = parse_statement("update items set set = 1") else {
        panic!("expected SET-column UpdateStmt");
    };
    assert!(
        set_column
            .relation
            .as_deref()
            .is_some_and(|relation| relation.alias.is_none())
    );

    let Node::DeleteStmt(delete) = parse_statement("delete from items set") else {
        panic!("expected DeleteStmt with SET alias");
    };
    assert_eq!(
        delete
            .relation
            .as_deref()
            .and_then(|relation| relation.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("set")
    );

    let Node::MergeStmt(merge) =
        parse_statement("merge into items set using source on true when matched then do nothing")
    else {
        panic!("expected MergeStmt with SET alias");
    };
    assert_eq!(
        merge
            .relation
            .as_deref()
            .and_then(|relation| relation.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("set")
    );
}

#[test]
fn insert_stmt_populates_relation_source_conflict_and_returning() {
    let Node::InsertStmt(stmt) = parse_statement(
        "insert into public.items (id, name) values (1, 'one') on conflict do nothing returning id",
    ) else {
        panic!("expected InsertStmt");
    };
    assert!(stmt.relation.is_some());
    assert_eq!(stmt.cols.len(), 2);
    assert!(stmt.select_stmt.is_some());
    assert!(stmt.on_conflict_clause.is_some());
    assert!(stmt.returning_clause.is_some());
}

#[test]
fn insert_stmt_preserves_column_field_and_subscript_indirection() {
    let Node::InsertStmt(stmt) = parse_statement(
        "insert into items (payload.name, nums[1], nums[2:4]) values ('x', 1, array[2, 3, 4])",
    ) else {
        panic!("expected InsertStmt");
    };
    assert_eq!(stmt.cols.len(), 3);
    let Node::ResTarget(field) = &stmt.cols[0] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(field.indirection.as_slice(), [Node::String(_)]));
    assert!(field.location >= 0);
    let Node::ResTarget(index) = &stmt.cols[1] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        index.indirection.as_slice(),
        [Node::AIndices(index)] if !index.is_slice
    ));
    let Node::ResTarget(slice) = &stmt.cols[2] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice
    ));

    let Node::InsertStmt(quoted) = parse_statement("insert into items (\"select\") values (1)")
    else {
        panic!("expected InsertStmt");
    };
    assert!(matches!(
        quoted.cols.as_slice(),
        [Node::ResTarget(target)] if target.name.as_deref() == Some("select")
    ));
}

#[test]
fn insert_stmt_requires_as_for_alias_and_accepts_parenthesized_select_source() {
    let Node::InsertStmt(stmt) =
        parse_statement("insert into items as target (id) (select id from source)")
    else {
        panic!("expected InsertStmt");
    };
    assert_eq!(
        stmt.relation
            .as_deref()
            .and_then(|relation| relation.alias.as_deref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("target")
    );
    assert!(matches!(
        stmt.select_stmt.as_deref(),
        Some(Node::SelectStmt(_))
    ));

    for (sql, columns, override_, has_source) in [
        (
            "insert into items select id from source",
            0,
            OverridingKind::NotSet,
            true,
        ),
        (
            "insert into items overriding user value values (1)",
            0,
            OverridingKind::UserValue,
            true,
        ),
        (
            "insert into items (id) select id from source",
            1,
            OverridingKind::NotSet,
            true,
        ),
        (
            "insert into items (id) overriding system value select id from source",
            1,
            OverridingKind::SystemValue,
            true,
        ),
        (
            "insert into items default values",
            0,
            OverridingKind::NotSet,
            false,
        ),
        (
            "insert into items with source as (select 1 as id) select id from source",
            0,
            OverridingKind::NotSet,
            true,
        ),
        (
            "insert into items with source as (select 1 as id) (select id from source)",
            0,
            OverridingKind::NotSet,
            true,
        ),
    ] {
        let Node::InsertStmt(stmt) = parse_statement(sql) else {
            panic!("expected InsertStmt for {sql}");
        };
        assert_eq!(stmt.cols.len(), columns, "{sql}");
        assert_eq!(stmt.override_, override_, "{sql}");
        assert_eq!(stmt.select_stmt.is_some(), has_source, "{sql}");
        assert!(
            stmt.select_stmt
                .as_deref()
                .is_none_or(|source| matches!(source, Node::SelectStmt(_))),
            "{sql}"
        );
    }
}

#[test]
fn insert_stmt_populates_override_inference_update_and_returning_options() {
    let sql = "insert into items (id, name) overriding system value values (1, 'one') on conflict (id) where id > 0 do update set name = 'updated' where items.id > 0 returning with (old as previous, new as current) id";
    let Node::InsertStmt(stmt) = parse_statement(sql) else {
        panic!("expected InsertStmt");
    };
    assert_eq!(stmt.override_, OverridingKind::SystemValue);
    let conflict = stmt.on_conflict_clause.expect("OnConflictClause");
    assert_eq!(conflict.action, OnConflictAction::Update);
    assert_eq!(conflict.location as usize, sql.find("on conflict").unwrap());
    let infer = conflict.infer.expect("InferClause");
    assert_eq!(infer.index_elems.len(), 1);
    assert_eq!(infer.location as usize, sql.find("(id)").unwrap());
    assert!(infer.where_clause.is_some());
    assert_eq!(conflict.target_list.len(), 1);
    assert!(conflict.where_clause.is_some());

    let returning = stmt.returning_clause.expect("ReturningClause");
    assert_eq!(returning.options.len(), 2);
    assert_eq!(returning.exprs.len(), 1);
    let Node::ReturningOption(old) = &returning.options[0] else {
        panic!("expected ReturningOption");
    };
    assert_eq!(old.option, ReturningOptionKind::Old);
    assert_eq!(old.value.as_deref(), Some("previous"));
    assert_eq!(old.location as usize, sql.find("old as").unwrap());
    let Node::ReturningOption(new) = &returning.options[1] else {
        panic!("expected ReturningOption");
    };
    assert_eq!(new.option, ReturningOptionKind::New);
    assert_eq!(new.location as usize, sql.find("new as").unwrap());

    let on_constraint_sql =
        "insert into items values (1) on conflict on constraint items_pkey do nothing";
    let Node::InsertStmt(on_constraint) = parse_statement(on_constraint_sql) else {
        panic!("expected ON CONSTRAINT InsertStmt");
    };
    let infer = on_constraint
        .on_conflict_clause
        .as_deref()
        .and_then(|conflict| conflict.infer.as_deref())
        .expect("ON CONSTRAINT InferClause");
    assert_eq!(infer.conname.as_deref(), Some("items_pkey"));
    assert_eq!(
        infer.location,
        on_constraint_sql.find("on constraint").unwrap() as i32
    );

    let inference_sql = "insert into items values (1) on conflict (
        name collate pg_catalog.\"C\" text_pattern_ops desc nulls first,
        lower(code) app.custom_ops asc nulls last,
        (id + 1)
    ) where active do nothing";
    let Node::InsertStmt(inference) = parse_statement(inference_sql) else {
        panic!("expected inference IndexElem InsertStmt");
    };
    let infer = inference
        .on_conflict_clause
        .as_deref()
        .and_then(|conflict| conflict.infer.as_deref())
        .expect("InferClause");
    let [
        Node::IndexElem(name),
        Node::IndexElem(function),
        Node::IndexElem(expression),
    ] = infer.index_elems.as_slice()
    else {
        panic!("expected three inference IndexElem nodes");
    };
    assert_eq!(name.name.as_deref(), Some("name"));
    assert_eq!(name.collation.len(), 2);
    assert_eq!(name.opclass.len(), 1);
    assert_eq!(name.ordering, pg_parser::SortByDir::Desc);
    assert_eq!(name.nulls_ordering, pg_parser::SortByNulls::First);
    assert_eq!(
        name.location as usize,
        inference_sql.find("name collate").unwrap()
    );
    assert!(matches!(function.expr.as_deref(), Some(Node::FuncCall(_))));
    assert_eq!(function.opclass.len(), 2);
    assert_eq!(function.ordering, pg_parser::SortByDir::Asc);
    assert_eq!(function.nulls_ordering, pg_parser::SortByNulls::Last);
    assert_eq!(
        function.location as usize,
        inference_sql.find("lower(code)").unwrap()
    );
    assert!(matches!(expression.expr.as_deref(), Some(Node::AExpr(_))));
    assert_eq!(
        expression.location as usize,
        inference_sql.find("(id + 1)").unwrap()
    );
    assert!(infer.where_clause.is_some());
}

#[test]
fn insert_on_conflict_select_preserves_lock_strength_and_filter() {
    let Node::InsertStmt(stmt) = parse_statement(
        "insert into items (id) values (1) on conflict (id) do select for no key update where items.active returning id",
    ) else {
        panic!("expected InsertStmt");
    };
    let conflict = stmt.on_conflict_clause.expect("OnConflictClause");
    assert_eq!(conflict.action, OnConflictAction::Select);
    assert_eq!(conflict.lock_strength, LockClauseStrength::Fornokeyupdate);
    assert!(conflict.target_list.is_empty());
    assert!(conflict.where_clause.is_some());

    let Node::InsertStmt(unlocked) =
        parse_statement("insert into items values (1) on conflict do select")
    else {
        panic!("expected unlocked InsertStmt");
    };
    let conflict = unlocked.on_conflict_clause.expect("OnConflictClause");
    assert_eq!(conflict.action, OnConflictAction::Select);
    assert_eq!(conflict.lock_strength, LockClauseStrength::None);
}

#[test]
fn update_stmt_populates_assignments_from_filter_and_returning() {
    let sql = "update public.items set name = 'updated' from audit where items.id = audit.id returning items.id";
    let Node::UpdateStmt(stmt) = parse_statement(sql) else {
        panic!("expected UpdateStmt");
    };
    assert!(stmt.relation.is_some());
    assert_eq!(stmt.target_list.len(), 1);
    assert_eq!(stmt.from_clause.len(), 1);
    assert!(stmt.where_clause.is_some());
    assert!(stmt.returning_clause.is_some());

    let Node::ResTarget(target) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    assert_eq!(target.name.as_deref(), Some("name"));
    assert!(target.val.is_some());
    assert_eq!(target.location as usize, sql.find("name =").unwrap());

    let Node::UpdateStmt(quoted) = parse_statement("update items set \"select\" = 1") else {
        panic!("expected UpdateStmt");
    };
    assert!(matches!(
        quoted.target_list.as_slice(),
        [Node::ResTarget(target)] if target.name.as_deref() == Some("select")
    ));
}

#[test]
fn update_stmt_builds_multi_assign_refs() {
    let sql = "update items set (name, status) = row('updated', 'active')";
    let Node::UpdateStmt(stmt) = parse_statement(sql) else {
        panic!("expected UpdateStmt");
    };
    assert_eq!(stmt.target_list.len(), 2);
    for (index, target) in stmt.target_list.iter().enumerate() {
        let Node::ResTarget(target) = target else {
            panic!("expected ResTarget");
        };
        let Some(value) = &target.val else {
            panic!("expected MultiAssignRef");
        };
        let Node::MultiAssignRef(reference) = value.as_ref() else {
            panic!("expected MultiAssignRef");
        };
        assert_eq!(reference.colno, index as i32 + 1);
        assert_eq!(reference.ncolumns, 2);
        assert!(reference.source.is_some());
        let expected = if index == 0 { "name" } else { "status" };
        assert_eq!(target.location as usize, sql.find(expected).unwrap());
    }
}

#[test]
fn update_stmt_preserves_field_subscript_and_slice_assignment_indirection() {
    let Node::UpdateStmt(stmt) = parse_statement(
        "update items set payload.name = 'x', nums[2] = 5, nums[1:3] = array[1, 2, 3], (left_side.field, right_side[1]) = row('l', 'r')",
    ) else {
        panic!("expected UpdateStmt");
    };
    assert_eq!(stmt.target_list.len(), 5);
    let Node::ResTarget(field) = &stmt.target_list[0] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(field.indirection.as_slice(), [Node::String(_)]));
    let Node::ResTarget(index) = &stmt.target_list[1] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        index.indirection.as_slice(),
        [Node::AIndices(index)] if !index.is_slice && index.uidx.is_some()
    ));
    let Node::ResTarget(slice) = &stmt.target_list[2] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        slice.indirection.as_slice(),
        [Node::AIndices(index)] if index.is_slice && index.lidx.is_some() && index.uidx.is_some()
    ));
    let Node::ResTarget(multi_field) = &stmt.target_list[3] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        multi_field.indirection.as_slice(),
        [Node::String(_)]
    ));
    let Node::ResTarget(multi_index) = &stmt.target_list[4] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(
        multi_index.indirection.as_slice(),
        [Node::AIndices(_)]
    ));
}

#[test]
fn update_and_delete_build_default_and_current_of_nodes() {
    let Node::UpdateStmt(update) = parse_statement("update items set status = default") else {
        panic!("expected UpdateStmt");
    };
    let Node::ResTarget(target) = &update.target_list[0] else {
        panic!("expected ResTarget");
    };
    assert!(matches!(target.val.as_deref(), Some(Node::SetToDefault(_))));

    let Node::DeleteStmt(delete) =
        parse_statement("delete from items where current of item_cursor")
    else {
        panic!("expected DeleteStmt");
    };
    assert!(matches!(
        delete.where_clause.as_deref(),
        Some(Node::CurrentOfExpr(current))
            if current.cursor_name.as_deref() == Some("item_cursor")
                && current.cursor_param == 0
                && current.cvarno == 0
    ));

    let Node::UpdateStmt(update) =
        parse_statement("update items set status = 'active' where current of item_cursor")
    else {
        panic!("expected UpdateStmt");
    };
    assert!(matches!(
        update.where_clause.as_deref(),
        Some(Node::CurrentOfExpr(current))
            if current.cursor_name.as_deref() == Some("item_cursor")
                && current.cursor_param == 0
                && current.cvarno == 0
    ));
}

#[test]
fn update_and_delete_populate_for_portion_of_clause() {
    let update_sql = "update items for portion of valid_time from lower_bound to upper_bound as current_items set status = 'active'";
    let Node::UpdateStmt(update) = parse_statement(update_sql) else {
        panic!("expected UpdateStmt");
    };
    let portion = update.for_portion_of.expect("ForPortionOfClause");
    assert_eq!(portion.range_name.as_deref(), Some("valid_time"));
    assert_eq!(
        portion.location as usize,
        update_sql.find("valid_time").unwrap()
    );
    assert_eq!(
        portion.target_location as usize,
        update_sql.find("from").unwrap()
    );
    assert!(portion.target_start.is_some());
    assert!(portion.target_end.is_some());
    assert_eq!(
        update
            .relation
            .as_ref()
            .and_then(|relation| relation.alias.as_ref())
            .and_then(|alias| alias.aliasname.as_deref()),
        Some("current_items")
    );

    let delete_sql = "delete from items for portion of valid_time (requested_period)";
    let Node::DeleteStmt(delete) = parse_statement(delete_sql) else {
        panic!("expected DeleteStmt");
    };
    let portion = delete.for_portion_of.expect("ForPortionOfClause");
    assert!(portion.target.is_some());
    assert_eq!(
        portion.location as usize,
        delete_sql.find("valid_time").unwrap()
    );
    assert_eq!(
        portion.target_location as usize,
        delete_sql.find("requested_period").unwrap()
    );
}

#[test]
fn delete_stmt_populates_using_filter_and_returning() {
    let Node::DeleteStmt(stmt) = parse_statement(
        "delete from public.items using audit where items.id = audit.id returning items.id",
    ) else {
        panic!("expected DeleteStmt");
    };
    assert!(stmt.relation.is_some());
    assert_eq!(stmt.using_clause.len(), 1);
    assert!(stmt.where_clause.is_some());
    assert!(stmt.returning_clause.is_some());
}

#[test]
fn dml_returning_options_are_preserved_across_statement_families() {
    for sql in [
        "update items set value = 1 returning with (old as previous, new as current) current.value",
        "delete from items returning with (old as previous, new as current) previous.id",
        "merge into items using source on items.id = source.id when matched then do nothing returning with (old as previous, new as current) current.id",
    ] {
        let returning = match parse_statement(sql) {
            Node::UpdateStmt(stmt) => stmt.returning_clause,
            Node::DeleteStmt(stmt) => stmt.returning_clause,
            Node::MergeStmt(stmt) => stmt.returning_clause,
            other => panic!("expected DML statement, got {other:?}"),
        }
        .expect("ReturningClause");
        assert_eq!(returning.options.len(), 2, "{sql}");
        assert_eq!(returning.exprs.len(), 1, "{sql}");
        assert!(matches!(
            returning.options.as_slice(),
            [Node::ReturningOption(old), Node::ReturningOption(new)]
                if old.option == ReturningOptionKind::Old
                    && new.option == ReturningOptionKind::New
        ));
    }
}

#[test]
fn merge_stmt_populates_match_kinds_actions_and_values() {
    let Node::MergeStmt(stmt) = parse_statement(
        "merge into target t using source s on t.id = s.id when matched and s.deleted = true then delete when matched then update set name = s.name when not matched by target then insert (id, name) overriding system value values (s.id, s.name) when not matched by source then do nothing returning t.id",
    ) else {
        panic!("expected MergeStmt");
    };
    assert!(stmt.relation.is_some());
    assert!(stmt.source_relation.is_some());
    assert!(stmt.join_condition.is_some());
    assert_eq!(stmt.merge_when_clauses.len(), 4);

    let expected = [
        (MergeMatchKind::Matched, CmdType::Delete),
        (MergeMatchKind::Matched, CmdType::Update),
        (MergeMatchKind::NotMatchedByTarget, CmdType::Insert),
        (MergeMatchKind::NotMatchedBySource, CmdType::Nothing),
    ];
    for (node, expected) in stmt.merge_when_clauses.iter().zip(expected) {
        let Node::MergeWhenClause(clause) = node else {
            panic!("expected MergeWhenClause");
        };
        assert_eq!((clause.match_kind, clause.command_type), expected);
    }
    let Node::MergeWhenClause(conditional_delete) = &stmt.merge_when_clauses[0] else {
        panic!("expected MergeWhenClause");
    };
    assert!(matches!(
        conditional_delete.condition.as_deref(),
        Some(Node::AExpr(_))
    ));
    let Node::MergeWhenClause(unconditional_update) = &stmt.merge_when_clauses[1] else {
        panic!("expected MergeWhenClause");
    };
    assert!(unconditional_update.condition.is_none());

    let Node::MergeWhenClause(insert) = &stmt.merge_when_clauses[2] else {
        panic!("expected MergeWhenClause");
    };
    assert_eq!(insert.target_list.len(), 2);
    assert_eq!(insert.values.len(), 2);
    assert_eq!(insert.override_, OverridingKind::SystemValue);
    assert!(stmt.returning_clause.is_some());

    let Node::MergeStmt(join_source) = parse_statement(
        "merge into target t using source_a a join source_b b on a.id = b.id on t.id = a.id when matched then do nothing",
    ) else {
        panic!("expected MergeStmt with joined source");
    };
    assert!(matches!(
        join_source.source_relation.as_deref(),
        Some(Node::JoinExpr(join)) if join.quals.is_some()
    ));
    assert!(matches!(
        join_source.join_condition.as_deref(),
        Some(Node::AExpr(_))
    ));

    for (sql, match_kind, command_type, override_) in [
        (
            "merge into t using s on true when matched then do nothing",
            MergeMatchKind::Matched,
            CmdType::Nothing,
            OverridingKind::NotSet,
        ),
        (
            "merge into t using s on true when not matched by source then update set id = s.id",
            MergeMatchKind::NotMatchedBySource,
            CmdType::Update,
            OverridingKind::NotSet,
        ),
        (
            "merge into t using s on true when not matched by source then delete",
            MergeMatchKind::NotMatchedBySource,
            CmdType::Delete,
            OverridingKind::NotSet,
        ),
        (
            "merge into t using s on true when not matched then do nothing",
            MergeMatchKind::NotMatchedByTarget,
            CmdType::Nothing,
            OverridingKind::NotSet,
        ),
        (
            "merge into t using s on true when not matched then insert overriding user value values (s.id)",
            MergeMatchKind::NotMatchedByTarget,
            CmdType::Insert,
            OverridingKind::UserValue,
        ),
        (
            "merge into t using s on true when not matched by target then insert default values",
            MergeMatchKind::NotMatchedByTarget,
            CmdType::Insert,
            OverridingKind::NotSet,
        ),
    ] {
        let Node::MergeStmt(stmt) = parse_statement(sql) else {
            panic!("expected MergeStmt for {sql}");
        };
        let [Node::MergeWhenClause(clause)] = stmt.merge_when_clauses.as_slice() else {
            panic!("expected one MergeWhenClause for {sql}");
        };
        assert_eq!(clause.match_kind, match_kind, "{sql}");
        assert_eq!(clause.command_type, command_type, "{sql}");
        assert_eq!(clause.override_, override_, "{sql}");
    }
}
