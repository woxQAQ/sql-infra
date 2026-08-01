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
    pub diagnostics: Vec<CompletionDiagnostic>,
}

impl CompletionContext {
    /// Returns syntax items worth presenting in an editor.
    ///
    /// Raw grammar expectations remain available on `expectations`. This
    /// projection removes punctuation and symbolic operators, and defers
    /// expression continuations and enclosing follows until the user starts
    /// typing the next token. Callers therefore do not need statement- or
    /// clause-specific suppression rules.
    pub fn syntax_completions(&self) -> Vec<SyntaxCompletion> {
        let mut completions = Vec::new();
        for kind in &self.expectations.tokens {
            let Some(label) = keyword_spelling(*kind) else {
                continue;
            };
            if self.prefix.raw.is_empty() && !self.expectations.eager_without_prefix(*kind) {
                continue;
            }
            completions.push(SyntaxCompletion {
                insert_text: label.clone(),
                label,
                kind: SyntaxCompletionKind::Keyword,
                is_follow: self.expectations.follow_tokens.contains(kind),
            });
        }
        for phrase in &self.expectations.phrases {
            let labels = phrase
                .iter()
                .filter_map(|kind| keyword_spelling(*kind))
                .collect::<Vec<_>>();
            if labels.len() != phrase.len()
                || (self.prefix.raw.is_empty()
                    && !self.expectations.eager_without_prefix(phrase[0]))
            {
                continue;
            }
            let label = labels.join(" ");
            completions.push(SyntaxCompletion {
                insert_text: label.clone(),
                label,
                kind: SyntaxCompletionKind::Phrase,
                is_follow: self.expectations.follow_tokens.contains(&phrase[0]),
            });
        }
        completions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxCompletionKind {
    Keyword,
    Phrase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxCompletion {
    pub label: String,
    pub insert_text: String,
    pub kind: SyntaxCompletionKind,
    pub is_follow: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpectationSet {
    pub tokens: Vec<TokenKind>,
    /// Tokens introduced directly by the active grammar production.
    pub direct_tokens: Vec<TokenKind>,
    /// Keyword alternatives observed through parser lookahead predicates.
    /// They remain quiet without a prefix.
    pub lookahead_tokens: Vec<TokenKind>,
    /// Tokens that can start the active expression.
    pub expression_start_tokens: Vec<TokenKind>,
    /// Tokens that extend the already parsed expression.
    pub expression_continuation_tokens: Vec<TokenKind>,
    /// Tokens that leave the active expression for its enclosing production.
    /// This is a subset of `tokens`; callers can rank expression continuations
    /// ahead of clause transitions without reconstructing parser state.
    pub follow_tokens: Vec<TokenKind>,
    /// Fixed multi-token units that are grammatical at the point, e.g.
    /// `GROUP BY` or `IF NOT EXISTS`. Each phrase's head token also appears
    /// in `tokens`; a phrase does not claim the head has no other
    /// continuation.
    pub phrases: Vec<&'static [TokenKind]>,
    pub slots: Vec<GrammarSlot>,
    pub membership: Option<pg_parser::GrammarMembership>,
}

impl ExpectationSet {
    fn eager_without_prefix(&self, kind: TokenKind) -> bool {
        self.direct_tokens.contains(&kind)
            || (kind != TokenKind::Operator && self.expression_start_tokens.contains(&kind))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionIntent {
    pub object_kinds: Vec<ObjectKind>,
    pub qualifier: Vec<NamePart>,
    pub membership: Option<CatalogMembership>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogMembership {
    pub member_kinds: Vec<ObjectKind>,
    pub owner: ObjectReference,
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
    /// The relation can only be referenced through its alias; its columns do
    /// not participate in unqualified column lookup.
    pub qualified_only: bool,
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

#[derive(Clone, Debug, Eq, PartialEq)]
/// A condition that affected completion collection without preventing a
/// context from being returned.
pub struct CompletionDiagnostic {
    pub kind: CompletionDiagnosticKind,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// The kind of adjustment, recovery, or incomplete analysis reported while
/// collecting completion context.
pub enum CompletionDiagnosticKind {
    PointClampedToEof,
    PointMovedToCharBoundary,
    TokenizationRecovered,
    LexErrorBeforePoint,
    ScopeIncomplete,
}

/// Collect syntax and scope information for completion at a UTF-8 byte point.
///
/// The requested point is clamped and normalized before any slicing occurs.
pub fn collect(source: &str, point: TextSize) -> CompletionContext {
    let normalized = prefix::normalize_point(source, point);
    let statement_range = statement::range_at(source, normalized.point);
    let site = prefix::analyze(source, statement_range, normalized.point);
    let mut diagnostics = normalized.diagnostics;

    let statement_start = usize::from(statement_range.start());
    let statement_text = &source[statement_start..usize::from(statement_range.end())];
    let completion_start = TextSize::try_from(
        usize::from(site.replacement_range.start()).saturating_sub(statement_start),
    )
    .expect("completion start fits TextSize");
    let tokenization_result = pg_parser::lex_for_completion(statement_text, completion_start);
    match &tokenization_result {
        Ok(tokenization) => {
            if let Some(error) = tokenization.recovered_error() {
                let range = absolute_lex_range(statement_start, error.range);
                diagnostics.push(CompletionDiagnostic {
                    kind: CompletionDiagnosticKind::TokenizationRecovered,
                    range,
                });
                diagnostics.push(CompletionDiagnostic {
                    kind: CompletionDiagnosticKind::ScopeIncomplete,
                    range,
                });
            }
            if let Some(range) =
                scope::incomplete_range(statement_range.start(), tokenization.tokens())
                && !diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind == CompletionDiagnosticKind::ScopeIncomplete
                        && diagnostic.range == range
                })
            {
                diagnostics.push(CompletionDiagnostic {
                    kind: CompletionDiagnosticKind::ScopeIncomplete,
                    range,
                });
            }
        }
        Err(error) => diagnostics.push(CompletionDiagnostic {
            kind: CompletionDiagnosticKind::LexErrorBeforePoint,
            range: absolute_lex_range(statement_start, error.range),
        }),
    }
    let expectation_result = if site.supports_grammar_completion() {
        collect_expectations(statement_text, completion_start)
    } else {
        Ok(pg_parser::ParserExpectations::default())
    };
    let mut expectations = match expectation_result {
        Ok(expectations) => ExpectationSet {
            tokens: expectations.tokens,
            direct_tokens: expectations.direct_tokens,
            lookahead_tokens: expectations.lookahead_tokens,
            expression_start_tokens: expectations.expression_start_tokens,
            expression_continuation_tokens: expectations.expression_continuation_tokens,
            follow_tokens: expectations.follow_tokens,
            phrases: expectations.phrases,
            slots: expectations.slots,
            membership: expectations.membership,
        },
        Err(_) => ExpectationSet::default(),
    };
    filter_token_prefix(&mut expectations, &site.prefix);
    let point_in_statement = TextSize::try_from(
        usize::from(normalized.point).saturating_sub(usize::from(statement_range.start())),
    )
    .expect("point in statement fits TextSize");
    let scope = match &tokenization_result {
        Ok(tokenization) => scope::collect_tokens(
            statement_text,
            statement_range.start(),
            point_in_statement,
            tokenization.tokens(),
        ),
        Err(_) => ScopeSnapshot::default(),
    };
    let mut intent =
        intent::from_expectations(&expectations, statement_text, statement_range.start());
    intent.qualifier = site.qualifier;

    CompletionContext {
        statement_range,
        point: normalized.point,
        replacement_range: site.replacement_range,
        prefix: site.prefix,
        expectations,
        intent,
        scope,
        diagnostics,
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
        expectations.direct_tokens.clear();
        expectations.lookahead_tokens.clear();
        expectations.expression_start_tokens.clear();
        expectations.expression_continuation_tokens.clear();
        expectations.follow_tokens.clear();
        expectations.phrases.clear();
        return;
    }
    if prefix.normalized.is_empty() {
        return;
    }
    let matching_syntax = expectations
        .tokens
        .iter()
        .filter(|kind| {
            token_spelling(**kind)
                .is_some_and(|candidate| candidate.starts_with(&prefix.normalized))
        })
        .take(2)
        .count();
    let exact_syntax = expectations
        .tokens
        .iter()
        .any(|kind| token_spelling(*kind).is_some_and(|candidate| candidate == prefix.normalized));
    if expectations.slots.contains(&GrammarSlot::Alias) && matching_syntax != 1 && !exact_syntax {
        expectations.tokens.clear();
        expectations.direct_tokens.clear();
        expectations.lookahead_tokens.clear();
        expectations.expression_start_tokens.clear();
        expectations.expression_continuation_tokens.clear();
        expectations.follow_tokens.clear();
        expectations.phrases.clear();
        return;
    }
    expectations.tokens.retain(|kind| {
        token_spelling(*kind).is_some_and(|candidate| candidate.starts_with(&prefix.normalized))
    });
    expectations
        .direct_tokens
        .retain(|kind| expectations.tokens.contains(kind));
    expectations
        .lookahead_tokens
        .retain(|kind| expectations.tokens.contains(kind));
    expectations
        .expression_start_tokens
        .retain(|kind| expectations.tokens.contains(kind));
    expectations
        .expression_continuation_tokens
        .retain(|kind| expectations.tokens.contains(kind));
    expectations
        .follow_tokens
        .retain(|kind| expectations.tokens.contains(kind));
    expectations.phrases.retain(|phrase| {
        token_spelling(phrase[0]).is_some_and(|head| head.starts_with(&prefix.normalized))
    });
}

fn keyword_spelling(kind: TokenKind) -> Option<String> {
    KEYWORDS
        .iter()
        .find(|keyword| keyword.kind == kind)
        .map(|keyword| keyword.word.to_ascii_uppercase())
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
