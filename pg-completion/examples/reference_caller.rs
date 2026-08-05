//! A complete caller-side adapter for `pg-completion`.
//!
//! Run with:
//! `cargo run -p pg-completion --example reference_caller`

use std::collections::HashSet;

use pg_completion::CompletionContext;
use pg_completion::CompletionPrefix;
use pg_completion::GrammarSlot;
use pg_completion::NamePart;
use pg_completion::ObjectKind;
use pg_completion::ObjectReference;
use pg_completion::RelationKind;
use pg_completion::SyntaxCompletionKind;
use pg_completion::VisibleRelation;
use pg_completion::collect;
use pg_parser::KEYWORDS;
use pg_parser::KeywordCategory;
use pg_parser::TextRange;
use pg_parser::TextSize;
#[cfg(test)]
use pg_parser::TokenKind;

#[derive(Clone, Debug, Eq, PartialEq)]
enum CompletionItemKind {
    Continuation,
    Syntax,
    Phrase,
    Privilege,
    Object(ObjectKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionItem {
    label: String,
    insert_text: String,
    replacement_range: TextRange,
    kind: CompletionItemKind,
    detail: String,
}

#[derive(Clone, Debug)]
struct CatalogObject {
    kind: ObjectKind,
    schema: Option<String>,
    name: String,
}

/// The only seam a real caller needs to implement for its metadata source.
trait Catalog {
    fn objects(&self, kinds: &[ObjectKind], qualifier: &[NamePart]) -> Vec<CatalogObject>;

    fn members(&self, owner: &ObjectReference, kinds: &[ObjectKind]) -> Vec<CatalogObject>;

    fn columns(&self, relation: &VisibleRelation) -> Vec<CatalogObject>;
}

fn complete<C: Catalog>(
    source: &str,
    cursor: usize,
    catalog: &C,
) -> (CompletionContext, Vec<CompletionItem>) {
    let context = collect(
        source,
        TextSize::try_from(cursor).expect("document fits TextSize"),
    );
    let mut items = Vec::new();

    add_syntax_items(&context, &mut items);
    add_privilege_items(&context, &mut items);
    add_scope_items(&context, catalog, &mut items);
    add_catalog_items(&context, catalog, &mut items);
    add_cte_items(&context, &mut items);

    let mut seen = HashSet::new();
    items.retain(|item| seen.insert((format!("{:?}", item.kind), item.insert_text.clone())));
    items.sort_by(|left, right| {
        item_rank(&left.kind)
            .cmp(&item_rank(&right.kind))
            .then_with(|| left.label.cmp(&right.label))
    });
    (context, items)
}

fn add_syntax_items(context: &CompletionContext, items: &mut Vec<CompletionItem>) {
    for syntax in context.syntax_completions() {
        let kind = match syntax.kind {
            SyntaxCompletionKind::Phrase => CompletionItemKind::Phrase,
            SyntaxCompletionKind::Keyword if syntax.is_follow => CompletionItemKind::Syntax,
            SyntaxCompletionKind::Keyword => CompletionItemKind::Continuation,
        };
        items.push(CompletionItem {
            label: syntax.label,
            insert_text: syntax.insert_text,
            replacement_range: context.replacement_range,
            kind,
            detail: "SQL phrase".to_owned(),
        });
    }
}

fn add_privilege_items(context: &CompletionContext, items: &mut Vec<CompletionItem>) {
    if !context.expectations.slots.contains(&GrammarSlot::Privilege) {
        return;
    }
    for privilege in ["CONNECT", "MAINTAIN", "USAGE"] {
        if prefix_matches(&context.prefix, privilege) {
            items.push(CompletionItem {
                label: privilege.to_owned(),
                insert_text: privilege.to_owned(),
                replacement_range: context.replacement_range,
                kind: CompletionItemKind::Privilege,
                detail: "PostgreSQL privilege".to_owned(),
            });
        }
    }
}

fn add_scope_items<C: Catalog>(
    context: &CompletionContext,
    catalog: &C,
    items: &mut Vec<CompletionItem>,
) {
    if !context.intent.object_kinds.contains(&ObjectKind::Column)
        || context
            .intent
            .membership
            .as_ref()
            .is_some_and(|membership| membership.member_kinds.contains(&ObjectKind::Column))
    {
        return;
    }

    let mut relations = Vec::new();
    relations.extend(context.scope.local.relations.iter());
    if let Some(target) = &context.scope.dml_target {
        relations.push(target);
    }
    if let Some(source) = &context.scope.merge_source {
        relations.push(source);
    }
    for outer in &context.scope.outer {
        relations.extend(outer.relations.iter());
    }

    for relation in relations {
        if !relation_is_visible_for_qualifier(relation, &context.intent.qualifier) {
            continue;
        }
        let columns = if relation.explicit_columns.is_empty() {
            if relation.kind != RelationKind::Relation {
                Vec::new()
            } else {
                catalog.columns(relation)
            }
        } else {
            relation
                .explicit_columns
                .iter()
                .map(|column| CatalogObject {
                    kind: ObjectKind::Column,
                    schema: None,
                    name: column.text.clone(),
                })
                .collect()
        };
        for column in columns {
            push_object(context, column, "visible column", items);
        }
    }
}

fn add_catalog_items<C: Catalog>(
    context: &CompletionContext,
    catalog: &C,
    items: &mut Vec<CompletionItem>,
) {
    let mut global_kinds = context.intent.object_kinds.clone();
    global_kinds.retain(|kind| *kind != ObjectKind::Column);

    if let Some(membership) = &context.intent.membership {
        let member_kinds = global_kinds
            .iter()
            .copied()
            .filter(|kind| membership.member_kinds.contains(kind))
            .collect::<Vec<_>>();
        for candidate in catalog.members(&membership.owner, &member_kinds) {
            push_object(context, candidate, "catalog member", items);
        }
        if membership.member_kinds.contains(&ObjectKind::Column) {
            for candidate in catalog.members(&membership.owner, &[ObjectKind::Column]) {
                push_object(context, candidate, "catalog column", items);
            }
        }
        global_kinds.retain(|kind| !membership.member_kinds.contains(kind));
    }

    for candidate in catalog.objects(&global_kinds, &context.intent.qualifier) {
        push_object(context, candidate, "catalog object", items);
    }
}

fn add_cte_items(context: &CompletionContext, items: &mut Vec<CompletionItem>) {
    if !context.expectations.slots.contains(&GrammarSlot::Relation) {
        return;
    }
    for cte in &context.scope.ctes {
        push_object(
            context,
            CatalogObject {
                kind: ObjectKind::Table,
                schema: None,
                name: cte.name.text.clone(),
            },
            "visible CTE",
            items,
        );
    }
}

fn push_object(
    context: &CompletionContext,
    object: CatalogObject,
    detail: &str,
    items: &mut Vec<CompletionItem>,
) {
    if !prefix_matches(&context.prefix, &object.name) {
        return;
    }
    let insert_text = if object.kind == ObjectKind::Operator {
        object.name.clone()
    } else {
        quote_identifier(&object.name)
    };
    let label = object.schema.as_ref().map_or_else(
        || object.name.clone(),
        |schema| format!("{schema}.{}", object.name),
    );
    items.push(CompletionItem {
        label,
        insert_text,
        replacement_range: context.replacement_range,
        kind: CompletionItemKind::Object(object.kind),
        detail: detail.to_owned(),
    });
}

fn relation_is_visible_for_qualifier(relation: &VisibleRelation, qualifier: &[NamePart]) -> bool {
    if qualifier.is_empty() {
        return !relation.qualified_only;
    }
    if let Some(alias) = &relation.alias {
        return qualifier.len() == 1 && alias.normalized == qualifier[0].normalized;
    }
    let matches_full_name = relation.name.len() == qualifier.len()
        && relation
            .name
            .iter()
            .zip(qualifier)
            .all(|(left, right)| left.normalized == right.normalized);
    let matches_relation_name = qualifier.len() == 1
        && relation
            .name
            .last()
            .is_some_and(|name| name.normalized == qualifier[0].normalized);
    matches_full_name || matches_relation_name
}

fn prefix_matches(prefix: &CompletionPrefix, candidate: &str) -> bool {
    let candidate = if prefix.quoting == pg_completion::IdentifierQuoting::Unquoted {
        candidate.to_ascii_lowercase()
    } else {
        candidate.to_owned()
    };
    candidate.starts_with(&prefix.normalized)
}

fn quote_identifier(identifier: &str) -> String {
    let simple = identifier
        .bytes()
        .enumerate()
        .all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' | b'$' => index != 0,
            _ => false,
        });
    let reserved = KEYWORDS.iter().any(|keyword| {
        keyword.word.eq_ignore_ascii_case(identifier)
            && keyword.category == KeywordCategory::Reserved
    });
    if simple && !reserved {
        identifier.to_owned()
    } else {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }
}

fn item_rank(kind: &CompletionItemKind) -> u8 {
    match kind {
        CompletionItemKind::Object(ObjectKind::Column) => 0,
        CompletionItemKind::Object(_) | CompletionItemKind::Privilege => 1,
        CompletionItemKind::Continuation => 2,
        CompletionItemKind::Phrase => 3,
        CompletionItemKind::Syntax => 4,
    }
}

#[derive(Default)]
struct MemoryCatalog {
    objects: Vec<CatalogObject>,
    relations: Vec<(Vec<String>, Vec<String>)>,
}

impl Catalog for MemoryCatalog {
    fn objects(&self, kinds: &[ObjectKind], qualifier: &[NamePart]) -> Vec<CatalogObject> {
        self.objects
            .iter()
            .filter(|object| kinds.contains(&object.kind))
            .filter(|object| {
                qualifier.is_empty()
                    || (qualifier.len() == 1
                        && object.schema.as_deref() == Some(qualifier[0].normalized.as_str()))
            })
            .cloned()
            .collect()
    }

    fn members(&self, owner: &ObjectReference, kinds: &[ObjectKind]) -> Vec<CatalogObject> {
        if !kinds.contains(&ObjectKind::Column) {
            return Vec::new();
        }
        let name = normalized_name(&owner.name);
        self.relations
            .iter()
            .find(|(relation, _)| relation == &name)
            .map(|(_, columns)| {
                columns
                    .iter()
                    .map(|column| CatalogObject {
                        kind: ObjectKind::Column,
                        schema: None,
                        name: column.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn columns(&self, relation: &VisibleRelation) -> Vec<CatalogObject> {
        let name = normalized_name(&relation.name);
        self.relations
            .iter()
            .find(|(relation, _)| relation == &name)
            .map(|(_, columns)| {
                columns
                    .iter()
                    .map(|column| CatalogObject {
                        kind: ObjectKind::Column,
                        schema: None,
                        name: column.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn normalized_name(name: &[NamePart]) -> Vec<String> {
    name.iter().map(|part| part.normalized.clone()).collect()
}

fn catalog() -> MemoryCatalog {
    MemoryCatalog {
        objects: vec![
            CatalogObject {
                kind: ObjectKind::Function,
                schema: Some("u".to_owned()),
                name: "refresh".to_owned(),
            },
            CatalogObject {
                kind: ObjectKind::Operator,
                schema: Some("pg_catalog".to_owned()),
                name: "=".to_owned(),
            },
        ],
        relations: vec![(
            vec!["public".to_owned(), "users".to_owned()],
            vec!["id".to_owned(), "name".to_owned()],
        )],
    }
}

fn main() {
    let source = "SELECT u. FROM public.users AS u";
    let cursor = source.find(" FROM").unwrap();
    let (_, items) = complete(source, cursor, &catalog());

    for item in &items {
        println!("{:?}\t{}\t{}", item.kind, item.label, item.detail);
    }

    assert!(items.iter().any(|item| {
        item.label == "name" && item.kind == CompletionItemKind::Object(ObjectKind::Column)
    }));
    assert!(items.iter().any(|item| {
        item.label == "u.refresh" && item.kind == CompletionItemKind::Object(ObjectKind::Function)
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_labels(source: &str, cursor: usize) -> Vec<(CompletionItemKind, String)> {
        complete(source, cursor, &catalog())
            .1
            .into_iter()
            .filter_map(|item| {
                if matches!(item.kind, CompletionItemKind::Object(_)) {
                    Some((item.kind, item.label))
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn relation_alias_and_schema_are_resolved_independently() {
        let source = "SELECT u. FROM public.users AS u";
        let labels = object_labels(source, source.find(" FROM").unwrap());
        assert!(labels.contains(&(
            CompletionItemKind::Object(ObjectKind::Column),
            "name".into()
        )));
        assert!(labels.contains(&(
            CompletionItemKind::Object(ObjectKind::Function),
            "u.refresh".into()
        )));
    }

    #[test]
    fn unaliased_qualified_relation_accepts_its_relation_name() {
        for source in [
            "SELECT users. FROM public.users",
            "SELECT public.users. FROM public.users",
        ] {
            let labels = object_labels(source, source.find(" FROM").unwrap());
            assert!(
                labels.contains(&(
                    CompletionItemKind::Object(ObjectKind::Column),
                    "name".into()
                )),
                "{source:?}: {labels:?}"
            );
        }

        let source = "SELECT users. FROM public.users AS u";
        let labels = object_labels(source, source.find(" FROM").unwrap());
        assert!(
            !labels
                .iter()
                .any(|(kind, _)| *kind == CompletionItemKind::Object(ObjectKind::Column)),
            "an alias must hide the original relation name: {labels:?}"
        );
    }

    #[test]
    fn member_columns_are_queried_from_the_named_relation() {
        let source = "CREATE INDEX i ON public.users ()";
        let labels = object_labels(source, source.find("()").unwrap() + 1);
        assert!(labels.contains(&(CompletionItemKind::Object(ObjectKind::Column), "id".into())));
    }

    #[test]
    fn explicit_cte_columns_do_not_require_catalog_lookup() {
        let source = "WITH c(user_id) AS (SELECT 1) SELECT c. FROM c";
        let labels = object_labels(source, source.rfind(" FROM").unwrap());
        assert!(labels.contains(&(
            CompletionItemKind::Object(ObjectKind::Column),
            "user_id".into()
        )));
    }

    #[test]
    fn qualified_only_dml_relations_resolve_target_columns() {
        for source in [
            "INSERT INTO public.users VALUES (1) ON CONFLICT (id) DO UPDATE SET name = excluded.",
            "UPDATE public.users SET name = 'x' RETURNING old.",
        ] {
            let labels = object_labels(source, source.len());
            assert!(
                labels.contains(&(
                    CompletionItemKind::Object(ObjectKind::Column),
                    "name".into()
                )),
                "{source:?}: {labels:?}"
            );
        }
    }

    #[test]
    fn raw_punctuation_expectations_are_not_editor_items() {
        let source = "SELECT ";
        let (context, items) = complete(source, source.len(), &catalog());

        for punctuation in ['(', '*', '+', '-'] {
            assert!(
                context
                    .expectations
                    .tokens
                    .contains(&TokenKind::Char(punctuation)),
                "raw context lost {punctuation:?}"
            );
            assert!(
                !items
                    .iter()
                    .any(|item| item.label == punctuation.to_string()),
                "editor items contain {punctuation:?}: {items:?}"
            );
        }
        assert!(items.iter().any(|item| item.label == "ARRAY"));
    }

    #[test]
    fn unprefixed_expression_tail_is_silent() {
        let source = "SELECT * FROM public.users JOIN public.users ON users.id ";
        let (context, items) = complete(source, source.len(), &catalog());

        assert!(
            context
                .expectations
                .follow_tokens
                .contains(&TokenKind::Join)
        );
        assert!(!context.intent.object_kinds.contains(&ObjectKind::Operator));
        assert!(context.syntax_completions().is_empty());
        assert!(items.is_empty(), "{items:?}");
    }

    #[test]
    fn operator_catalog_is_queried_only_by_explicit_operator_name_grammar() {
        let source = "SELECT 1 ORDER BY 1 USING ";
        let (context, items) = complete(source, source.len(), &catalog());

        assert!(context.expectations.slots.contains(&GrammarSlot::Operator));
        assert!(context.intent.object_kinds.contains(&ObjectKind::Operator));
        assert!(items.iter().any(|item| {
            item.kind == CompletionItemKind::Object(ObjectKind::Operator)
                && item.label == "pg_catalog.="
                && item.insert_text == "="
        }));
    }
}
