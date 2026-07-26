use pg_parser::{TextSize, Token, TokenKind};

use crate::{
    CompletionIntent, ExpectationSet, GrammarSlot, ObjectContainer, ObjectKind, ObjectReference,
    prefix,
};

pub(super) fn from_expectations(expectations: &ExpectationSet) -> CompletionIntent {
    let mut object_kinds = Vec::new();
    for slot in &expectations.slots {
        match slot {
            GrammarSlot::Relation => extend_unique(
                &mut object_kinds,
                &[
                    ObjectKind::Table,
                    ObjectKind::View,
                    ObjectKind::MaterializedView,
                    ObjectKind::ForeignTable,
                    ObjectKind::Sequence,
                    ObjectKind::Schema,
                ],
            ),
            GrammarSlot::Table => push_unique(&mut object_kinds, ObjectKind::Table),
            GrammarSlot::View => push_unique(&mut object_kinds, ObjectKind::View),
            GrammarSlot::MaterializedView => {
                push_unique(&mut object_kinds, ObjectKind::MaterializedView)
            }
            GrammarSlot::ForeignTable => push_unique(&mut object_kinds, ObjectKind::ForeignTable),
            GrammarSlot::Column => push_unique(&mut object_kinds, ObjectKind::Column),
            GrammarSlot::Attribute => push_unique(&mut object_kinds, ObjectKind::Attribute),
            GrammarSlot::Function => push_unique(&mut object_kinds, ObjectKind::Function),
            GrammarSlot::Procedure => push_unique(&mut object_kinds, ObjectKind::Procedure),
            GrammarSlot::Routine => push_unique(&mut object_kinds, ObjectKind::Routine),
            GrammarSlot::Aggregate => push_unique(&mut object_kinds, ObjectKind::Aggregate),
            GrammarSlot::Type => push_unique(&mut object_kinds, ObjectKind::Type),
            GrammarSlot::Domain => push_unique(&mut object_kinds, ObjectKind::Domain),
            GrammarSlot::Schema => push_unique(&mut object_kinds, ObjectKind::Schema),
            GrammarSlot::Sequence => push_unique(&mut object_kinds, ObjectKind::Sequence),
            GrammarSlot::Index => push_unique(&mut object_kinds, ObjectKind::Index),
            GrammarSlot::Constraint => push_unique(&mut object_kinds, ObjectKind::Constraint),
            GrammarSlot::Collation => push_unique(&mut object_kinds, ObjectKind::Collation),
            GrammarSlot::Operator => push_unique(&mut object_kinds, ObjectKind::Operator),
            GrammarSlot::OperatorClass => push_unique(&mut object_kinds, ObjectKind::OperatorClass),
            GrammarSlot::OperatorFamily => {
                push_unique(&mut object_kinds, ObjectKind::OperatorFamily)
            }
            GrammarSlot::Role => push_unique(&mut object_kinds, ObjectKind::Role),
            GrammarSlot::Database => push_unique(&mut object_kinds, ObjectKind::Database),
            GrammarSlot::AccessMethod => push_unique(&mut object_kinds, ObjectKind::AccessMethod),
            GrammarSlot::Conversion => push_unique(&mut object_kinds, ObjectKind::Conversion),
            GrammarSlot::EventTrigger => push_unique(&mut object_kinds, ObjectKind::EventTrigger),
            GrammarSlot::Extension => push_unique(&mut object_kinds, ObjectKind::Extension),
            GrammarSlot::ForeignDataWrapper => {
                push_unique(&mut object_kinds, ObjectKind::ForeignDataWrapper)
            }
            GrammarSlot::ForeignServer => push_unique(&mut object_kinds, ObjectKind::ForeignServer),
            GrammarSlot::Language => push_unique(&mut object_kinds, ObjectKind::Language),
            GrammarSlot::Policy => push_unique(&mut object_kinds, ObjectKind::Policy),
            GrammarSlot::PropertyGraph => push_unique(&mut object_kinds, ObjectKind::PropertyGraph),
            GrammarSlot::Publication => push_unique(&mut object_kinds, ObjectKind::Publication),
            GrammarSlot::Rule => push_unique(&mut object_kinds, ObjectKind::Rule),
            GrammarSlot::Statistics => push_unique(&mut object_kinds, ObjectKind::Statistics),
            GrammarSlot::Subscription => push_unique(&mut object_kinds, ObjectKind::Subscription),
            GrammarSlot::Tablespace => push_unique(&mut object_kinds, ObjectKind::Tablespace),
            GrammarSlot::TextSearchConfiguration => {
                push_unique(&mut object_kinds, ObjectKind::TextSearchConfiguration)
            }
            GrammarSlot::TextSearchDictionary => {
                push_unique(&mut object_kinds, ObjectKind::TextSearchDictionary)
            }
            GrammarSlot::TextSearchParser => {
                push_unique(&mut object_kinds, ObjectKind::TextSearchParser)
            }
            GrammarSlot::TextSearchTemplate => {
                push_unique(&mut object_kinds, ObjectKind::TextSearchTemplate)
            }
            GrammarSlot::Trigger => push_unique(&mut object_kinds, ObjectKind::Trigger),
            // This slot only says that the grammar accepts a name. It is also
            // used for non-Catalog identities such as cursors, prepared
            // statements, savepoints, and notification channels. Publishing
            // arbitrary Catalog kinds here would turn missing semantic
            // classification into misleading adapter queries.
            GrammarSlot::AnyName => {}
        }
    }
    CompletionIntent {
        object_kinds,
        qualifier: Vec::new(),
        container: None,
    }
}

pub(super) fn attach_container(
    intent: &mut CompletionIntent,
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) {
    if !intent.object_kinds.iter().any(|kind| {
        matches!(
            kind,
            ObjectKind::Column
                | ObjectKind::Attribute
                | ObjectKind::Constraint
                | ObjectKind::Trigger
                | ObjectKind::Policy
                | ObjectKind::Rule
        )
    }) {
        return;
    }

    let reference = alter_container(source, base, point, tokens)
        .or_else(|| copy_container(source, base, point, tokens))
        .or_else(|| references_container(source, base, point, tokens))
        .or_else(|| create_trigger_container(source, base, point, tokens))
        .or_else(|| create_index_container(source, base, point, tokens))
        .or_else(|| create_statistics_container(source, base, point, tokens))
        .or_else(|| grant_container(source, base, point, tokens))
        .or_else(|| vacuum_container(source, base, point, tokens));
    if let Some(reference) = reference {
        let members = intent
            .object_kinds
            .iter()
            .copied()
            .filter(|kind| {
                matches!(
                    kind,
                    ObjectKind::Column
                        | ObjectKind::Attribute
                        | ObjectKind::Constraint
                        | ObjectKind::Trigger
                        | ObjectKind::Policy
                        | ObjectKind::Rule
                )
            })
            .collect();
        intent.container = Some(ObjectContainer { members, reference });
    }
}

fn alter_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if token_kind(tokens, 0) != TokenKind::Alter {
        return None;
    }
    let (object_kinds, mut index) = match token_kind(tokens, 1) {
        TokenKind::Table => (vec![ObjectKind::Table], 2),
        TokenKind::View => (vec![ObjectKind::View], 2),
        TokenKind::Materialized if token_kind(tokens, 2) == TokenKind::View => {
            (vec![ObjectKind::MaterializedView], 3)
        }
        TokenKind::Foreign if token_kind(tokens, 2) == TokenKind::Table => {
            (vec![ObjectKind::ForeignTable], 3)
        }
        TokenKind::DomainP => (vec![ObjectKind::Domain], 2),
        TokenKind::TypeP => (vec![ObjectKind::Type], 2),
        _ => return None,
    };
    if token_kind(tokens, index) == TokenKind::IfP
        && token_kind(tokens, index + 1) == TokenKind::Exists
    {
        index += 2;
    }
    if token_kind(tokens, index) == TokenKind::Only {
        index += 1;
    }
    let (name, next) = qualified_name(source, base, tokens, index)?;
    (tokens[next.saturating_sub(1)].range.end() <= point)
        .then_some(ObjectReference { object_kinds, name })
}

fn copy_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    let mut index = 0;
    if token_kind(tokens, index) == TokenKind::Binary {
        index += 1;
    }
    if token_kind(tokens, index) != TokenKind::Copy {
        return None;
    }
    index += 1;
    if token_kind(tokens, index) == TokenKind::Char('(') {
        return None;
    }
    let (name, next) = qualified_name(source, base, tokens, index)?;
    (tokens[next.saturating_sub(1)].range.end() <= point).then_some(ObjectReference {
        object_kinds: vec![ObjectKind::Table, ObjectKind::ForeignTable],
        name,
    })
}

fn references_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if !matches!(token_kind(tokens, 0), TokenKind::Create | TokenKind::Alter) {
        return None;
    }
    let reference = tokens
        .iter()
        .enumerate()
        .take_while(|(_, token)| token.range.start() < point)
        .filter(|(_, token)| token.kind == TokenKind::References)
        .map(|(index, _)| index)
        .last()?;
    let (name, next) = qualified_name(source, base, tokens, reference + 1)?;
    (tokens[next.saturating_sub(1)].range.end() <= point).then_some(ObjectReference {
        object_kinds: vec![ObjectKind::Table],
        name,
    })
}

fn create_trigger_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if token_kind(tokens, 0) != TokenKind::Create {
        return None;
    }
    let trigger = tokens
        .iter()
        .position(|token| token.kind == TokenKind::Trigger)?;
    let on = tokens
        .iter()
        .enumerate()
        .skip(trigger + 1)
        .find(|(_, token)| token.kind == TokenKind::On)
        .map(|(index, _)| index)?;
    let (name, next) = qualified_name(source, base, tokens, on + 1)?;
    // Column candidates reference the table named after ON from both sides:
    // `UPDATE OF |` precedes it, `WHEN (...)` follows its complete name.
    (tokens[on].range.start() >= point || tokens[next.saturating_sub(1)].range.end() <= point)
        .then_some(ObjectReference {
            object_kinds: vec![
                ObjectKind::Table,
                ObjectKind::View,
                ObjectKind::ForeignTable,
            ],
            name,
        })
}

fn create_index_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if token_kind(tokens, 0) != TokenKind::Create {
        return None;
    }
    let index = usize::from(token_kind(tokens, 1) == TokenKind::Unique) + 1;
    if token_kind(tokens, index) != TokenKind::Index {
        return None;
    }
    let on = tokens
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, token)| token.kind == TokenKind::On)
        .map(|(index, _)| index)?;
    let (name, next) = qualified_name(source, base, tokens, on + 1)?;
    (tokens[next.saturating_sub(1)].range.end() <= point).then_some(relation_reference(name))
}

fn create_statistics_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if token_kind(tokens, 0) != TokenKind::Create || token_kind(tokens, 1) != TokenKind::Statistics
    {
        return None;
    }
    let from = tokens
        .iter()
        .enumerate()
        .find(|(_, token)| token.kind == TokenKind::From && token.range.start() >= point)
        .map(|(index, _)| index)?;
    let (name, _) = qualified_name(source, base, tokens, from + 1)?;
    Some(relation_reference(name))
}

fn grant_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if !matches!(token_kind(tokens, 0), TokenKind::Grant | TokenKind::Revoke) {
        return None;
    }
    let on = tokens
        .iter()
        .enumerate()
        .find(|(_, token)| token.kind == TokenKind::On && token.range.start() >= point)
        .map(|(index, _)| index)?;
    let mut index = on + 1;
    if token_kind(tokens, index) == TokenKind::Table {
        index += 1;
    }
    let (name, _) = qualified_name(source, base, tokens, index)?;
    Some(relation_reference(name))
}

fn vacuum_container(
    source: &str,
    base: TextSize,
    point: TextSize,
    tokens: &[Token],
) -> Option<ObjectReference> {
    if !matches!(
        token_kind(tokens, 0),
        TokenKind::Vacuum | TokenKind::Analyze
    ) {
        return None;
    }
    let open = tokens
        .iter()
        .enumerate()
        .take_while(|(_, token)| token.range.start() < point)
        .filter(|(_, token)| token.kind == TokenKind::Char('('))
        .map(|(index, _)| index)
        .last()?;
    let name = qualified_name_before(source, base, tokens, open)?;
    Some(relation_reference(name))
}

fn relation_reference(name: Vec<crate::NamePart>) -> ObjectReference {
    ObjectReference {
        object_kinds: vec![
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::MaterializedView,
            ObjectKind::ForeignTable,
        ],
        name,
    }
}

fn qualified_name(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    mut index: usize,
) -> Option<(Vec<crate::NamePart>, usize)> {
    let mut name = vec![prefix::name_part_from_token(
        source,
        base,
        tokens.get(index)?,
    )?];
    index += 1;
    while token_kind(tokens, index) == TokenKind::Char('.') {
        let part = prefix::name_part_from_token(source, base, tokens.get(index + 1)?)?;
        name.push(part);
        index += 2;
    }
    Some((name, index))
}

fn qualified_name_before(
    source: &str,
    base: TextSize,
    tokens: &[Token],
    end: usize,
) -> Option<Vec<crate::NamePart>> {
    let mut index = end.checked_sub(1)?;
    let mut name = vec![prefix::name_part_from_token(
        source,
        base,
        tokens.get(index)?,
    )?];
    while index >= 2 && token_kind(tokens, index - 1) == TokenKind::Char('.') {
        index -= 2;
        name.push(prefix::name_part_from_token(
            source,
            base,
            tokens.get(index)?,
        )?);
    }
    name.reverse();
    Some(name)
}

fn token_kind(tokens: &[Token], index: usize) -> TokenKind {
    tokens
        .get(index)
        .map(|token| token.kind)
        .unwrap_or(TokenKind::Eof)
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
        let intent = from_expectations(&ExpectationSet {
            tokens: Vec::new(),
            phrases: Vec::new(),
            slots: vec![GrammarSlot::Relation, GrammarSlot::Schema],
        });
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
        let intent = from_expectations(&ExpectationSet {
            tokens: Vec::new(),
            phrases: Vec::new(),
            slots: vec![GrammarSlot::Table, GrammarSlot::MaterializedView],
        });
        assert_eq!(
            intent.object_kinds,
            [ObjectKind::Table, ObjectKind::MaterializedView]
        );
    }

    #[test]
    fn any_name_does_not_invent_catalog_object_kinds() {
        let expectations = ExpectationSet {
            tokens: Vec::new(),
            phrases: Vec::new(),
            slots: vec![GrammarSlot::AnyName],
        };
        let intent = from_expectations(&expectations);
        assert!(intent.object_kinds.is_empty());
    }
}
