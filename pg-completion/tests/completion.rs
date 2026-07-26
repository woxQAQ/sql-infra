use std::{collections::HashSet, fs, path::Path};

use pg_completion::{
    CompletionContext, CteDefinition, GrammarSlot, NamePart, ObjectContainer, RelationKind,
    ScopeSnapshot, VisibleRelation, collect,
};
use pg_parser::{KEYWORDS, TextSize, TokenKind, lex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    want: Want,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Want {
    candidates: Candidates,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    qualifier: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<ScopeWant>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Candidates {
    tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    phrases: Vec<String>,
    slots: Vec<String>,
}

/// A present `scope` block asserts the whole snapshot: an omitted field
/// asserts emptiness. Relations keep SQL visibility order and are not sorted.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ScopeWant {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    local: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outer: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ctes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dml_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merge_source: Option<String>,
}

#[test]
fn completion_candidates() {
    let record = std::env::var_os("PG_COMPLETION_RECORD").is_some();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/completion");
    let mut files = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut registered_starters = HashSet::new();
    let mut registered_slots = HashSet::new();
    let mut failures = Vec::new();

    for file in files {
        let mut cases: Vec<Case> =
            serde_yaml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        for (index, case) in cases.iter_mut().enumerate() {
            let (source, point) = remove_caret(&case.input);
            if let Ok(tokens) = lex(&source)
                && let Some(first) = tokens.first()
                && first.kind != TokenKind::Eof
            {
                registered_starters.insert(token_name(first.kind));
            }
            let context = collect(&source, TextSize::try_from(point).unwrap());
            let actual = candidates_of(&context);
            let actual_qualifier = context
                .intent
                .qualifier
                .iter()
                .map(|part| part.text.clone())
                .collect::<Vec<_>>();
            let actual_container = context.intent.container.as_ref().map(render_container);
            let actual_scope = render_scope(&context.scope);
            if record {
                case.want.candidates = actual;
                if case.want.qualifier.is_some() {
                    case.want.qualifier = Some(actual_qualifier);
                }
                if case.want.container.is_some() {
                    case.want.container = actual_container;
                }
                if case.want.scope.is_some() {
                    case.want.scope = Some(actual_scope);
                }
            } else {
                normalize(&mut case.want.candidates);
                if actual != case.want.candidates {
                    failures.push(format!(
                        "{} case {}: {}\n  actual: {:?}\n    want: {:?}",
                        file.display(),
                        index,
                        case.input,
                        actual,
                        case.want.candidates
                    ));
                }
                if let Some(want) = &case.want.qualifier
                    && *want != actual_qualifier
                {
                    failures.push(format!(
                        "{} case {}: {}\n  qualifier actual: {:?}\n    want: {:?}",
                        file.display(),
                        index,
                        case.input,
                        actual_qualifier,
                        want
                    ));
                }
                if let Some(want) = &case.want.container
                    && Some(want) != actual_container.as_ref()
                {
                    failures.push(format!(
                        "{} case {}: {}\n  container actual: {:?}\n    want: {:?}",
                        file.display(),
                        index,
                        case.input,
                        actual_container,
                        want
                    ));
                }
                if let Some(want) = &case.want.scope
                    && *want != actual_scope
                {
                    failures.push(format!(
                        "{} case {}: {}\n  scope actual: {:?}\n    want: {:?}",
                        file.display(),
                        index,
                        case.input,
                        actual_scope,
                        want
                    ));
                }
            }
            registered_slots.extend(case.want.candidates.slots.iter().cloned());
        }
        if record {
            fs::write(&file, render_cases(&cases)).unwrap();
        }
    }

    let mut missing_starters = candidates_of(&collect("", TextSize::new(0)))
        .tokens
        .into_iter()
        .filter(|starter| !registered_starters.contains(starter))
        .collect::<Vec<_>>();
    missing_starters.sort();
    assert!(
        missing_starters.is_empty(),
        "top-level completion starters without a YAML scenario: {missing_starters:?}"
    );
    let missing_slots = all_grammar_slots()
        .iter()
        .map(|slot| format!("{slot:?}"))
        .filter(|slot| !registered_slots.contains(slot))
        .collect::<Vec<_>>();
    assert!(
        missing_slots.is_empty(),
        "grammar slots without a YAML scenario: {missing_slots:?}"
    );
    assert!(
        failures.is_empty(),
        "completion candidate mismatches:\n{}",
        failures.join("\n")
    );
}

macro_rules! grammar_slots {
    ($($slot:ident),+ $(,)?) => {
        fn all_grammar_slots() -> &'static [GrammarSlot] {
            fn exhaustive(slot: GrammarSlot) {
                match slot {
                    $(GrammarSlot::$slot => {}),+
                }
            }
            let _ = exhaustive;
            &[$(GrammarSlot::$slot),+]
        }
    };
}

grammar_slots! {
    Relation,
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Column,
    Attribute,
    Function,
    Procedure,
    Routine,
    Aggregate,
    Type,
    Domain,
    Schema,
    Sequence,
    Index,
    Constraint,
    Collation,
    Operator,
    OperatorClass,
    OperatorFamily,
    Role,
    Database,
    AccessMethod,
    Conversion,
    EventTrigger,
    Extension,
    ForeignDataWrapper,
    ForeignServer,
    Language,
    Policy,
    PropertyGraph,
    Publication,
    Rule,
    Statistics,
    Subscription,
    Tablespace,
    TextSearchConfiguration,
    TextSearchDictionary,
    TextSearchParser,
    TextSearchTemplate,
    Trigger,
    AnyName,
}

fn render_cases(cases: &[Case]) -> String {
    let mut output = String::new();
    for (index, case) in cases.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        output.push_str("- input: ");
        output.push_str(&yaml_scalar(&case.input));
        output.push_str("\n  want:\n    candidates:\n      tokens: ");
        output.push_str(&flow_sequence(&case.want.candidates.tokens));
        if !case.want.candidates.phrases.is_empty() {
            output.push_str("\n      phrases: ");
            output.push_str(&flow_sequence(&case.want.candidates.phrases));
        }
        output.push_str("\n      slots: ");
        output.push_str(&flow_sequence(&case.want.candidates.slots));
        if let Some(qualifier) = &case.want.qualifier {
            output.push_str("\n    qualifier: ");
            output.push_str(&flow_sequence(qualifier));
        }
        if let Some(container) = &case.want.container {
            output.push_str("\n    container: ");
            output.push_str(&yaml_scalar(container));
        }
        if let Some(scope) = &case.want.scope {
            output.push_str("\n    scope:");
            output.push_str(&render_scope_block(scope));
        }
        output.push('\n');
    }
    output
}

fn render_scope_block(scope: &ScopeWant) -> String {
    let mut output = String::new();
    if !scope.local.is_empty() {
        output.push_str("\n      local: ");
        output.push_str(&flow_sequence(&scope.local));
    }
    if !scope.outer.is_empty() {
        output.push_str("\n      outer: [");
        output.push_str(
            &scope
                .outer
                .iter()
                .map(|layer| flow_sequence(layer))
                .collect::<Vec<_>>()
                .join(", "),
        );
        output.push(']');
    }
    if !scope.ctes.is_empty() {
        output.push_str("\n      ctes: ");
        output.push_str(&flow_sequence(&scope.ctes));
    }
    if let Some(target) = &scope.dml_target {
        output.push_str("\n      dml_target: ");
        output.push_str(&yaml_scalar(target));
    }
    if let Some(source) = &scope.merge_source {
        output.push_str("\n      merge_source: ");
        output.push_str(&yaml_scalar(source));
    }
    if output.is_empty() {
        output.push_str(" {}");
    }
    output
}

fn flow_sequence(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| flow_scalar(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn yaml_scalar(value: &str) -> String {
    serde_yaml::to_string(value)
        .expect("fixture strings serialize as YAML scalars")
        .trim()
        .to_owned()
}

/// Block-context scalars may contain flow indicators; quote them before
/// embedding in a `[...]` sequence.
fn flow_scalar(value: &str) -> String {
    let rendered = yaml_scalar(value);
    if !rendered.starts_with(['\'', '"']) && rendered.contains([',', '[', ']', '{', '}']) {
        format!("'{}'", rendered.replace('\'', "''"))
    } else {
        rendered
    }
}

fn candidates_of(context: &CompletionContext) -> Candidates {
    let mut candidates = Candidates {
        tokens: context
            .expectations
            .tokens
            .iter()
            .map(|kind| token_name(*kind))
            .collect(),
        phrases: context
            .expectations
            .phrases
            .iter()
            .map(|phrase| phrase_name(phrase))
            .collect(),
        slots: context
            .expectations
            .slots
            .iter()
            .map(|slot| format!("{slot:?}"))
            .collect(),
    };
    normalize(&mut candidates);
    candidates
}

fn phrase_name(phrase: &[TokenKind]) -> String {
    phrase
        .iter()
        .map(|kind| token_name(*kind))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_scope(scope: &ScopeSnapshot) -> ScopeWant {
    ScopeWant {
        local: scope.local.relations.iter().map(render_relation).collect(),
        outer: scope
            .outer
            .iter()
            .map(|layer| layer.relations.iter().map(render_relation).collect())
            .collect(),
        ctes: scope.ctes.iter().map(render_cte).collect(),
        dml_target: scope.dml_target.as_ref().map(render_relation),
        merge_source: scope.merge_source.as_ref().map(render_relation),
    }
}

fn render_relation(relation: &VisibleRelation) -> String {
    let mut parts = Vec::new();
    if relation.lateral {
        parts.push("lateral".to_owned());
    }
    match relation.kind {
        RelationKind::Relation => {}
        RelationKind::Cte => parts.push("cte".to_owned()),
        RelationKind::Subquery => parts.push("subquery".to_owned()),
        RelationKind::TableFunction => parts.push("function".to_owned()),
        RelationKind::JoinAlias => parts.push("join".to_owned()),
        RelationKind::Values => parts.push("values".to_owned()),
    }
    let name = dotted(&relation.name);
    if !name.is_empty() {
        parts.push(name);
    }
    if let Some(alias) = &relation.alias {
        parts.push("AS".to_owned());
        parts.push(alias.text.clone());
    }
    let mut rendered = parts.join(" ");
    rendered.push_str(&columns_suffix(&relation.explicit_columns));
    if relation.unsupported.is_some() {
        rendered.push_str(" [unsupported]");
    }
    rendered
}

fn render_cte(cte: &CteDefinition) -> String {
    let mut rendered = cte.name.text.clone();
    rendered.push_str(&columns_suffix(&cte.explicit_columns));
    rendered
}

fn render_container(container: &ObjectContainer) -> String {
    format!(
        "{} in {} {}",
        container
            .members
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        container
            .reference
            .object_kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join("|"),
        dotted(&container.reference.name),
    )
}

fn dotted(name: &[NamePart]) -> String {
    name.iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn columns_suffix(columns: &[NamePart]) -> String {
    if columns.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        columns
            .iter()
            .map(|column| column.text.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn normalize(candidates: &mut Candidates) {
    candidates.tokens.sort();
    candidates.tokens.dedup();
    candidates.phrases.sort();
    candidates.phrases.dedup();
    candidates.slots.sort();
    candidates.slots.dedup();
}

fn remove_caret(input: &str) -> (String, usize) {
    let mut matches = input.match_indices('|');
    let (point, _) = matches
        .next()
        .expect("completion input must contain one '|'");
    assert!(
        matches.next().is_none(),
        "completion input must contain exactly one '|'"
    );
    let mut source = input.to_owned();
    source.remove(point);
    (source, point)
}

fn token_name(kind: TokenKind) -> String {
    if let TokenKind::Char(ch) = kind {
        return ch.to_string();
    }
    if let Some(keyword) = KEYWORDS.iter().find(|keyword| keyword.kind == kind) {
        return keyword.word.to_ascii_uppercase();
    }
    format!("{kind:?}")
}
