//! PostgreSQL completion context collection.
//!
//! This crate deliberately stops before catalog resolution and presentation.

mod intent;
mod lexical;
mod prefix;
mod scope;
mod statement;

use pg_parser::{KEYWORDS, TextRange, TextSize, TokenKind, collect_expectations};

pub use pg_parser::GrammarSlot;
pub use prefix::{CompletionPrefix, IdentifierQuoting, NamePart};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionContext {
    pub statement_range: TextRange,
    pub point: TextSize,
    pub replacement_range: TextRange,
    pub prefix: CompletionPrefix,
    pub expectations: ExpectationSet,
    pub intent: CompletionIntent,
    pub scope: ScopeSnapshot,
    pub recovery: CompletionRecovery,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpectationSet {
    pub tokens: Vec<TokenKind>,
    /// Fixed multi-token units that are grammatical at the point, e.g.
    /// `GROUP BY` or `IF NOT EXISTS`. Each phrase's head token also appears
    /// in `tokens`; a phrase does not claim the head has no other
    /// continuation.
    pub phrases: Vec<&'static [TokenKind]>,
    pub slots: Vec<GrammarSlot>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionIntent {
    pub object_kinds: Vec<ObjectKind>,
    pub qualifier: Vec<NamePart>,
    pub container: Option<ObjectContainer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectContainer {
    pub members: Vec<ObjectKind>,
    pub reference: ObjectReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectReference {
    pub object_kinds: Vec<ObjectKind>,
    pub name: Vec<NamePart>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObjectKind {
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Sequence,
    Index,
    Column,
    Attribute,
    Function,
    Procedure,
    Routine,
    Aggregate,
    Type,
    Domain,
    Schema,
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
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeSnapshot {
    pub local: QueryScope,
    pub outer: Vec<QueryScope>,
    pub ctes: Vec<CteDefinition>,
    pub dml_target: Option<VisibleRelation>,
    pub merge_source: Option<VisibleRelation>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryScope {
    pub relations: Vec<VisibleRelation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CteDefinition {
    pub name: NamePart,
    pub explicit_columns: Vec<NamePart>,
    pub syntax_range: TextRange,
    pub body_range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleRelation {
    pub kind: RelationKind,
    pub name: Vec<NamePart>,
    pub alias: Option<NamePart>,
    pub explicit_columns: Vec<NamePart>,
    pub syntax_range: TextRange,
    pub body_range: Option<TextRange>,
    pub lateral: bool,
    pub unsupported: Option<UnsupportedRelation>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RelationKind {
    Relation,
    Cte,
    Subquery,
    TableFunction,
    JoinAlias,
    Values,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedRelation {
    pub range: TextRange,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionRecovery {
    pub issues: Vec<RecoveryIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryIssue {
    pub kind: RecoveryKind,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryKind {
    PointClampedToEof,
    PointMovedToCharBoundary,
    UnterminatedToken,
    LexErrorBeforePoint,
    ScopeIncomplete,
}

/// Collect syntax and scope information for completion at a UTF-8 byte point.
///
/// The requested point is clamped and normalized before any slicing occurs.
pub fn collect(source: &str, point: TextSize) -> CompletionContext {
    let normalized = prefix::normalize_point(source, point);
    let statement_range = statement::containing_statement(source, normalized.point);
    let prefix = prefix::extract(source, statement_range, normalized.point);
    let mut recovery = CompletionRecovery {
        issues: normalized.issues,
    };
    recovery.issues.extend(prefix.issues);

    let statement_start = usize::from(statement_range.start());
    let statement_source = &source[statement_start..usize::from(statement_range.end())];
    let relative_collection_point =
        TextSize::try_from(usize::from(prefix.range.start()).saturating_sub(statement_start))
            .expect("relative completion point fits TextSize");
    let completion_lex = pg_parser::lex_for_completion(statement_source, relative_collection_point);
    match &completion_lex {
        Ok(output) => {
            for issue in &output.issues {
                recovery.issues.push(RecoveryIssue {
                    kind: RecoveryKind::UnterminatedToken,
                    range: absolute_lex_range(statement_start, issue.range),
                });
            }
            if !output.issues.is_empty() {
                recovery.issues.push(RecoveryIssue {
                    kind: RecoveryKind::ScopeIncomplete,
                    range: absolute_lex_range(statement_start, output.issues[0].range),
                });
            }
            if let Some(range) = scope::incomplete_range(statement_range.start(), &output.tokens)
                && !recovery.issues.iter().any(|issue| {
                    issue.kind == RecoveryKind::ScopeIncomplete && issue.range == range
                })
            {
                recovery.issues.push(RecoveryIssue {
                    kind: RecoveryKind::ScopeIncomplete,
                    range,
                });
            }
        }
        Err(error) => recovery.issues.push(RecoveryIssue {
            kind: RecoveryKind::LexErrorBeforePoint,
            range: absolute_lex_range(statement_start, error.range),
        }),
    }
    let expectation_result = if prefix.suppress_expectations {
        Ok(pg_parser::ParserExpectations::default())
    } else {
        collect_expectations(statement_source, relative_collection_point)
    };
    let mut expectations = match expectation_result {
        Ok(expectations) => ExpectationSet {
            tokens: expectations.tokens,
            phrases: expectations.phrases,
            slots: expectations.slots,
        },
        Err(_) => ExpectationSet::default(),
    };
    filter_token_prefix(&mut expectations, &prefix.prefix);
    let relative_point = TextSize::try_from(
        usize::from(normalized.point).saturating_sub(usize::from(statement_range.start())),
    )
    .expect("relative point fits TextSize");
    let scope = match &completion_lex {
        Ok(output) => scope::collect_tokens(
            statement_source,
            statement_range.start(),
            relative_point,
            &output.tokens,
        ),
        Err(_) => ScopeSnapshot::default(),
    };
    narrow_qualified_slots(&mut expectations, &prefix.qualifier, &scope);
    let mut intent = intent::from_expectations(&expectations);
    intent.qualifier = prefix.qualifier;
    if let Ok(output) = &completion_lex {
        intent::attach_container(
            &mut intent,
            statement_source,
            statement_range.start(),
            relative_collection_point,
            &output.tokens,
        );
    }

    CompletionContext {
        statement_range,
        point: normalized.point,
        replacement_range: prefix.range,
        prefix: prefix.prefix,
        expectations,
        intent,
        scope,
        recovery,
    }
}

fn absolute_lex_range(base: usize, range: TextRange) -> TextRange {
    TextRange::new(
        TextSize::try_from(base + usize::from(range.start()))
            .expect("lexical range start belongs to source"),
        TextSize::try_from(base + usize::from(range.end()))
            .expect("lexical range end belongs to source"),
    )
}

fn filter_token_prefix(expectations: &mut ExpectationSet, prefix: &CompletionPrefix) {
    if prefix.quoting != IdentifierQuoting::Unquoted {
        expectations.tokens.clear();
        expectations.phrases.clear();
        return;
    }
    if prefix.normalized.is_empty() {
        return;
    }
    expectations.tokens.retain(|kind| {
        token_spelling(*kind).is_some_and(|candidate| candidate.starts_with(&prefix.normalized))
    });
    expectations.phrases.retain(|phrase| {
        token_spelling(phrase[0]).is_some_and(|head| head.starts_with(&prefix.normalized))
    });
}

fn token_spelling(kind: TokenKind) -> Option<String> {
    if let TokenKind::Char(ch) = kind {
        return Some(ch.to_string());
    }
    if let Some(keyword) = KEYWORDS.iter().find(|keyword| keyword.kind == kind) {
        return Some(keyword.word.to_owned());
    }
    match kind {
        TokenKind::TypeCast => Some("::".to_owned()),
        TokenKind::DotDot => Some("..".to_owned()),
        TokenKind::ColonEquals => Some(":=".to_owned()),
        TokenKind::EqualsGreater => Some("=>".to_owned()),
        TokenKind::LessEquals => Some("<=".to_owned()),
        TokenKind::GreaterEquals => Some(">=".to_owned()),
        TokenKind::NotEquals => Some("<>".to_owned()),
        TokenKind::RightArrow => Some("->".to_owned()),
        _ => None,
    }
}

fn narrow_qualified_slots(
    expectations: &mut ExpectationSet,
    qualifier: &[NamePart],
    scope: &ScopeSnapshot,
) {
    if !expectations.slots.contains(&GrammarSlot::Column)
        || !expectations.slots.contains(&GrammarSlot::Function)
    {
        return;
    }
    let Some(first) = qualifier.first() else {
        return;
    };
    let visible = scope
        .local
        .relations
        .iter()
        .chain(scope.outer.iter().flat_map(|outer| &outer.relations))
        .chain(scope.dml_target.iter())
        .chain(scope.merge_source.iter())
        .any(|relation| {
            relation
                .alias
                .as_ref()
                .is_some_and(|alias| alias.normalized == first.normalized)
                || relation
                    .name
                    .last()
                    .is_some_and(|name| name.normalized == first.normalized)
        })
        || scope
            .ctes
            .iter()
            .any(|cte| cte.name.normalized == first.normalized);
    if visible {
        expectations
            .slots
            .retain(|slot| *slot != GrammarSlot::Function);
    }
}
