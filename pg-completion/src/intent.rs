//! Translation from parser grammar slots to catalog-facing completion intent.
//!
//! This module maps syntactic object categories and membership owners into the
//! object kinds a catalog adapter can resolve; it does not perform catalog I/O.

use pg_parser::{GrammarMembership, TextSize, object_type_slot};

use crate::{
    CatalogMembership, CompletionIntent, ExpectationSet, GrammarSlot, ObjectKind, ObjectReference,
    prefix,
};

pub(super) fn from_expectations(
    expectations: &ExpectationSet,
    source: &str,
    base: TextSize,
) -> CompletionIntent {
    let mut object_kinds = Vec::new();
    for slot in &expectations.slots {
        extend_unique(&mut object_kinds, object_kinds_for_slot(*slot));
    }
    let catalog_membership = expectations
        .membership
        .as_ref()
        .and_then(|grammar_membership| {
            catalog_membership_from_grammar(grammar_membership, source, base)
        });
    CompletionIntent {
        object_kinds,
        qualifier: Vec::new(),
        membership: catalog_membership,
    }
}

fn object_kinds_for_slot(slot: GrammarSlot) -> &'static [ObjectKind] {
    match slot {
        GrammarSlot::Relation => &[
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::MaterializedView,
            ObjectKind::ForeignTable,
            ObjectKind::Sequence,
            ObjectKind::Schema,
        ],
        GrammarSlot::Table => &[ObjectKind::Table],
        GrammarSlot::View => &[ObjectKind::View],
        GrammarSlot::MaterializedView => &[ObjectKind::MaterializedView],
        GrammarSlot::ForeignTable => &[ObjectKind::ForeignTable],
        GrammarSlot::Column => &[ObjectKind::Column],
        GrammarSlot::Attribute => &[ObjectKind::Attribute],
        GrammarSlot::Function => &[ObjectKind::Function],
        GrammarSlot::Procedure => &[ObjectKind::Procedure],
        GrammarSlot::Routine => &[ObjectKind::Routine],
        GrammarSlot::Aggregate => &[ObjectKind::Aggregate],
        GrammarSlot::Type => &[ObjectKind::Type],
        GrammarSlot::Domain => &[ObjectKind::Domain],
        GrammarSlot::Schema => &[ObjectKind::Schema],
        GrammarSlot::Sequence => &[ObjectKind::Sequence],
        GrammarSlot::Index => &[ObjectKind::Index],
        GrammarSlot::Constraint => &[ObjectKind::Constraint],
        GrammarSlot::Collation => &[ObjectKind::Collation],
        GrammarSlot::Operator => &[ObjectKind::Operator],
        GrammarSlot::OperatorClass => &[ObjectKind::OperatorClass],
        GrammarSlot::OperatorFamily => &[ObjectKind::OperatorFamily],
        GrammarSlot::Role => &[ObjectKind::Role],
        GrammarSlot::Database => &[ObjectKind::Database],
        GrammarSlot::AccessMethod => &[ObjectKind::AccessMethod],
        GrammarSlot::Conversion => &[ObjectKind::Conversion],
        GrammarSlot::EventTrigger => &[ObjectKind::EventTrigger],
        GrammarSlot::Extension => &[ObjectKind::Extension],
        GrammarSlot::ForeignDataWrapper => &[ObjectKind::ForeignDataWrapper],
        GrammarSlot::ForeignServer => &[ObjectKind::ForeignServer],
        GrammarSlot::Language => &[ObjectKind::Language],
        GrammarSlot::Policy => &[ObjectKind::Policy],
        GrammarSlot::PropertyGraph => &[ObjectKind::PropertyGraph],
        GrammarSlot::Publication => &[ObjectKind::Publication],
        GrammarSlot::Rule => &[ObjectKind::Rule],
        GrammarSlot::Statistics => &[ObjectKind::Statistics],
        GrammarSlot::Subscription => &[ObjectKind::Subscription],
        GrammarSlot::Tablespace => &[ObjectKind::Tablespace],
        GrammarSlot::TextSearchConfiguration => &[ObjectKind::TextSearchConfiguration],
        GrammarSlot::TextSearchDictionary => &[ObjectKind::TextSearchDictionary],
        GrammarSlot::TextSearchParser => &[ObjectKind::TextSearchParser],
        GrammarSlot::TextSearchTemplate => &[ObjectKind::TextSearchTemplate],
        GrammarSlot::Trigger => &[ObjectKind::Trigger],
        // Privileges are adapter-provided syntax names rather than Catalog
        // objects. AnyName covers non-Catalog identities such as cursors,
        // prepared statements, savepoints, and notification channels.
        GrammarSlot::Privilege | GrammarSlot::Alias | GrammarSlot::AnyName => &[],
    }
}

fn catalog_membership_from_grammar(
    membership: &GrammarMembership,
    source: &str,
    base: TextSize,
) -> Option<CatalogMembership> {
    let mut member_kinds = Vec::new();
    for slot in &membership.member_slots {
        extend_unique(&mut member_kinds, object_kinds_for_slot(*slot));
    }
    let mut object_kinds = Vec::new();
    for object_type in &membership.owner.object_types {
        extend_unique(
            &mut object_kinds,
            object_kinds_for_slot(object_type_slot(*object_type)),
        );
    }
    let name = membership
        .owner
        .name
        .iter()
        .map(|token| prefix::name_part_from_token(source, base, token))
        .collect::<Option<Vec<_>>>()?;
    (!member_kinds.is_empty() && !object_kinds.is_empty() && !name.is_empty()).then_some(
        CatalogMembership {
            member_kinds,
            owner: ObjectReference { object_kinds, name },
        },
    )
}

fn extend_unique(target: &mut Vec<ObjectKind>, values: &[ObjectKind]) {
    for value in values {
        push_unique(target, *value);
    }
}

fn push_unique(target: &mut Vec<ObjectKind>, value: ObjectKind) {
    if !target.contains(&value) {
        target.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_intent_is_catalog_facing_and_deduplicated() {
        let intent = from_expectations(
            &ExpectationSet {
                slots: vec![GrammarSlot::Relation, GrammarSlot::Schema],
                ..ExpectationSet::default()
            },
            "",
            TextSize::ZERO,
        );
        assert_eq!(
            intent
                .object_kinds
                .iter()
                .filter(|kind| **kind == ObjectKind::Schema)
                .count(),
            1
        );
        assert!(intent.object_kinds.contains(&ObjectKind::Table));
    }

    #[test]
    fn exact_relation_slots_do_not_depend_on_statement_tokens() {
        let intent = from_expectations(
            &ExpectationSet {
                slots: vec![GrammarSlot::Table, GrammarSlot::MaterializedView],
                ..ExpectationSet::default()
            },
            "",
            TextSize::ZERO,
        );
        assert_eq!(
            intent.object_kinds,
            [ObjectKind::Table, ObjectKind::MaterializedView]
        );
    }

    #[test]
    fn any_name_does_not_invent_catalog_object_kinds() {
        let expectations = ExpectationSet {
            slots: vec![GrammarSlot::AnyName],
            ..ExpectationSet::default()
        };
        let intent = from_expectations(&expectations, "", TextSize::ZERO);
        assert!(intent.object_kinds.is_empty());
    }
}
