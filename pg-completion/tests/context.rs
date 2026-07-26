use pg_completion::{GrammarSlot, IdentifierQuoting, ObjectKind, RecoveryKind, collect};
use pg_parser::{TextRange, TextSize, TokenKind};

fn size(value: usize) -> TextSize {
    TextSize::try_from(value).unwrap()
}

#[test]
fn isolates_the_statement_and_extracts_the_prefix() {
    let source = "select broken;  SELECT db.Users.Na FROM db.Users";
    let point = source.find("Na").unwrap() + 2;
    let context = collect(source, size(point));

    assert_eq!(
        context.statement_range,
        TextRange::new(size(16), size(source.len()))
    );
    assert_eq!(context.point, size(point));
    assert_eq!(
        context.replacement_range,
        TextRange::new(size(point - 2), size(point))
    );
    assert_eq!(context.prefix.raw, "Na");
    assert_eq!(context.prefix.normalized, "na");
    assert_eq!(context.prefix.quoting, IdentifierQuoting::Unquoted);
    assert_eq!(
        context
            .intent
            .qualifier
            .iter()
            .map(|part| part.normalized.as_str())
            .collect::<Vec<_>>(),
        ["db", "users"]
    );
}

#[test]
fn keeps_a_point_in_leading_statement_trivia() {
    let context = collect("   select 1", TextSize::ZERO);
    assert_eq!(context.statement_range.start(), TextSize::ZERO);
    assert_eq!(context.point, TextSize::ZERO);
}

#[test]
fn isolates_empty_and_lexically_contained_statement_boundaries() {
    let source = "SELECT 1; ; SELECT 2";
    let empty_terminator = source.find("; ;").unwrap() + 2;
    let context = collect(source, size(empty_terminator));
    assert_eq!(
        context.statement_range,
        TextRange::empty(size(empty_terminator))
    );
    assert!(context.expectations.tokens.contains(&TokenKind::Select));

    let source = "SELECT ';' /* ; */; SELECT 2";
    let point = source.find("/* ; */").unwrap() + 3;
    let context = collect(source, size(point));
    assert_eq!(context.statement_range.start(), TextSize::ZERO);
    assert_eq!(
        context.statement_range.end(),
        size(source.find("; SELECT 2").unwrap())
    );
    assert!(context.expectations.tokens.is_empty());
    assert!(context.expectations.slots.is_empty());
}

#[test]
fn reports_point_normalization_without_panicking() {
    let source = "名";
    let context = collect(source, TextSize::new(2));
    assert_eq!(context.point, TextSize::ZERO);
    assert_eq!(
        context.recovery.issues[0].kind,
        RecoveryKind::PointMovedToCharBoundary
    );

    let context = collect(source, TextSize::new(10));
    assert_eq!(context.point, size(source.len()));
    assert_eq!(
        context.recovery.issues[0].kind,
        RecoveryKind::PointClampedToEof
    );
}

#[test]
fn combines_parser_candidates_intent_and_forward_scope() {
    let source = "SELECT  FROM users AS u";
    let context = collect(source, size(7));

    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    assert!(context.expectations.slots.contains(&GrammarSlot::Function));
    assert!(context.expectations.tokens.contains(&TokenKind::From));
    assert!(context.intent.object_kinds.contains(&ObjectKind::Column));
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "u"
    );
}

#[test]
fn filters_keywords_by_prefix_and_resolves_relation_qualifiers() {
    let context = collect("SEL", size(3));
    assert_eq!(context.expectations.tokens, [TokenKind::Select]);

    let source = "SELECT u. FROM users AS u";
    let point = source.find(" FROM").unwrap();
    let context = collect(source, size(point));
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
    assert_eq!(context.intent.qualifier[0].normalized, "u");

    let source = "SELECT accounts. FROM accounts AS a";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(
        context.expectations.slots,
        [GrammarSlot::Column, GrammarSlot::Function]
    );

    let source = "WITH c AS (SELECT 1) SELECT c.";
    let context = collect(source, size(source.len()));
    assert_eq!(
        context.expectations.slots,
        [GrammarSlot::Column, GrammarSlot::Function]
    );

    let source = "WITH c AS (SELECT 1) SELECT c. FROM c";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
}

#[test]
fn resolves_qualifiers_in_a_parenthesized_query_suffix() {
    let source = "(SELECT * FROM users AS u) ORDER BY u.";
    let context = collect(source, size(source.len()));

    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "u"
    );
}

#[test]
fn set_operation_suffix_does_not_expose_a_branch_relation() {
    for source in [
        "SELECT a FROM left_table UNION SELECT b FROM right_table ORDER BY ",
        "(SELECT a FROM left_table UNION SELECT b FROM right_table) ORDER BY ",
    ] {
        let context = collect(source, size(source.len()));
        assert!(context.scope.local.relations.is_empty(), "{source:?}");
        assert!(context.scope.outer.is_empty(), "{source:?}");
    }
}

#[test]
fn filters_phrases_by_their_head_token_prefix() {
    let source = "SELECT * FROM t GRO";
    let context = collect(source, size(source.len()));
    assert_eq!(context.expectations.tokens, [TokenKind::GroupP]);
    assert_eq!(
        context.expectations.phrases,
        [&[TokenKind::GroupP, TokenKind::By][..]]
    );

    let source = "SELECT * FROM t ORD";
    let context = collect(source, size(source.len()));
    assert_eq!(
        context.expectations.phrases,
        [&[TokenKind::Order, TokenKind::By][..]]
    );
}

#[test]
fn exposes_catalog_containers_for_nested_object_candidates() {
    let cases = [
        (
            "ALTER TABLE app.accounts DROP COLUMN ",
            GrammarSlot::Column,
            &[ObjectKind::Table][..],
            &["app", "accounts"][..],
        ),
        (
            "ALTER TABLE app.accounts DROP CONSTRAINT ",
            GrammarSlot::Constraint,
            &[ObjectKind::Table][..],
            &["app", "accounts"][..],
        ),
        (
            "ALTER DOMAIN app.email DROP CONSTRAINT ",
            GrammarSlot::Constraint,
            &[ObjectKind::Domain][..],
            &["app", "email"][..],
        ),
        (
            "ALTER TYPE app.address DROP ATTRIBUTE ",
            GrammarSlot::Attribute,
            &[ObjectKind::Type][..],
            &["app", "address"][..],
        ),
    ];

    for (source, slot, kinds, names) in cases {
        let context = collect(source, size(source.len()));
        assert!(context.expectations.slots.contains(&slot), "{source:?}");
        let container = context
            .intent
            .container
            .as_ref()
            .unwrap_or_else(|| panic!("missing Catalog container for {source:?}"));
        let member = match slot {
            GrammarSlot::Column => ObjectKind::Column,
            GrammarSlot::Attribute => ObjectKind::Attribute,
            GrammarSlot::Constraint => ObjectKind::Constraint,
            _ => unreachable!(),
        };
        assert_eq!(container.members, [member], "{source:?}");
        assert_eq!(container.reference.object_kinds, kinds, "{source:?}");
        assert_eq!(
            container
                .reference
                .name
                .iter()
                .map(|part| part.normalized.as_str())
                .collect::<Vec<_>>(),
            names,
            "{source:?}"
        );
    }
}

#[test]
fn finds_catalog_containers_on_either_side_of_the_completion_point() {
    let source = "COPY app.accounts () FROM STDIN";
    let point = source.find(')').unwrap();
    let context = collect(source, size(point));
    assert_eq!(
        context.intent.container.unwrap().reference.name[1].normalized,
        "accounts"
    );

    let source = "CREATE TABLE child (parent_id int REFERENCES app.parent ())";
    let point = source.rfind(')').unwrap() - 1;
    let context = collect(source, size(point));
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
    assert_eq!(
        context.intent.container.unwrap().reference.name[1].normalized,
        "parent"
    );

    let source = "CREATE TRIGGER tr BEFORE UPDATE OF  ON app.accounts EXECUTE FUNCTION f()";
    let point = source.find("  ON").unwrap() + 1;
    let context = collect(source, size(point));
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
    assert_eq!(
        context.intent.container.unwrap().reference.name[1].normalized,
        "accounts"
    );

    let source = "CREATE TRIGGER tr BEFORE UPDATE ON app.accounts WHEN () EXECUTE FUNCTION f()";
    let point = source.find("()").unwrap() + 1;
    let context = collect(source, size(point));
    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    let container = context.intent.container.unwrap();
    assert_eq!(
        container.reference.object_kinds,
        [
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::ForeignTable
        ]
    );
    assert_eq!(container.reference.name[1].normalized, "accounts");

    for (source, point) in [
        (
            "GRANT SELECT () ON TABLE app.accounts TO role_name",
            "GRANT SELECT (".len(),
        ),
        ("VACUUM app.accounts ()", "VACUUM app.accounts (".len()),
        (
            "CREATE INDEX i ON app.accounts (lower())",
            "CREATE INDEX i ON app.accounts (lower(".len(),
        ),
        (
            "CREATE STATISTICS s ON (lower()) FROM app.accounts",
            "CREATE STATISTICS s ON (lower(".len(),
        ),
    ] {
        let context = collect(source, size(point));
        let container = context
            .intent
            .container
            .unwrap_or_else(|| panic!("missing Catalog container for {source:?}"));
        assert_eq!(container.members, [ObjectKind::Column], "{source:?}");
        assert_eq!(
            container.reference.name[1].normalized, "accounts",
            "{source:?}"
        );
    }

    let source = "CREATE TABLE child (parent_id int REFERENCES parent(id), value int CHECK ())";
    let point = source.rfind("()").unwrap() + 1;
    let context = collect(source, size(point));
    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    assert!(context.intent.container.is_none());
}

#[test]
fn generic_names_do_not_masquerade_as_catalog_objects() {
    for source in ["DECLARE ", "PREPARE ", "SAVEPOINT ", "LISTEN "] {
        let context = collect(source, size(source.len()));
        assert_eq!(context.expectations.slots, [GrammarSlot::AnyName]);
        assert!(context.intent.object_kinds.is_empty(), "{source:?}");
    }
}

#[test]
fn catalog_name_slots_produce_exact_object_intent() {
    for (source, slot, kind) in [
        (
            "ALTER EXTENSION ",
            GrammarSlot::Extension,
            ObjectKind::Extension,
        ),
        (
            "ALTER SERVER ",
            GrammarSlot::ForeignServer,
            ObjectKind::ForeignServer,
        ),
        (
            "ALTER PUBLICATION ",
            GrammarSlot::Publication,
            ObjectKind::Publication,
        ),
        (
            "ALTER STATISTICS ",
            GrammarSlot::Statistics,
            ObjectKind::Statistics,
        ),
        (
            "ALTER TEXT SEARCH DICTIONARY ",
            GrammarSlot::TextSearchDictionary,
            ObjectKind::TextSearchDictionary,
        ),
        (
            "DROP PROCEDURE ",
            GrammarSlot::Procedure,
            ObjectKind::Procedure,
        ),
        (
            "DROP AGGREGATE ",
            GrammarSlot::Aggregate,
            ObjectKind::Aggregate,
        ),
        ("ALTER DOMAIN ", GrammarSlot::Domain, ObjectKind::Domain),
        (
            "DROP OPERATOR FAMILY ",
            GrammarSlot::OperatorFamily,
            ObjectKind::OperatorFamily,
        ),
        ("CALL ", GrammarSlot::Procedure, ObjectKind::Procedure),
    ] {
        let context = collect(source, size(source.len()));
        assert!(context.expectations.slots.contains(&slot), "{source:?}");
        assert_eq!(context.intent.object_kinds, [kind], "{source:?}");
    }
}

#[test]
fn relation_intent_comes_from_the_active_production() {
    let ddl = collect("ALTER TABLE ", size("ALTER TABLE ".len()));
    assert_eq!(ddl.expectations.slots, [GrammarSlot::Table]);
    assert_eq!(ddl.intent.object_kinds, [ObjectKind::Table]);

    for source in ["INSERT INTO ", "UPDATE ", "DELETE FROM ", "MERGE INTO "] {
        let dml = collect(source, size(source.len()));
        assert_eq!(dml.expectations.slots, [GrammarSlot::Table], "{source:?}");
        assert_eq!(dml.intent.object_kinds, [ObjectKind::Table], "{source:?}");
    }

    let source = "CREATE TABLE target AS SELECT * FROM ";
    let query = collect(source, size(source.len()));
    assert_eq!(
        query.expectations.slots,
        [GrammarSlot::Relation, GrammarSlot::Function]
    );
    for kind in [
        ObjectKind::Table,
        ObjectKind::View,
        ObjectKind::MaterializedView,
        ObjectKind::ForeignTable,
        ObjectKind::Sequence,
        ObjectKind::Schema,
    ] {
        assert!(query.intent.object_kinds.contains(&kind), "{kind:?}");
    }
}

#[test]
fn replaces_the_whole_identifier_when_the_point_is_inside_it() {
    let source = "SELECT";
    let context = collect(source, size(3));
    assert_eq!(context.prefix.raw, "SEL");
    assert_eq!(context.replacement_range, TextRange::new(size(0), size(6)));
    assert_eq!(context.expectations.tokens, [TokenKind::Select]);

    let source = "SELECT \"Mixed\" FROM t";
    let point = source.find("Mix").unwrap() + 3;
    let context = collect(source, size(point));
    assert_eq!(context.prefix.raw, "Mix");
    assert_eq!(
        context.replacement_range,
        TextRange::new(
            size(source.find('"').unwrap()),
            size(source.find(" FROM").unwrap())
        )
    );
}

#[test]
fn handles_unicode_quoted_and_utf8_identifier_prefixes() {
    let source = "SELECT U&\"Schema\".U&\"Mixed\" FROM t";
    let point = source.find("Mix").unwrap() + 3;
    let context = collect(source, size(point));
    assert_eq!(context.prefix.raw, "Mix");
    assert_eq!(context.prefix.normalized, "Mix");
    assert_eq!(context.prefix.quoting, IdentifierQuoting::UnicodeQuoted);
    assert!(context.expectations.tokens.is_empty());
    assert_eq!(context.intent.qualifier[0].normalized, "Schema");
    assert_eq!(
        context.replacement_range,
        TextRange::new(
            size(source.find("U&\"Mixed\"").unwrap()),
            size(source.find(" FROM").unwrap())
        )
    );

    let complete = "SELECT \"Mixed\"";
    let context = collect(complete, size(complete.len()));
    assert_eq!(context.prefix.raw, "Mixed");
    assert_eq!(context.prefix.quoting, IdentifierQuoting::Quoted);
    assert_eq!(
        context.replacement_range,
        TextRange::new(size(7), size(complete.len()))
    );

    let escaped = "SELECT \"A\"\"B\"";
    let context = collect(escaped, size(escaped.len()));
    assert_eq!(context.prefix.raw, "A\"\"B");
    assert_eq!(context.prefix.normalized, "A\"B");

    let unicode_escape = r#"SELECT U&"u\0061". FROM users AS U&"u\0061""#;
    let point = unicode_escape.find(" FROM").unwrap();
    let context = collect(unicode_escape, size(point));
    assert_eq!(context.intent.qualifier[0].normalized, "ua");
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "ua"
    );

    let utf8 = "SELECT 名称后缀";
    let point = utf8.find('名').unwrap() + '名'.len_utf8();
    let context = collect(utf8, size(point));
    assert_eq!(context.prefix.raw, "名");
    assert_eq!(
        context.replacement_range,
        TextRange::new(size(7), size(utf8.len()))
    );
}

#[test]
fn numeric_literals_are_not_identifier_prefixes() {
    let source = "SELECT 1";
    let context = collect(source, size(source.len()));
    assert!(context.prefix.raw.is_empty());
    assert_eq!(
        context.replacement_range,
        TextRange::empty(size(source.len()))
    );
    assert!(
        context.expectations.tokens.contains(&TokenKind::From),
        "{:?}",
        context.expectations
    );
}

#[test]
fn captures_dml_target_and_source_scope() {
    let source = "UPDATE accounts a SET name = u.name FROM users u WHERE ";
    let context = collect(source, size(source.len()));

    assert_eq!(
        context
            .scope
            .dml_target
            .as_ref()
            .unwrap()
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "a"
    );
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "u"
    );
}

#[test]
fn applies_dml_visibility_inside_sources_and_correlated_subqueries() {
    for source in [
        "INSERT INTO target_table SELECT target_table.",
        "INSERT INTO target_table VALUES (target_table.)",
        "MERGE INTO target_table target USING (SELECT target.) source ON true WHEN MATCHED THEN DO NOTHING",
    ] {
        let context = collect(source, size(source.find('.').unwrap() + 1));
        assert!(context.scope.dml_target.is_none(), "{source:?}");
        assert!(context.scope.merge_source.is_none(), "{source:?}");
    }

    let source = "UPDATE target_table target SET value = (SELECT target.) FROM source_table source";
    let context = collect(source, size(source.find("target.)").unwrap() + 7));
    assert_eq!(
        context
            .scope
            .dml_target
            .as_ref()
            .and_then(|relation| relation.alias.as_ref())
            .unwrap()
            .normalized,
        "target"
    );
    assert!(context.scope.local.relations.is_empty());
    assert_eq!(
        context.scope.outer[0].relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "source"
    );

    let source = "UPDATE target_table target SET value = 1 FROM source_table source, (SELECT target.) derived";
    let context = collect(source, size(source.find("target.)").unwrap() + 7));
    assert!(context.scope.dml_target.is_none());
    assert!(context.scope.outer.is_empty());

    let source = "UPDATE target_table target SET value = 1 FROM source_table source, LATERAL (SELECT source.) derived";
    let context = collect(source, size(source.find("source.)").unwrap() + 7));
    assert!(context.scope.dml_target.is_none());
    assert_eq!(
        context.scope.outer[0].relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "source"
    );

    let source = "UPDATE target_table target SET value = 1 FROM source_table source, lookup(source.) derived";
    let context = collect(source, size(source.find("source.)").unwrap() + 7));
    assert!(context.scope.dml_target.is_none());
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "source"
    );
    assert!(context.scope.outer.is_empty());
}

#[test]
fn cte_scope_tracks_recursion_nesting_and_shadowing() {
    let recursive = "WITH RECURSIVE first AS (SELECT  FROM later), later AS (SELECT 1) SELECT 1";
    let point = recursive.find(" FROM later").unwrap();
    let context = collect(recursive, size(point));
    assert_eq!(
        context
            .scope
            .ctes
            .iter()
            .map(|cte| cte.name.normalized.as_str())
            .collect::<Vec<_>>(),
        ["first", "later"]
    );

    let nested = "WITH shared AS (SELECT 1) SELECT * FROM (WITH shared AS (SELECT 2), inner_cte AS (SELECT 3) SELECT  FROM shared) q";
    let point = nested.rfind(" FROM shared").unwrap();
    let context = collect(nested, size(point));
    assert_eq!(
        context
            .scope
            .ctes
            .iter()
            .map(|cte| cte.name.normalized.as_str())
            .collect::<Vec<_>>(),
        ["shared", "inner_cte"]
    );
    assert_eq!(
        usize::from(context.scope.ctes[0].syntax_range.start()),
        nested.rfind("shared AS").unwrap()
    );

    let incomplete = "WITH RECURSIVE self_ref AS (SELECT self_ref.";
    let context = collect(incomplete, size(incomplete.len()));
    assert_eq!(context.scope.ctes[0].name.normalized, "self_ref");
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::ScopeIncomplete)
    );
}

#[test]
fn incomplete_derived_tables_keep_lateral_visibility_rules() {
    let non_lateral = "SELECT * FROM accounts a, (SELECT a.";
    let context = collect(non_lateral, size(non_lateral.len()));
    assert!(context.scope.outer.is_empty());
    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::ScopeIncomplete)
    );

    let lateral = "SELECT * FROM accounts a, LATERAL (SELECT a.";
    let context = collect(lateral, size(lateral.len()));
    assert_eq!(context.scope.outer.len(), 1);
    assert_eq!(
        context.scope.outer[0].relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "a"
    );
}

#[test]
fn subqueries_in_table_function_arguments_inherit_implicit_lateral_visibility() {
    for source in [
        "SELECT * FROM accounts a, lookup((SELECT a.)) f",
        "SELECT * FROM accounts a, ROWS FROM (lookup((SELECT a.))) f",
    ] {
        let context = collect(source, size(source.find("a.)").unwrap() + 2));
        assert_eq!(context.scope.outer.len(), 1, "{source:?}");
        assert_eq!(
            context.scope.outer[0].relations[0]
                .alias
                .as_ref()
                .unwrap()
                .normalized,
            "a",
            "{source:?}"
        );
    }
}

#[test]
fn correlated_subqueries_in_join_conditions_see_the_join_inputs() {
    let source = "SELECT * FROM accounts a JOIN users u ON u.account_id = (SELECT a.) IS NOT NULL, future_table future";
    let context = collect(source, size(source.find("a.)").unwrap() + 2));
    assert_eq!(context.scope.outer.len(), 1);
    assert_eq!(
        context.scope.outer[0]
            .relations
            .iter()
            .map(|relation| relation.alias.as_ref().unwrap().normalized.as_str())
            .collect::<Vec<_>>(),
        ["a", "u"]
    );

    let source = "SELECT * FROM accounts a JOIN users u ON  LEFT JOIN future_table future ON true";
    let point = source.find("ON  LEFT").unwrap() + 3;
    let context = collect(source, size(point));
    assert_eq!(
        context
            .scope
            .local
            .relations
            .iter()
            .map(|relation| relation.alias.as_ref().unwrap().normalized.as_str())
            .collect::<Vec<_>>(),
        ["a", "u"]
    );
}

#[test]
fn rows_from_arguments_see_only_preceding_from_items() {
    let source = "SELECT * FROM accounts a, ROWS FROM (f(a.id), g(a.id)) r, later_relation later";
    let point = source.rfind("a.id").unwrap() + 2;
    let context = collect(source, size(point));
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "a"
    );
}

#[test]
fn merge_actions_keep_target_and_source_visible() {
    let source = "MERGE INTO target_table AS target USING source_table AS source ON target.id = source.id WHEN MATCHED THEN UPDATE SET value = source.";
    let context = collect(source, size(source.len()));
    assert_eq!(
        context
            .scope
            .dml_target
            .as_ref()
            .and_then(|relation| relation.alias.as_ref())
            .unwrap()
            .normalized,
        "target"
    );
    assert_eq!(
        context
            .scope
            .merge_source
            .as_ref()
            .and_then(|relation| relation.alias.as_ref())
            .unwrap()
            .normalized,
        "source"
    );
    assert_eq!(context.intent.qualifier[0].normalized, "source");
    assert_eq!(context.expectations.slots, [GrammarSlot::Column]);
}

#[test]
fn merge_not_matched_actions_expose_only_existing_rows() {
    let prefix =
        "MERGE INTO target_table AS target USING source_table AS source ON target.id = source.id ";
    let target_only = format!("{prefix}WHEN NOT MATCHED BY SOURCE AND ");
    let context = collect(&target_only, size(target_only.len()));
    assert!(context.scope.dml_target.is_some());
    assert!(context.scope.merge_source.is_none());

    let source_only = format!("{prefix}WHEN NOT MATCHED BY TARGET AND ");
    let context = collect(&source_only, size(source_only.len()));
    assert!(context.scope.dml_target.is_none());
    assert!(context.scope.merge_source.is_some());

    let source_only = format!("{prefix}WHEN NOT MATCHED THEN INSERT VALUES (source.)");
    let point = source_only.find("source.)").unwrap() + "source.".len();
    let context = collect(&source_only, size(point));
    assert!(context.scope.dml_target.is_none());
    assert!(context.scope.merge_source.is_some());
}

#[test]
fn parenthesized_joins_only_hide_relations_when_aliased() {
    let unaliased = "SELECT a. FROM (accounts a JOIN users u ON a.id = u.id)";
    let context = collect(unaliased, size(unaliased.find(" FROM").unwrap()));
    assert_eq!(
        context
            .scope
            .local
            .relations
            .iter()
            .map(|relation| relation.alias.as_ref().unwrap().normalized.as_str())
            .collect::<Vec<_>>(),
        ["a", "u"]
    );

    let aliased = "SELECT joined. FROM (accounts a JOIN users u ON a.id = u.id) AS joined";
    let context = collect(aliased, size(aliased.find(" FROM").unwrap()));
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0].kind,
        pg_completion::RelationKind::JoinAlias
    );
    assert_eq!(
        context.scope.local.relations[0]
            .alias
            .as_ref()
            .unwrap()
            .normalized,
        "joined"
    );
}

#[test]
fn classifies_table_functions_and_insert_aliases_through_collect() {
    let source = "SELECT r. FROM ROWS FROM (f(), g()) AS r(a)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0].kind,
        pg_completion::RelationKind::TableFunction
    );
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["a"]
    );

    let source = "SELECT r. FROM ROWS FROM (f() AS (first bigint), g() AS (second text)) AS r";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );

    let source = "SELECT r. FROM ROWS FROM (f() AS (inner_name bigint)) AS r(outer_name)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["outer_name"]
    );

    let source = "INSERT INTO target OVERRIDING SYSTEM VALUE VALUES (1)";
    let context = collect(source, size(source.find(" OVERRIDING").unwrap()));
    assert!(context.scope.dml_target.as_ref().unwrap().alias.is_none());

    let source = "INSERT INTO target AS inserted(first_column, second_column) VALUES (1, 2) RETURNING inserted.";
    let context = collect(source, size(source.len()));
    let target = context.scope.dml_target.as_ref().unwrap();
    assert_eq!(target.alias.as_ref().unwrap().normalized, "inserted");
    assert_eq!(
        target
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["first_column", "second_column"]
    );
}

#[test]
fn table_function_suffixes_do_not_masquerade_as_aliases() {
    for source in [
        "SELECT f. FROM generate_series(1, 2) WITH ORDINALITY AS f",
        "SELECT f. FROM ROWS FROM (generate_series(1, 2)) WITH ORDINALITY AS f",
    ] {
        let context = collect(source, size(source.find(" FROM").unwrap()));
        assert_eq!(context.scope.local.relations.len(), 1, "{source:?}");
        assert_eq!(
            context.scope.local.relations[0]
                .alias
                .as_ref()
                .unwrap()
                .normalized,
            "f",
            "{source:?}"
        );
    }

    let source = "SELECT sampled. FROM sampled TABLESAMPLE bernoulli(10)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert!(context.scope.local.relations[0].alias.is_none());
    assert_eq!(
        context.scope.local.relations[0].name[0].normalized,
        "sampled"
    );

    let source = "SELECT f. FROM record_source() AS f(value bigint, label text)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["value", "label"]
    );

    let source = "SELECT record_source. FROM record_source() AS (value bigint, label text)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert!(context.scope.local.relations[0].alias.is_none());
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["value", "label"]
    );
}

#[test]
fn exposes_explicit_subquery_alias_columns() {
    let source = "SELECT q. FROM (SELECT 1) AS q(first_column)";
    let context = collect(source, size(source.find(" FROM").unwrap()));
    assert_eq!(context.scope.local.relations.len(), 1);
    assert_eq!(
        context.scope.local.relations[0]
            .explicit_columns
            .iter()
            .map(|column| column.normalized.as_str())
            .collect::<Vec<_>>(),
        ["first_column"]
    );
}

#[test]
fn suppresses_candidates_inside_non_identifier_lexical_containers() {
    for source in [
        "SELECT 'unfinished",
        "SELECT /* unfinished",
        "SELECT $tag$unfinished",
        "SELECT 1 -- unfinished",
        "SELECT E'escaped\\'quote",
    ] {
        let context = collect(source, size(source.len()));
        assert!(
            context.expectations.tokens.is_empty(),
            "unexpected token candidates for {source:?}"
        );
        assert!(
            context.expectations.slots.is_empty(),
            "unexpected slot candidates for {source:?}"
        );
    }
}

#[test]
fn recovers_an_unterminated_identifier_as_the_active_prefix() {
    let source = "SELECT schema.\"Mi";
    let context = collect(source, size(source.len()));

    assert_eq!(context.prefix.raw, "Mi");
    assert_eq!(context.prefix.normalized, "Mi");
    assert_eq!(context.prefix.quoting, IdentifierQuoting::Quoted);
    assert_eq!(context.intent.qualifier[0].normalized, "schema");
    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::UnterminatedToken)
    );
}

#[test]
fn distinguishes_an_active_unterminated_token_from_an_earlier_lex_error() {
    let active = "SELECT 'unfinished";
    let context = collect(active, size(active.len()));
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::UnterminatedToken)
    );

    let before = "SELECT 1e+ FROM users";
    let context = collect(before, size(before.len()));
    assert!(context.expectations.tokens.is_empty());
    assert!(context.expectations.slots.is_empty());
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::LexErrorBeforePoint)
    );
}

#[test]
fn scope_recovery_does_not_discard_parser_expectations() {
    let source = "SELECT  FROM \"unfinished";
    let context = collect(source, size(7));

    assert!(context.expectations.slots.contains(&GrammarSlot::Column));
    assert!(context.expectations.tokens.contains(&TokenKind::From));
    assert!(context.scope.local.relations.is_empty());
    assert!(
        context
            .recovery
            .issues
            .iter()
            .any(|issue| issue.kind == RecoveryKind::ScopeIncomplete)
    );
}
