use std::{collections::HashSet, fs, path::Path};

use pg_completion::{
    CatalogMembership, CompletionContext, CteDefinition, GrammarSlot, NamePart, ObjectKind,
    RelationKind, ScopeSnapshot, VisibleRelation, collect,
};
use pg_parser::{KEYWORDS, TextSize, TokenKind, lex, parse_one};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    want: Want,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionWitness {
    name: String,
    sql: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Want {
    candidates: Candidates,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    qualifier: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    membership: Option<String>,
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
                && path
                    .file_name()
                    .is_none_or(|name| name != "position-witnesses.yaml")
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
            let actual_membership = context.intent.membership.as_ref().map(render_membership);
            let actual_scope = render_scope(&context.scope);
            if record {
                case.want.candidates = actual;
                if case.want.qualifier.is_some() {
                    case.want.qualifier = Some(actual_qualifier);
                }
                if case.want.membership.is_some() {
                    case.want.membership = actual_membership;
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
                if let Some(want) = &case.want.membership
                    && Some(want) != actual_membership.as_ref()
                {
                    failures.push(format!(
                        "{} case {}: {}\n  membership actual: {:?}\n    want: {:?}",
                        file.display(),
                        index,
                        case.input,
                        actual_membership,
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

#[test]
fn every_token_in_complete_family_witnesses_is_reachable_through_completion() {
    let witnesses = completion_witnesses();
    let mut missing = Vec::new();

    for (name, source) in witnesses {
        parse_one(&source)
            .unwrap_or_else(|error| panic!("completion witness {name:?} is invalid: {error}"));
        let tokens = lex(&source)
            .unwrap_or_else(|error| panic!("failed to lex family witness {source:?}: {error}"));

        for token in tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::Eof | TokenKind::Char(';')))
        {
            let original_point = token.range.start();
            let original = collect(&source, original_point);
            assert_expectation_provenance(&source, &original);

            let (prefix_source, point) = source_before_token(&source, token.range.start(), "");
            let context = collect(&prefix_source, TextSize::try_from(point).unwrap());
            assert_expectation_provenance(&prefix_source, &context);

            let token_is_raw_syntax = raw_syntax_token(token.kind);
            let token_is_name = matches!(token.kind, TokenKind::Ident | TokenKind::UIdent);
            if token_is_raw_syntax
                && !expectations_accept_token(&context, token.kind)
                && context.expectations.slots.is_empty()
            {
                missing.push(format!(
                    "missing raw token {:?} at byte {} in {:?}: {:?}",
                    token.kind,
                    usize::from(token.range.start()),
                    name,
                    context.expectations
                ));
                continue;
            }
            if token_is_name && context.expectations.slots.is_empty() {
                missing.push(format!(
                    "identifier position has no GrammarSlot at byte {} in {:?}: {:?}",
                    usize::from(token.range.start()),
                    name,
                    context.expectations
                ));
                continue;
            }

            let Some(keyword) = KEYWORDS.iter().find(|keyword| keyword.kind == token.kind) else {
                continue;
            };
            if !context.expectations.tokens.contains(&token.kind) {
                continue;
            }
            let token_text =
                &source[usize::from(token.range.start())..usize::from(token.range.end())];
            let label = keyword.word.to_ascii_uppercase();
            let recovered = token_text
                .char_indices()
                .map(|(index, character)| index + character.len_utf8())
                .any(|prefix_len| {
                    let (prefixed, point) = source_before_token(
                        &source,
                        token.range.start(),
                        &token_text[..prefix_len],
                    );
                    let prefixed = collect(&prefixed, TextSize::try_from(point).unwrap());
                    prefixed.expectations.tokens.contains(&token.kind)
                        && prefixed
                            .syntax_completions()
                            .iter()
                            .any(|completion| completion.label == label)
                });
            if !recovered {
                missing.push(format!(
                    "no typed keyword prefix recovers {label} at byte {} in {:?}",
                    usize::from(token.range.start()),
                    name
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "completion witness gaps:\n{}",
        missing.join("\n")
    );
}

#[test]
fn editor_projection_stays_quiet_across_every_witness_boundary() {
    let mut violations = Vec::new();

    for (name, source) in completion_witnesses() {
        parse_one(&source)
            .unwrap_or_else(|error| panic!("completion witness {name:?} is invalid: {error}"));
        let tokens = lex(&source)
            .unwrap_or_else(|error| panic!("failed to lex completion witness {name:?}: {error}"));
        let mut points = tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .flat_map(|token| [token.range.start(), token.range.end()])
            .collect::<Vec<_>>();
        points.push(TextSize::try_from(source.len()).expect("witness length fits TextSize"));
        points.sort_unstable();
        points.dedup();

        for point in points {
            let context = collect(&source, point);
            check_editor_projection(&name, point, &context, &mut violations);
        }

        let complete_source = format!("{source} ");
        let end = collect(
            &complete_source,
            TextSize::try_from(complete_source.len()).expect("witness length fits TextSize"),
        );
        let stale_slots = end
            .expectations
            .slots
            .iter()
            .filter(|slot| **slot != GrammarSlot::Alias)
            .collect::<Vec<_>>();
        if !stale_slots.is_empty() || !end.intent.object_kinds.is_empty() {
            violations.push(format!(
                "{name:?} has stale catalog intent at the complete statement: slots={:?}, kinds={:?}",
                end.expectations.slots, end.intent.object_kinds
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "editor projection invariant failures:\n{}",
        violations.join("\n")
    );
}

fn completion_witnesses() -> Vec<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/completion/family-ends.yaml");
    let cases: Vec<Case> =
        serde_yaml::from_str(&fs::read_to_string(&path).expect("read family witnesses"))
            .expect("parse family witnesses");
    let position_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/completion/position-witnesses.yaml");
    let positions: Vec<PositionWitness> =
        serde_yaml::from_str(&fs::read_to_string(&position_path).expect("read position witnesses"))
            .expect("parse position witnesses");
    let mut witnesses = cases
        .into_iter()
        .map(|case| {
            let (source, _) = remove_caret(&case.input);
            (case.input, source)
        })
        .collect::<Vec<_>>();
    witnesses.extend(
        positions
            .into_iter()
            .map(|witness| (witness.name, witness.sql)),
    );
    witnesses
}

fn check_editor_projection(
    name: &str,
    point: TextSize,
    context: &CompletionContext,
    violations: &mut Vec<String>,
) {
    let operator_slot = context.expectations.slots.contains(&GrammarSlot::Operator);
    let operator_intent = context.intent.object_kinds.contains(&ObjectKind::Operator);
    if operator_slot != operator_intent {
        violations.push(format!(
            "{name:?} at byte {} does not map the explicit Operator slot exactly: slots={:?}, kinds={:?}",
            usize::from(point),
            context.expectations.slots,
            context.intent.object_kinds
        ));
    }
    if !operator_slot
        && !context
            .expectations
            .expression_continuation_tokens
            .is_empty()
        && operator_intent
    {
        violations.push(format!(
            "{name:?} at byte {} exposes an Operator catalog query from an ordinary expression continuation",
            usize::from(point)
        ));
    }

    for completion in context.syntax_completions() {
        let words = completion
            .label
            .split_ascii_whitespace()
            .collect::<Vec<_>>();
        let keyword_only = !words.is_empty()
            && words.iter().all(|word| {
                KEYWORDS
                    .iter()
                    .any(|keyword| keyword.word.eq_ignore_ascii_case(word))
            });
        if !keyword_only {
            violations.push(format!(
                "{name:?} at byte {} exposes punctuation or a symbolic operator as an editor item: {:?}",
                usize::from(point),
                completion
            ));
            continue;
        }
        if context.prefix.raw.is_empty() {
            let head = KEYWORDS
                .iter()
                .find(|keyword| keyword.word.eq_ignore_ascii_case(words[0]))
                .expect("keyword-only completion has a keyword head")
                .kind;
            let eager = context.expectations.direct_tokens.contains(&head)
                || (head != TokenKind::Operator
                    && context.expectations.expression_start_tokens.contains(&head));
            if !eager {
                violations.push(format!(
                    "{name:?} at byte {} eagerly exposes a lookahead/follow/expression-tail-only item: {:?}",
                    usize::from(point),
                    completion
                ));
            }
        }
    }
}

fn expectations_accept_token(context: &CompletionContext, kind: TokenKind) -> bool {
    context.expectations.tokens.contains(&kind)
        || (context.expectations.tokens.contains(&TokenKind::Op)
            && matches!(
                kind,
                TokenKind::Op
                    | TokenKind::RightArrow
                    | TokenKind::LessEquals
                    | TokenKind::GreaterEquals
                    | TokenKind::NotEquals
                    | TokenKind::Char(
                        '+' | '-'
                            | '*'
                            | '/'
                            | '%'
                            | '^'
                            | '<'
                            | '>'
                            | '='
                            | '~'
                            | '!'
                            | '@'
                            | '#'
                            | '&'
                            | '|'
                            | '?'
                            | '`'
                            | ':'
                    )
            ))
}

fn source_before_token(source: &str, start: TextSize, partial: &str) -> (String, usize) {
    let start = usize::from(start);
    let mut prefix = String::with_capacity(start + partial.len() + 1);
    prefix.push_str(&source[..start]);
    prefix.push(' ');
    prefix.push_str(partial);
    let point = prefix.len();
    (prefix, point)
}

fn raw_syntax_token(kind: TokenKind) -> bool {
    KEYWORDS.iter().any(|keyword| keyword.kind == kind)
        || matches!(
            kind,
            TokenKind::Char(_)
                | TokenKind::Op
                | TokenKind::TypeCast
                | TokenKind::DotDot
                | TokenKind::ColonEquals
                | TokenKind::EqualsGreater
                | TokenKind::LessEquals
                | TokenKind::GreaterEquals
                | TokenKind::NotEquals
                | TokenKind::RightArrow
        )
}

fn assert_expectation_provenance(source: &str, context: &CompletionContext) {
    for token in &context.expectations.tokens {
        assert!(
            context.expectations.direct_tokens.contains(token)
                || context.expectations.lookahead_tokens.contains(token)
                || context.expectations.expression_start_tokens.contains(token)
                || context
                    .expectations
                    .expression_continuation_tokens
                    .contains(token)
                || context.expectations.follow_tokens.contains(token),
            "token without provenance in {source:?}: {token:?}: {:?}",
            context.expectations
        );
    }
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
    Privilege,
    Alias,
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
        if let Some(membership) = &case.want.membership {
            output.push_str("\n    membership: ");
            output.push_str(&yaml_scalar(membership));
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

fn render_membership(membership: &CatalogMembership) -> String {
    format!(
        "{} in {} {}",
        membership
            .member_kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        membership
            .owner
            .object_kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join("|"),
        dotted(&membership.owner.name),
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
