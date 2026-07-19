use super::*;

#[test]
fn call_stmt_preserves_the_raw_function_call_only() {
    let stmt = parse_node!("call app.process_order(42, urgent => true)", CallStmt);
    let call = stmt.funccall.as_deref().expect("raw FuncCall");
    assert_eq!(call.funcname.len(), 2);
    assert_eq!(call.args.len(), 2);
    assert!(
        matches!(call.args.get(1), Some(Node::NamedArgExpr(arg)) if arg.name.as_deref() == Some("urgent"))
    );
    assert!(stmt.funcexpr.is_none());
    assert!(stmt.outargs.is_empty());

    let ordered = parse_node!("call app.collect(1 order by 2 desc)", CallStmt);
    assert_eq!(
        ordered
            .funccall
            .as_deref()
            .expect("ordered FuncCall")
            .agg_order
            .len(),
        1
    );

    let variadic = parse_node!("call app.collect(1, variadic values)", CallStmt);
    assert!(
        variadic
            .funccall
            .as_deref()
            .expect("variadic FuncCall")
            .func_variadic
    );

    let distinct = parse_node!("call app.collect(distinct value)", CallStmt);
    assert!(
        distinct
            .funccall
            .as_deref()
            .expect("distinct FuncCall")
            .agg_distinct
    );

    let all = parse_node!("call app.collect(all value order by value)", CallStmt);
    let all = all.funccall.as_deref().expect("ALL FuncCall");
    assert!(!all.agg_distinct);
    assert_eq!(all.args.len(), 1);
    assert_eq!(all.agg_order.len(), 1);

    let star = parse_node!("call app.collect(*)", CallStmt);
    assert!(star.funccall.as_deref().expect("star FuncCall").agg_star);
}

#[test]
fn repack_reassign_comment_and_security_label_populate_targets() {
    let all_relations = parse_node!("repack", RepackStmt);
    assert_eq!(all_relations.command, RepackCommand::Repack);
    assert!(all_relations.relation.is_none());
    assert!(!all_relations.usingindex);
    assert!(all_relations.indexname.is_none());

    let single_relation = parse_node!("repack app.items", RepackStmt);
    assert!(single_relation.relation.is_some());
    assert!(!single_relation.usingindex);
    assert!(single_relation.indexname.is_none());

    let repack = parse_node!(
        "repack (verbose true) app.items(id) using index item_idx",
        RepackStmt
    );
    assert_eq!(repack.command, RepackCommand::Repack);
    assert!(repack.usingindex);
    assert_eq!(repack.indexname.as_deref(), Some("item_idx"));
    assert_eq!(repack.params.len(), 1);
    assert_eq!(
        repack
            .relation
            .as_ref()
            .map(|relation| relation.va_cols.len()),
        Some(1)
    );

    let repack_only = parse_node!("repack only app.items using index", RepackStmt);
    assert!(repack_only.usingindex);
    assert!(repack_only.indexname.is_none());
    assert!(
        !repack_only
            .relation
            .as_deref()
            .and_then(|relation| relation.relation.as_deref())
            .expect("relation")
            .inh
    );

    let all_using_index = parse_node!("repack using index", RepackStmt);
    assert!(all_using_index.relation.is_none());
    assert!(all_using_index.usingindex);

    for sql in [
        "cluster",
        "cluster verbose",
        "cluster (verbose true)",
        "cluster app.items",
        "cluster (verbose true) app.items using item_idx",
        "cluster verbose item_idx on app.items",
    ] {
        let cluster = parse_node!(sql, RepackStmt);
        assert_eq!(cluster.command, RepackCommand::Cluster, "{sql}");
        assert!(cluster.usingindex, "{sql}");
    }

    let old_cluster = parse_node!("cluster verbose item_idx on app.items", RepackStmt);
    assert_eq!(old_cluster.indexname.as_deref(), Some("item_idx"));
    assert_eq!(old_cluster.params.len(), 1);
    assert!(old_cluster.relation.is_some());

    let option_cluster = parse_node!(
        "cluster (verbose true, workers 2) app.items using item_idx",
        RepackStmt
    );
    assert_eq!(option_cluster.params.len(), 2);
    assert_eq!(option_cluster.indexname.as_deref(), Some("item_idx"));
    assert!(option_cluster.relation.is_some());

    let reassign = parse_node!(
        "reassign owned by old_owner, current_user to new_owner",
        ReassignOwnedStmt
    );
    assert_eq!(reassign.roles.len(), 2);
    assert!(matches!(
        reassign.roles.as_slice(),
        [Node::RoleSpec(_), Node::RoleSpec(_)]
    ));
    assert!(reassign.newrole.is_some());

    let comment = parse_node!(
        "comment on table app.items is 'application items'",
        CommentStmt
    );
    assert_eq!(comment.objtype, ObjectType::Table);
    assert!(comment.object.is_some());
    assert_eq!(comment.comment.as_deref(), Some("application items"));

    let label = parse_node!(
        "security label for selinux on table app.items is 'system_u:object_r:table_t:s0'",
        SecLabelStmt
    );
    assert_eq!(label.provider.as_deref(), Some("selinux"));
    assert_eq!(label.objtype, ObjectType::Table);
    assert!(label.object.is_some());
    assert!(label.label.is_some());
}

#[test]
fn comment_and_security_label_build_object_type_specific_identities() {
    let function = parse_node!(
        "comment on function app.normalize(int, text) is 'normalizer'",
        CommentStmt
    );
    assert_eq!(function.objtype, ObjectType::Function);
    assert!(matches!(
        function.object.as_deref(),
        Some(Node::ObjectWithArgs(object))
            if object.objname.len() == 2 && object.objargs.len() == 2
    ));

    let cast = parse_node!(
        "comment on cast (int as text) is 'integer to text'",
        CommentStmt
    );
    assert_eq!(cast.objtype, ObjectType::Cast);
    assert!(matches!(
        cast.object.as_deref(),
        Some(Node::AArrayExpr(types))
            if types.elements.iter().all(|node| matches!(node, Node::TypeName(_)))
    ));

    let table_constraint = parse_node!(
        "comment on constraint positive_amount on app.orders is 'positive amount'",
        CommentStmt
    );
    assert_eq!(table_constraint.objtype, ObjectType::Tabconstraint);
    assert!(matches!(
        table_constraint.object.as_deref(),
        Some(Node::AArrayExpr(identity)) if identity.elements.len() == 3
    ));

    let domain_constraint = parse_node!(
        "comment on constraint valid_value on domain app.positive_int is 'valid value'",
        CommentStmt
    );
    assert_eq!(domain_constraint.objtype, ObjectType::Domconstraint);
    assert!(matches!(
        domain_constraint.object.as_deref(),
        Some(Node::AArrayExpr(identity))
            if matches!(identity.elements.first(), Some(Node::TypeName(_)))
    ));

    let trigger = parse_node!(
        "comment on trigger audit on app.orders is null",
        CommentStmt
    );
    assert_eq!(trigger.objtype, ObjectType::Trigger);
    assert!(trigger.comment.is_none());

    let opclass = parse_node!(
        "comment on operator class app.int_ops using btree is 'integer ops'",
        CommentStmt
    );
    assert_eq!(opclass.objtype, ObjectType::Opclass);
    assert!(matches!(
        opclass.object.as_deref(),
        Some(Node::AArrayExpr(identity)) if identity.elements.len() == 3
    ));

    let operator = parse_node!(
        "comment on operator app.-(none, int) is 'integer negation'",
        CommentStmt
    );
    let signature = expect_node!(operator.object.as_deref(), Some(ObjectWithArgs));
    assert!(matches!(
        signature.objargs.as_slice(),
        [None, Some(Node::TypeName(_))]
    ));

    let transform = parse_node!(
        "comment on transform for app.custom_type language plpgsql is 'custom transform'",
        CommentStmt
    );
    assert_eq!(transform.objtype, ObjectType::Transform);
    assert!(matches!(
        transform.object.as_deref(),
        Some(Node::AArrayExpr(identity))
            if matches!(identity.elements.first(), Some(Node::TypeName(_)))
    ));

    let function_label = parse_node!(
        "security label for 'selinux' on function app.normalize(int, text) is 'system_u:object_r:function_t:s0'",
        SecLabelStmt
    );
    assert_eq!(function_label.provider.as_deref(), Some("selinux"));
    assert_eq!(function_label.objtype, ObjectType::Function);
    assert!(matches!(
        function_label.object.as_deref(),
        Some(Node::ObjectWithArgs(_))
    ));
}

#[test]
fn comment_and_security_label_cover_every_grammar_object_family() {
    let common_objects = [
        ("table app.items", ObjectType::Table),
        ("sequence app.item_ids", ObjectType::Sequence),
        ("view app.item_view", ObjectType::View),
        ("materialized view app.item_cache", ObjectType::Matview),
        ("index app.item_idx", ObjectType::Index),
        ("foreign table app.remote_items", ObjectType::ForeignTable),
        ("property graph app.item_graph", ObjectType::Propgraph),
        ("collation app.item_collation", ObjectType::Collation),
        ("conversion app.item_conversion", ObjectType::Conversion),
        ("statistics app.item_stats", ObjectType::StatisticExt),
        ("text search parser app.item_parser", ObjectType::Tsparser),
        (
            "text search dictionary app.item_dictionary",
            ObjectType::Tsdictionary,
        ),
        (
            "text search template app.item_template",
            ObjectType::Tstemplate,
        ),
        (
            "text search configuration app.item_configuration",
            ObjectType::Tsconfiguration,
        ),
        ("column app.items.name", ObjectType::Column),
        ("access method item_am", ObjectType::AccessMethod),
        ("event trigger item_ddl", ObjectType::EventTrigger),
        ("extension item_extension", ObjectType::Extension),
        ("foreign data wrapper item_fdw", ObjectType::Fdw),
        ("procedural language item_lang", ObjectType::Language),
        ("language sql", ObjectType::Language),
        ("publication item_publication", ObjectType::Publication),
        ("schema app", ObjectType::Schema),
        ("server item_server", ObjectType::ForeignServer),
        ("database item_database", ObjectType::Database),
        ("role item_role", ObjectType::Role),
        ("subscription item_subscription", ObjectType::Subscription),
        ("tablespace item_tablespace", ObjectType::Tablespace),
        ("type app.item_type", ObjectType::Type),
        ("domain app.item_domain", ObjectType::Domain),
        ("aggregate app.item_agg(*)", ObjectType::Aggregate),
        ("function app.item_fn()", ObjectType::Function),
        ("procedure app.item_proc(int)", ObjectType::Procedure),
        ("routine app.item_routine(text)", ObjectType::Routine),
        ("large object 42", ObjectType::Largeobject),
    ];

    for (object, expected_type) in common_objects {
        let comment = parse_node!(&format!("comment on {object} is 'comment'"), CommentStmt);
        assert_eq!(comment.objtype, expected_type, "COMMENT ON {object}");
        assert!(comment.object.is_some(), "COMMENT ON {object}");

        let label = parse_node!(
            &format!("security label on {object} is 'label'"),
            SecLabelStmt
        );
        assert_eq!(label.objtype, expected_type, "SECURITY LABEL ON {object}");
        assert!(label.object.is_some(), "SECURITY LABEL ON {object}");
    }

    let comment_only_objects = [
        ("operator app.+(int, int)", ObjectType::Operator),
        (
            "constraint positive on app.items",
            ObjectType::Tabconstraint,
        ),
        (
            "constraint positive on domain app.item_domain",
            ObjectType::Domconstraint,
        ),
        ("policy item_policy on app.items", ObjectType::Policy),
        ("rule item_rule on app.items", ObjectType::Rule),
        ("trigger item_trigger on app.items", ObjectType::Trigger),
        (
            "transform for app.item_type language sql",
            ObjectType::Transform,
        ),
        (
            "operator class app.item_ops using btree",
            ObjectType::Opclass,
        ),
        (
            "operator family app.item_ops using btree",
            ObjectType::Opfamily,
        ),
        ("cast (int as text)", ObjectType::Cast),
    ];
    for (object, expected_type) in comment_only_objects {
        let comment = parse_node!(&format!("comment on {object} is 'comment'"), CommentStmt);
        assert_eq!(comment.objtype, expected_type, "COMMENT ON {object}");
        assert!(comment.object.is_some(), "COMMENT ON {object}");
    }
}
