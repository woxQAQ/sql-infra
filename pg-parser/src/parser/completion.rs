//! Parser-native grammar expectation collection at an editing point.
//!
//! A synthetic completion marker runs through the normal grammar. The collector
//! records tokens, phrases, catalog slots, membership, and provenance without
//! changing strict parsing behavior.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

/// A named identifier or Catalog-object position accepted by the grammar.
///
/// Slots describe syntax only: they do not assert that an object exists, is
/// visible, or has already been resolved in a Catalog. More than one slot can
/// be valid at the same completion point.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GrammarSlot {
    /// A relation-like object accepted by a general relation production.
    Relation,
    /// A table name.
    Table,
    /// A view name.
    View,
    /// A materialized-view name.
    MaterializedView,
    /// A foreign-table name.
    ForeignTable,
    /// A column name.
    Column,
    /// An attribute of a composite type.
    Attribute,
    /// A function name.
    Function,
    /// A procedure name.
    Procedure,
    /// A routine name where either a function or procedure is accepted.
    Routine,
    /// An aggregate name.
    Aggregate,
    /// A data-type name.
    Type,
    /// A domain name.
    Domain,
    /// A schema name.
    Schema,
    /// A sequence name.
    Sequence,
    /// An index name.
    Index,
    /// A table or domain constraint name.
    Constraint,
    /// A collation name.
    Collation,
    /// An operator name.
    Operator,
    /// An operator-class name.
    OperatorClass,
    /// An operator-family name.
    OperatorFamily,
    /// A role or user name.
    Role,
    /// A database name.
    Database,
    /// An index or table access-method name.
    AccessMethod,
    /// A character-set conversion name.
    Conversion,
    /// An event-trigger name.
    EventTrigger,
    /// An extension name.
    Extension,
    /// A foreign-data-wrapper name.
    ForeignDataWrapper,
    /// A foreign-server name.
    ForeignServer,
    /// A procedural-language name.
    Language,
    /// A row-level security policy name.
    Policy,
    /// A property-graph name.
    PropertyGraph,
    /// A publication name.
    Publication,
    /// A rewrite-rule name.
    Rule,
    /// An extended-statistics object name.
    Statistics,
    /// A subscription name.
    Subscription,
    /// A tablespace name.
    Tablespace,
    /// A text-search configuration name.
    TextSearchConfiguration,
    /// A text-search dictionary name.
    TextSearchDictionary,
    /// A text-search parser name.
    TextSearchParser,
    /// A text-search template name.
    TextSearchTemplate,
    /// A data-change trigger name.
    Trigger,
    /// A privilege keyword or adapter-provided privilege name.
    Privilege,
    /// A SQL alias introduced by the statement being edited.
    Alias,
    /// A name whose more specific object category is not encoded by the grammar.
    AnyName,
}

/// Maps a PostgreSQL AST object category to its closest grammar slot.
///
/// Object categories that share a completion namespace collapse to one slot.
/// Categories without a more specific slot map to [`GrammarSlot::AnyName`].
pub const fn object_type_slot(object_type: ObjectType) -> GrammarSlot {
    match object_type {
        ObjectType::Table => GrammarSlot::Table,
        ObjectType::View => GrammarSlot::View,
        ObjectType::Matview => GrammarSlot::MaterializedView,
        ObjectType::ForeignTable => GrammarSlot::ForeignTable,
        ObjectType::Column => GrammarSlot::Column,
        ObjectType::Attribute => GrammarSlot::Attribute,
        ObjectType::Function => GrammarSlot::Function,
        ObjectType::Procedure => GrammarSlot::Procedure,
        ObjectType::Routine => GrammarSlot::Routine,
        ObjectType::Aggregate => GrammarSlot::Aggregate,
        ObjectType::Type => GrammarSlot::Type,
        ObjectType::Domain => GrammarSlot::Domain,
        ObjectType::Sequence => GrammarSlot::Sequence,
        ObjectType::Index => GrammarSlot::Index,
        ObjectType::Domconstraint | ObjectType::Tabconstraint => GrammarSlot::Constraint,
        ObjectType::Collation => GrammarSlot::Collation,
        ObjectType::Operator => GrammarSlot::Operator,
        ObjectType::Opclass => GrammarSlot::OperatorClass,
        ObjectType::Opfamily => GrammarSlot::OperatorFamily,
        ObjectType::Schema => GrammarSlot::Schema,
        ObjectType::Role => GrammarSlot::Role,
        ObjectType::Database => GrammarSlot::Database,
        ObjectType::AccessMethod => GrammarSlot::AccessMethod,
        ObjectType::Conversion => GrammarSlot::Conversion,
        ObjectType::EventTrigger => GrammarSlot::EventTrigger,
        ObjectType::Extension => GrammarSlot::Extension,
        ObjectType::Fdw => GrammarSlot::ForeignDataWrapper,
        ObjectType::ForeignServer => GrammarSlot::ForeignServer,
        ObjectType::Language => GrammarSlot::Language,
        ObjectType::Policy => GrammarSlot::Policy,
        ObjectType::Propgraph => GrammarSlot::PropertyGraph,
        ObjectType::Publication | ObjectType::PublicationNamespace | ObjectType::PublicationRel => {
            GrammarSlot::Publication
        }
        ObjectType::Rule => GrammarSlot::Rule,
        ObjectType::StatisticExt => GrammarSlot::Statistics,
        ObjectType::Subscription => GrammarSlot::Subscription,
        ObjectType::Tablespace => GrammarSlot::Tablespace,
        ObjectType::Tsconfiguration => GrammarSlot::TextSearchConfiguration,
        ObjectType::Tsdictionary => GrammarSlot::TextSearchDictionary,
        ObjectType::Tsparser => GrammarSlot::TextSearchParser,
        ObjectType::Tstemplate => GrammarSlot::TextSearchTemplate,
        ObjectType::Trigger => GrammarSlot::Trigger,
        ObjectType::Amop
        | ObjectType::Amproc
        | ObjectType::Cast
        | ObjectType::Default
        | ObjectType::Defacl
        | ObjectType::Largeobject
        | ObjectType::ParameterAcl
        | ObjectType::Transform
        | ObjectType::UserMapping => GrammarSlot::AnyName,
    }
}

/// A grammar-level reference to the Catalog object whose members are being
/// completed. Name tokens retain their source ranges and quoting information
/// for the completion layer to project without reparsing statement syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrammarObjectReference {
    /// The syntactically possible object categories for the unresolved owner.
    pub object_types: Vec<ObjectType>,
    /// Name-component tokens in source order, with separating punctuation
    /// omitted. Their source ranges preserve the original spelling and quoting.
    pub name: Vec<Token>,
}

/// A grammar-level member/owner relation at the completion point.
///
/// For example, completing a column in `GRANT SELECT (col) ON table` produces
/// a column member slot owned by the unresolved `table` reference. This value
/// records only that syntactic relationship; it does not perform name or
/// Catalog resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrammarMembership {
    /// Member categories accepted at the completion point.
    pub member_slots: Vec<GrammarSlot>,
    /// The unresolved object whose members are being completed.
    pub owner: GrammarObjectReference,
}

pub(super) fn definition_value_slot(object_type: ObjectType, name: &str) -> Option<GrammarSlot> {
    match (object_type, name) {
        (ObjectType::Operator, "function" | "procedure" | "restrict" | "join" | "joins") => {
            Some(GrammarSlot::Function)
        }
        (ObjectType::Operator, "leftarg" | "rightarg") => Some(GrammarSlot::Type),
        (ObjectType::Operator, "commutator" | "negator") => Some(GrammarSlot::Operator),
        (
            ObjectType::Aggregate,
            "sfunc" | "finalfunc" | "combinefunc" | "serialfunc" | "deserialfunc" | "msfunc"
            | "minvfunc" | "mfinalfunc",
        ) => Some(GrammarSlot::Function),
        (ObjectType::Aggregate, "stype" | "mstype") => Some(GrammarSlot::Type),
        (ObjectType::Aggregate, "sortop") => Some(GrammarSlot::Operator),
        (
            ObjectType::Type,
            "input" | "output" | "receive" | "send" | "typmod_in" | "typmod_out" | "analyze"
            | "subscript",
        ) => Some(GrammarSlot::Function),
        (ObjectType::Type, "element") => Some(GrammarSlot::Type),
        (ObjectType::Type, "collation") => Some(GrammarSlot::Collation),
        (ObjectType::Tsconfiguration, "parser") => Some(GrammarSlot::TextSearchParser),
        (ObjectType::Tsconfiguration, "copy") => Some(GrammarSlot::TextSearchConfiguration),
        (ObjectType::Tsdictionary, "template") => Some(GrammarSlot::TextSearchTemplate),
        (ObjectType::Tsparser, "start" | "gettoken" | "end" | "lextypes" | "headline") => {
            Some(GrammarSlot::Function)
        }
        (ObjectType::Tstemplate, "init" | "lexize") => Some(GrammarSlot::Function),
        _ => None,
    }
}

/// The fixed phrase a clause-boundary token begins when it appears as an
/// expression follow token. These keywords open exactly one multi-word unit
/// in the grammar wherever an expression can end.
pub(super) const fn follow_phrase(kind: TokenKind) -> Option<&'static [TokenKind]> {
    match kind {
        TokenKind::GroupP => Some(&[TokenKind::GroupP, TokenKind::By]),
        TokenKind::Order => Some(&[TokenKind::Order, TokenKind::By]),
        TokenKind::Partition => Some(&[TokenKind::Partition, TokenKind::By]),
        TokenKind::Within => Some(&[TokenKind::Within, TokenKind::GroupP]),
        _ => None,
    }
}

/// Statement starters that are legal where an expression production admits
/// a parenthesized subquery. Keep this separate from the top-level statement
/// dispatcher: utility statements are never valid in these positions.
pub(super) const SUBQUERY_START_TOKENS: &[TokenKind] = &[
    TokenKind::Select,
    TokenKind::With,
    TokenKind::Values,
    TokenKind::Table,
];

/// Grammar expectations collected at one completion point.
///
/// `tokens` is the union of the five token-provenance collections. Provenance
/// collections may overlap, and every collection preserves first-observed
/// grammar order while suppressing duplicates. Named positions are reported
/// separately through `slots` because they require an adapter or Catalog to
/// turn them into concrete completion items.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParserExpectations {
    /// All concrete token kinds accepted at the completion point.
    ///
    /// This can include punctuation and operators as well as keywords. Parser
    /// sentinels, end of input, and statement-separating semicolons are omitted.
    pub tokens: Vec<TokenKind>,
    /// Tokens introduced directly by the active grammar production. This is a
    /// subset of [`Self::tokens`].
    pub direct_tokens: Vec<TokenKind>,
    /// Keyword alternatives observed through parser lookahead predicates.
    /// These are syntactically reachable but are not eager editor items until
    /// the user starts typing a prefix. This is a subset of [`Self::tokens`].
    pub lookahead_tokens: Vec<TokenKind>,
    /// Tokens that can start the active expression. This is a subset of
    /// [`Self::tokens`].
    pub expression_start_tokens: Vec<TokenKind>,
    /// Tokens that extend the already parsed expression. This is a subset of
    /// [`Self::tokens`].
    pub expression_continuation_tokens: Vec<TokenKind>,
    /// Tokens that end the active expression and continue in its enclosing
    /// production. This is a subset of `tokens`; the remaining tokens extend
    /// the expression itself.
    pub follow_tokens: Vec<TokenKind>,
    /// Fixed multi-token units that are grammatical at the point, e.g.
    /// `GROUP BY` or `IF NOT EXISTS`. Each phrase's head token also appears
    /// in `tokens`; a phrase does not claim the head has no other
    /// continuation.
    pub phrases: Vec<&'static [TokenKind]>,
    /// Named grammar positions accepted at the completion point.
    ///
    /// Specific slots supersede [`GrammarSlot::AnyName`] when both are
    /// discovered.
    pub slots: Vec<GrammarSlot>,
    /// The syntactic owner of a member slot, when the grammar identifies one.
    ///
    /// This may be `None` even when `slots` contains a member category: not
    /// every production names or successfully parses an owner.
    pub membership: Option<GrammarMembership>,
}

#[derive(Debug, Default)]
pub(super) struct CompletionCollector {
    expectations: ParserExpectations,
    allows_hole_recovery: bool,
    recovered_any_hole: bool,
    active_membership_owners: Vec<(Vec<GrammarSlot>, GrammarObjectReference)>,
    membership_recovery_requested: bool,
}

pub(super) type SharedCollector = Rc<RefCell<CompletionCollector>>;

#[derive(Clone, Copy)]
/// Controls how the lexer token at the completion point is treated before the
/// synthetic marker is inserted.
///
/// Initial collection removes the token as an editor prefix. Membership
/// recovery may need complete punctuation or numeric tokens at the same offset
/// as owner syntax following the recovered name hole, so it preserves them.
enum CompletionPointTokenPolicy {
    InitialCollection,
    MembershipRecovery,
}

impl CompletionPointTokenPolicy {
    fn should_remove_token(self, token: &Token, completion_point: TextSize) -> bool {
        let intersects_completion_point = token.kind != TokenKind::Eof
            && token.range.start() <= completion_point
            && (completion_point < token.range.end()
                || (token.kind == TokenKind::Incomplete && completion_point == token.range.end()));
        if !intersects_completion_point {
            return false;
        }

        match self {
            Self::InitialCollection => true,
            // Names and incomplete tokens remain editor prefixes during
            // recovery and therefore still get replaced.
            Self::MembershipRecovery => {
                token.range.start() < completion_point
                    || matches!(
                        &token.value,
                        Some(TokenValue::String(_) | TokenValue::Keyword(_))
                    )
                    || token.kind == TokenKind::Incomplete
            }
        }
    }
}

impl CompletionCollector {
    pub(super) fn record_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.direct_tokens, kinds);
    }

    pub(super) fn record_phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.record_tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    pub(super) fn record_lookahead_tokens(&mut self, kinds: &[TokenKind]) {
        let keywords = kinds
            .iter()
            .copied()
            .filter(|kind| crate::KEYWORDS.iter().any(|keyword| keyword.kind == *kind))
            .collect::<Vec<_>>();
        Self::insert_tokens(&mut self.expectations.tokens, &keywords);
        Self::insert_tokens(&mut self.expectations.lookahead_tokens, &keywords);
    }

    pub(super) fn record_expression_start_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.expression_start_tokens, kinds);
    }

    pub(super) fn record_expression_continuation_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.expression_continuation_tokens, kinds);
    }

    pub(super) fn record_expression_continuation_phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.record_expression_continuation_tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    pub(super) fn record_follow_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.follow_tokens, kinds);
    }

    pub(super) fn record_follow_phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.record_follow_tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    fn insert_tokens(target: &mut Vec<TokenKind>, kinds: &[TokenKind]) {
        for kind in kinds {
            if matches!(
                kind,
                TokenKind::Eof | TokenKind::Completion | TokenKind::Char(';')
            ) || target.contains(kind)
            {
                continue;
            }
            target.push(*kind);
        }
    }

    pub(super) fn record_slot(&mut self, slot: GrammarSlot) {
        if slot == GrammarSlot::AnyName && !self.expectations.slots.is_empty() {
            return;
        }
        if slot != GrammarSlot::AnyName {
            self.expectations
                .slots
                .retain(|candidate| *candidate != GrammarSlot::AnyName);
        }
        if !self.expectations.slots.contains(&slot) {
            self.expectations.slots.push(slot);
        }
        if let Some((member_slots, owner)) = self
            .active_membership_owners
            .iter()
            .rev()
            .find(|(member_slots, _)| member_slots.contains(&slot))
            .cloned()
        {
            self.attach_membership_owner(member_slots, owner);
        }
    }

    pub(super) fn push_membership_owner(
        &mut self,
        member_slots: &[GrammarSlot],
        owner: GrammarObjectReference,
    ) {
        self.active_membership_owners
            .push((member_slots.to_vec(), owner.clone()));
        let matches_recorded_slot = member_slots
            .iter()
            .any(|member| self.expectations.slots.contains(member));
        if self.recovered_any_hole || matches_recorded_slot {
            self.attach_membership_owner(member_slots.to_vec(), owner);
        }
    }

    pub(super) fn pop_membership_owner(&mut self) {
        self.active_membership_owners.pop();
    }

    fn clear_membership_owners(&mut self) {
        self.active_membership_owners.clear();
    }

    pub(super) fn request_membership_recovery(&mut self) {
        self.membership_recovery_requested = true;
    }

    pub(super) fn try_recover_hole(&mut self) -> bool {
        if !self.allows_hole_recovery {
            return false;
        }
        self.recovered_any_hole = true;
        true
    }

    fn attach_membership_owner(
        &mut self,
        candidate_slots: Vec<GrammarSlot>,
        owner: GrammarObjectReference,
    ) {
        let matching_slots = candidate_slots
            .into_iter()
            .filter(|member| self.expectations.slots.contains(member))
            .collect::<Vec<_>>();
        if matching_slots.is_empty() || self.expectations.membership.is_some() {
            return;
        }
        self.expectations.membership = Some(GrammarMembership {
            member_slots: matching_slots,
            owner,
        });
    }
}

fn tokens_with_completion_marker(
    source: &str,
    completion_point: TextSize,
    token_policy: CompletionPointTokenPolicy,
) -> Result<Vec<Token>, crate::lexer::LexError> {
    let mut tokens = crate::lexer::lex_for_completion(source, completion_point)?.into_tokens();
    if let Some(prefix_token_index) = tokens
        .iter()
        .position(|token| token_policy.should_remove_token(token, completion_point))
    {
        tokens.remove(prefix_token_index);
    }

    let marker_index = tokens
        .iter()
        .position(|token| token.range.start() >= completion_point)
        .unwrap_or_else(|| tokens.len().saturating_sub(1));
    tokens.insert(
        marker_index,
        Token::synthetic(TokenKind::Completion, usize::from(completion_point)),
    );
    Ok(tokens)
}

fn run_completion_parse(tokens: Vec<Token>, collector: &SharedCollector) {
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion: Some(Rc::clone(collector)),
    };
    // Expectations collected before a syntax exit remain useful at the
    // editing point, so completion intentionally ignores the parse outcome.
    let _parse_outcome = parser.parse_statements_with_ranges();
}

// ── Parser completion hooks ───────────────────────────────────────────────

impl Parser {
    /// Statement-scoped membership owners intentionally remain active through
    /// the rest of their statement. Starting a new statement ends that scope.
    pub(super) fn clear_completion_membership_owners(&self) {
        if let Some(collector) = &self.completion {
            collector.borrow_mut().clear_membership_owners();
        }
    }

    /// Preserve the synthetic completion marker when a deferred fragment is
    /// handed to another parser. The outer parser keeps its cursor at the
    /// marker; the nested parser receives a clone and shares the collector.
    pub(super) fn append_completion_marker(&self, tokens: &mut Vec<Token>) {
        if self.at_completion() {
            tokens.push(self.peek().clone());
        }
    }

    pub(super) fn record_completion_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_tokens(kinds);
        }
    }

    pub(super) fn record_completion_lookahead_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_lookahead_tokens(kinds);
        }
    }

    pub(super) fn record_completion_follow_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_follow_tokens(kinds);
        }
    }

    pub(super) fn record_completion_follow_phrase(&self, phrase: &'static [TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_follow_phrase(phrase);
        }
    }

    pub(super) fn record_completion_phrase(&self, phrase: &'static [TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_phrase(phrase);
        }
    }

    pub(super) fn record_completion_slot(&self, slot: GrammarSlot) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_slot(slot);
        }
    }

    /// Record a slot when the completion point follows a top-level `.` inside
    /// the current name fragment. The ordinary slot hook covers the fragment's
    /// first component; this hook covers later components of a qualified name.
    pub(super) fn record_completion_qualified_name_slot(
        &self,
        slot: GrammarSlot,
        fragment_end_tokens: &[TokenKind],
    ) {
        if self.top_level_token_before_completion(fragment_end_tokens) != Some(TokenKind::Char('.'))
        {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_slot(slot);
        }
    }

    /// Publish a slot when the completion marker is anywhere inside the
    /// fragment delimited by top-level `fragment_end_tokens`, not only at its
    /// first token.
    pub(super) fn record_completion_slot_within_fragment(
        &self,
        slot: GrammarSlot,
        fragment_end_tokens: &[TokenKind],
    ) {
        let follows_fragment_separator = matches!(
            self.top_level_token_before_completion(fragment_end_tokens),
            Some(TokenKind::Char('.') | TokenKind::Char(',') | TokenKind::Char('('))
        );
        if !self.at_completion() && !follows_fragment_separator {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().record_slot(slot);
        }
    }

    /// Mark a member position whose owner appears later in the production.
    /// The completion collector will run a second, hole-recovering parse only
    /// when the first pass actually reaches this point.
    pub(super) fn request_completion_membership_recovery(&self) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().request_membership_recovery();
        }
    }

    pub(super) fn push_completion_membership_owner_from_tokens(
        &self,
        member_slots: &[GrammarSlot],
        object_types: &[ObjectType],
        start_token_index: usize,
        end_token_index: usize,
    ) {
        self.push_completion_membership_owner_name(
            member_slots,
            object_types,
            self.membership_owner_name_tokens(start_token_index, end_token_index),
        );
    }

    pub(super) fn push_completion_membership_owner_name(
        &self,
        member_slots: &[GrammarSlot],
        object_types: &[ObjectType],
        name: Vec<Token>,
    ) {
        if name.is_empty() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().push_membership_owner(
                member_slots,
                GrammarObjectReference {
                    object_types: object_types.to_vec(),
                    name,
                },
            );
        }
    }

    pub(super) fn pop_completion_membership_owner(&self) {
        if let Some(collector) = &self.completion {
            collector.borrow_mut().pop_membership_owner();
        }
    }

    pub(super) fn recover_completion_hole(&mut self) -> Option<Token> {
        if !self.at_completion() {
            return None;
        }
        let hole_recovered = self
            .completion
            .as_ref()
            .is_some_and(|collector| collector.borrow_mut().try_recover_hole());
        if !hole_recovered {
            return None;
        }
        let location = self.peek().location();
        self.pos += 1;
        Some(Token::completion_hole(location))
    }

    fn membership_owner_name_tokens(
        &self,
        start_token_index: usize,
        end_token_index: usize,
    ) -> Vec<Token> {
        self.tokens[start_token_index..end_token_index]
            .iter()
            .filter(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Char('.')
                        | TokenKind::Char('(')
                        | TokenKind::Char(')')
                        | TokenKind::Char('*')
                        | TokenKind::Only
                )
            })
            .cloned()
            .collect()
    }

    fn top_level_token_before_completion(
        &self,
        fragment_end_tokens: &[TokenKind],
    ) -> Option<TokenKind> {
        let mut delimiter_depth = 0usize;
        let mut previous_top_level_kind = None;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => delimiter_depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    delimiter_depth = delimiter_depth.saturating_sub(1);
                }
                TokenKind::Completion if delimiter_depth == 0 => {
                    return previous_top_level_kind;
                }
                kind if delimiter_depth == 0 && fragment_end_tokens.contains(&kind) => return None,
                kind if delimiter_depth == 0 => previous_top_level_kind = Some(kind),
                _ => {}
            }
        }
        None
    }
}

/// Collects parser-native grammar expectations at a UTF-8 byte offset.
///
/// A token intersecting the point is treated as the editor prefix and removed
/// from the parser input before a synthetic completion marker is parsed.
/// Callers normally pass the start of the editor's replacement range rather
/// than the visual caret position so a partially typed identifier or keyword
/// does not affect the surrounding grammar.
///
/// `completion_point` is clamped to `source.len()`. Callers should normalize it
/// to a UTF-8 character boundary before calling this function. Syntax errors at
/// or after the marker can still yield partial expectations; only unrecoverable
/// lexical errors are returned.
///
/// # Errors
///
/// Returns [`crate::lexer::LexError`] when malformed input wholly before the
/// completion point prevents reliable parsing, or when the source is too large
/// for [`TextSize`].
pub fn collect_expectations(
    source: &str,
    completion_point: TextSize,
) -> Result<ParserExpectations, crate::lexer::LexError> {
    let completion_point = TextSize::try_from(usize::from(completion_point).min(source.len()))
        .expect("completion point was bounded by source length");
    let initial_tokens = tokens_with_completion_marker(
        source,
        completion_point,
        CompletionPointTokenPolicy::InitialCollection,
    )?;
    let initial_collector = Rc::new(RefCell::new(CompletionCollector::default()));
    run_completion_parse(initial_tokens, &initial_collector);

    let (initial_expectations, membership_recovery_requested) = {
        let collector = initial_collector.borrow();
        (
            collector.expectations.clone(),
            collector.membership_recovery_requested,
        )
    };
    if initial_expectations.membership.is_some() || !membership_recovery_requested {
        return Ok(initial_expectations);
    }

    let recovery_tokens = tokens_with_completion_marker(
        source,
        completion_point,
        CompletionPointTokenPolicy::MembershipRecovery,
    )?;

    let recovery_collector = Rc::new(RefCell::new(CompletionCollector {
        expectations: initial_expectations,
        allows_hole_recovery: true,
        ..CompletionCollector::default()
    }));
    run_completion_parse(recovery_tokens, &recovery_collector);
    let recovered_expectations = recovery_collector.borrow().expectations.clone();
    Ok(recovered_expectations)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expectations_at(source: &str, byte_offset: usize) -> ParserExpectations {
        let completion_point =
            TextSize::try_from(byte_offset).expect("test completion point fits TextSize");
        collect_expectations(source, completion_point).unwrap()
    }

    fn expectations_at_end(source: &str) -> ParserExpectations {
        expectations_at(source, source.len())
    }

    fn assert_token_provenance(source: &str, expectations: &ParserExpectations) {
        for token in &expectations.tokens {
            assert!(
                expectations.direct_tokens.contains(token)
                    || expectations.lookahead_tokens.contains(token)
                    || expectations.expression_start_tokens.contains(token)
                    || expectations.expression_continuation_tokens.contains(token)
                    || expectations.follow_tokens.contains(token),
                "token without provenance in {source:?}: {token:?}: {expectations:?}"
            );
        }
        for token in expectations
            .direct_tokens
            .iter()
            .chain(&expectations.lookahead_tokens)
            .chain(&expectations.expression_start_tokens)
            .chain(&expectations.expression_continuation_tokens)
            .chain(&expectations.follow_tokens)
        {
            assert!(
                expectations.tokens.contains(token),
                "provenance token missing from union in {source:?}: {token:?}: {expectations:?}"
            );
        }
    }

    fn assert_slots_at_end(cases: &[(&str, GrammarSlot)]) {
        for &(source, expected_slot) in cases {
            let expectations = expectations_at_end(source);
            assert!(
                expectations.slots.contains(&expected_slot),
                "{source}: {:?}",
                expectations.slots
            );
        }
    }

    #[test]
    fn collects_statement_starters() {
        let expectations = expectations_at_end("");
        let actual = expectations
            .tokens
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let expected = STATEMENT_FAMILIES
            .iter()
            .flat_map(|family| family.start_tokens())
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_statement_family_collects_through_its_complete_sample() {
        for family in STATEMENT_FAMILIES {
            let source = family.sample_sql();
            let tokens = crate::lex(source).unwrap_or_else(|error| {
                panic!("invalid completion coverage sample {source:?}: {error}")
            });
            let mut points = tokens
                .iter()
                .flat_map(|token| [token.range.start(), token.range.end()])
                .collect::<Vec<_>>();
            points.sort_unstable();
            points.dedup();
            for point in points {
                let expectations = collect_expectations(source, point).unwrap_or_else(|error| {
                    panic!(
                        "completion failed for family sample {source:?} at byte {}: {error}",
                        usize::from(point)
                    )
                });
                assert_token_provenance(source, &expectations);
            }

            let complete_expectations = expectations_at_end(source);
            assert!(
                !complete_expectations.tokens.contains(&TokenKind::Char(';')),
                "complete family sample published the statement terminator: {source:?}: {complete_expectations:?}"
            );
            assert!(
                complete_expectations
                    .slots
                    .iter()
                    .all(|slot| *slot == GrammarSlot::Alias),
                "complete family sample published a stale catalog slot: {source:?}: {complete_expectations:?}"
            );
        }
    }

    #[test]
    fn collects_select_and_from_slots() {
        let expectations = expectations_at_end("SELECT ");
        assert!(expectations.slots.contains(&GrammarSlot::Column));
        assert!(expectations.slots.contains(&GrammarSlot::Function));
        assert!(expectations.tokens.contains(&TokenKind::From));

        let expectations = expectations_at_end("SELECT * FROM ");
        assert!(expectations.slots.contains(&GrammarSlot::Relation));
        assert!(expectations.slots.contains(&GrammarSlot::Function));
    }

    #[test]
    fn collects_relation_slot_after_schema_qualifier() {
        let sql = "SELECT * FROM public.";
        let expectations = expectations_at_end(sql);
        assert!(expectations.slots.contains(&GrammarSlot::Relation));
    }

    #[test]
    fn publishes_membership_before_the_completion_point() {
        let sql = "ALTER TABLE app.accounts DROP COLUMN ";
        let expectations = expectations_at_end(sql);
        let membership = expectations.membership.expect("membership");
        assert_eq!(membership.member_slots, [GrammarSlot::Column]);
        assert_eq!(membership.owner.object_types, [ObjectType::Table]);
        assert_eq!(
            membership
                .owner
                .name
                .iter()
                .filter_map(token_name)
                .collect::<Vec<_>>(),
            ["app", "accounts"]
        );
    }

    #[test]
    fn publishes_membership_after_the_completion_point() {
        for (sql, point) in [
            (
                "GRANT SELECT () ON TABLE app.accounts TO role_name",
                "GRANT SELECT (".len(),
            ),
            (
                "CREATE TRIGGER tr BEFORE UPDATE OF  ON app.accounts EXECUTE FUNCTION f()",
                "CREATE TRIGGER tr BEFORE UPDATE OF ".len(),
            ),
            (
                "CREATE STATISTICS s ON (lower()) FROM app.accounts",
                "CREATE STATISTICS s ON (lower(".len(),
            ),
        ] {
            let expectations = expectations_at(sql, point);
            let membership = expectations
                .membership
                .unwrap_or_else(|| panic!("missing membership for {sql:?}"));
            assert_eq!(membership.member_slots, [GrammarSlot::Column], "{sql:?}");
            assert_eq!(
                membership
                    .owner
                    .name
                    .iter()
                    .filter_map(token_name)
                    .collect::<Vec<_>>(),
                ["app", "accounts"],
                "{sql:?}"
            );
        }
    }

    #[test]
    fn membership_owner_does_not_leak_across_statements() {
        let sql = "CREATE INDEX i ON app.accounts (id); SELECT ";
        let expectations = expectations_at_end(sql);

        assert!(expectations.slots.contains(&GrammarSlot::Column));
        assert_eq!(expectations.membership, None);
    }

    #[test]
    fn collects_alias_slots_without_leaking_past_explicit_as() {
        let implicit_source = "SELECT * FROM public.orders o";
        let implicit_point = implicit_source.rfind('o').unwrap();
        let implicit_expectations = expectations_at(implicit_source, implicit_point);
        assert!(implicit_expectations.slots.contains(&GrammarSlot::Alias));

        let explicit_source = "SELECT * FROM public.orders AS ";
        let explicit_expectations = expectations_at_end(explicit_source);
        assert_eq!(explicit_expectations.slots, [GrammarSlot::Alias]);
        assert!(
            explicit_expectations.tokens.is_empty(),
            "{explicit_expectations:?}"
        );
        assert!(
            explicit_expectations.phrases.is_empty(),
            "{explicit_expectations:?}"
        );
    }

    #[test]
    fn collects_join_starter_after_relation_alias() {
        for sql in [
            "SELECT * FROM public.orders o ",
            "SELECT * FROM public.orders AS o ",
        ] {
            let expectations = expectations_at_end(sql);
            assert!(expectations.tokens.contains(&TokenKind::Join), "{sql}");
        }
    }

    #[test]
    fn separates_expression_continuations_from_enclosing_follows() {
        let sql = "SELECT * FROM public.users JOIN public.orders ON users.id ";
        let expectations = expectations_at_end(sql);

        for continuation in [TokenKind::Char('='), TokenKind::Between, TokenKind::And] {
            assert!(
                expectations.tokens.contains(&continuation),
                "{expectations:?}"
            );
            assert!(
                expectations
                    .expression_continuation_tokens
                    .contains(&continuation),
                "{continuation:?}: {expectations:?}"
            );
            assert!(
                !expectations.follow_tokens.contains(&continuation),
                "{continuation:?}: {expectations:?}"
            );
        }
        for follow in [TokenKind::Join, TokenKind::Where, TokenKind::GroupP] {
            assert!(
                expectations.follow_tokens.contains(&follow),
                "{follow:?}: {expectations:?}"
            );
        }
    }

    #[test]
    fn publishes_fixed_phrases_as_units() {
        let cases: &[(&str, &[&'static [TokenKind]])] = &[
            (
                "SELECT * FROM t ",
                &[
                    &[TokenKind::GroupP, TokenKind::By],
                    &[TokenKind::Order, TokenKind::By],
                ],
            ),
            ("DROP TABLE ", &[&[TokenKind::IfP, TokenKind::Exists]]),
            (
                "CREATE TABLE ",
                &[&[TokenKind::IfP, TokenKind::Not, TokenKind::Exists]],
            ),
            (
                "CREATE TABLE t (c int ",
                &[
                    &[TokenKind::Not, TokenKind::NullP],
                    &[TokenKind::Primary, TokenKind::Key],
                ],
            ),
            (
                "CREATE TABLE t (CONSTRAINT c ",
                &[
                    &[TokenKind::Primary, TokenKind::Key],
                    &[TokenKind::Foreign, TokenKind::Key],
                ],
            ),
            (
                "SELECT sum(x) OVER (",
                &[
                    &[TokenKind::Partition, TokenKind::By],
                    &[TokenKind::Order, TokenKind::By],
                ],
            ),
            ("SELECT array_agg(x ", &[&[TokenKind::Order, TokenKind::By]]),
            ("SELECT rank() ", &[&[TokenKind::Within, TokenKind::GroupP]]),
        ];
        for (sql, phrases) in cases {
            let expectations = expectations_at_end(sql);
            for phrase in *phrases {
                assert!(
                    expectations.phrases.contains(phrase),
                    "{sql}: {:?}",
                    expectations.phrases
                );
                assert!(
                    expectations.tokens.contains(&phrase[0]),
                    "{sql}: phrase head missing from tokens: {:?}",
                    expectations.tokens
                );
            }
        }
    }

    #[test]
    fn complete_expression_fragments_publish_outer_follow_tokens() {
        let select_expectations = expectations_at_end("SELECT 1");
        assert!(select_expectations.tokens.contains(&TokenKind::Char(',')));
        assert!(select_expectations.tokens.contains(&TokenKind::From));
        assert!(select_expectations.tokens.contains(&TokenKind::And));
        assert!(select_expectations.tokens.contains(&TokenKind::TypeCast));
        assert!(select_expectations.follow_tokens.contains(&TokenKind::From));
        assert!(
            select_expectations
                .follow_tokens
                .contains(&TokenKind::Char(','))
        );
        assert!(
            select_expectations
                .expression_continuation_tokens
                .contains(&TokenKind::And)
        );
        assert!(
            select_expectations
                .expression_continuation_tokens
                .contains(&TokenKind::TypeCast)
        );
        assert!(!select_expectations.slots.contains(&GrammarSlot::Operator));

        let sql = "SELECT * FROM t WHERE true";
        let where_clause_expectations = expectations_at_end(sql);
        assert!(
            where_clause_expectations
                .tokens
                .contains(&TokenKind::GroupP)
        );
        assert!(where_clause_expectations.tokens.contains(&TokenKind::Order));
    }

    #[test]
    fn completed_names_and_restricted_calls_do_not_publish_stale_slots() {
        let drop_table = expectations_at_end("DROP TABLE target ");
        assert!(!drop_table.slots.contains(&GrammarSlot::Table));
        assert!(!drop_table.tokens.contains(&TokenKind::Char(';')));

        let alter_expectations = expectations_at_end("ALTER TABLE t ADD COLUMN c int ");
        assert!(!alter_expectations.slots.contains(&GrammarSlot::Type));
        assert!(!alter_expectations.tokens.contains(&TokenKind::Char(';')));

        let setting_expectations = expectations_at_end("SET work_mem = '4MB' ");
        assert!(!setting_expectations.slots.contains(&GrammarSlot::AnyName));
        assert!(!setting_expectations.tokens.contains(&TokenKind::Default));
        assert!(setting_expectations.tokens.contains(&TokenKind::Char(',')));
        assert!(!setting_expectations.tokens.contains(&TokenKind::Char(';')));

        let call_expectations = expectations_at_end("CALL f() ");
        assert!(call_expectations.tokens.is_empty());
        assert!(call_expectations.slots.is_empty());

        let signature_expectations = expectations_at_end("DROP FUNCTION f(int) ");
        assert!(!signature_expectations.slots.contains(&GrammarSlot::Type));
        assert!(
            !signature_expectations
                .slots
                .contains(&GrammarSlot::Function)
        );
        assert!(
            !signature_expectations
                .tokens
                .contains(&TokenKind::Char(';'))
        );
    }

    #[test]
    fn collects_slot_inside_an_expression_fragment() {
        let sql = "SELECT u.na FROM users AS u";
        let expectations = expectations_at(sql, sql.find("na").unwrap());
        assert!(expectations.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE INDEX i ON t ((lower(x)) COLLATE c)";
        let expectations = expectations_at(sql, sql.find('x').unwrap());
        assert!(expectations.slots.contains(&GrammarSlot::Column));
        assert!(expectations.slots.contains(&GrammarSlot::Function));
    }

    #[test]
    fn propagates_completion_into_deferred_expression_fragments() {
        for sql in [
            "SELECT * FROM JSON_TABLE(",
            "SELECT * FROM XMLTABLE(",
            "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT ",
            "SELECT * FROM XMLTABLE('/x' PASSING ",
            "SELECT * FROM XMLTABLE('/x' PASSING doc COLUMNS c text DEFAULT ",
            "SELECT * FROM ROWS FROM (lower(",
            "SELECT * FROM generate_series(",
            "SELECT * FROM JSON_TABLE(doc, '$' PASSING ",
            "SELECT * FROM JSON_TABLE(doc, '$' COLUMNS (c int DEFAULT ",
            "SELECT JSON_ARRAYAGG(value ORDER BY ",
            "SELECT * FROM t OFFSET lower(",
            "SELECT * FROM t FETCH FIRST lower(",
            "SELECT sum(x) OVER (PARTITION BY ",
            "CREATE INDEX i ON t ((lower(",
            "CREATE TABLE t (c int) PARTITION BY RANGE ((lower(",
            "CREATE TABLE t (EXCLUDE USING gist ((lower(",
            "ALTER TABLE t ADD COLUMN c int DEFAULT ",
            "CREATE FUNCTION f(x int DEFAULT ",
            "CREATE STATISTICS s ON (lower(",
            "INSERT INTO t VALUES (1) ON CONFLICT ((lower(",
            "UPDATE t SET a[lower(",
        ] {
            let expectations = expectations_at_end(sql);
            assert!(
                expectations.slots.contains(&GrammarSlot::Column),
                "{sql}: {:?}",
                expectations.slots
            );
            assert!(
                expectations.slots.contains(&GrammarSlot::Function),
                "{sql}: {:?}",
                expectations.slots
            );
        }
    }

    #[test]
    fn propagates_completion_into_copy_option_fragments() {
        let option_expectations = expectations_at_end("COPY source_table TO STDOUT WITH (");
        assert!(option_expectations.tokens.contains(&TokenKind::Format));
        assert!(option_expectations.slots.contains(&GrammarSlot::AnyName));

        let column_expectations =
            expectations_at_end("COPY source_table TO STDOUT WITH (force_quote (");
        assert!(column_expectations.slots.contains(&GrammarSlot::Column));
        assert!(!column_expectations.slots.contains(&GrammarSlot::AnyName));
    }

    #[test]
    fn xmltable_column_fragment_shares_the_expression_collector() {
        let mut tokens = crate::lex("c text DEFAULT ").unwrap();
        let eof = tokens.pop().unwrap();
        tokens.push(Token::synthetic(TokenKind::Completion, eof.location()));
        let collector = Rc::new(RefCell::new(CompletionCollector::default()));
        let _ = xmltable_column_from_tokens_with_completion(tokens, Some(collector.clone()));
        let slots = &collector.borrow().expectations.slots;
        assert!(slots.contains(&GrammarSlot::Column), "{slots:?}");
        assert!(slots.contains(&GrammarSlot::Function), "{slots:?}");
    }

    #[test]
    fn collects_json_array_query_suffixes_after_the_nested_query() {
        let format_expectations = expectations_at_end("SELECT JSON_ARRAY(SELECT 1 FORMAT ");
        assert!(format_expectations.tokens.contains(&TokenKind::Json));

        let returning_expectations = expectations_at_end("SELECT JSON_ARRAY(SELECT 1 RETURNING ");
        assert!(returning_expectations.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn recovers_an_unterminated_token_at_the_point() {
        let sql = "SELECT \"na";
        let expectations = expectations_at(sql, 7);
        assert!(expectations.slots.contains(&GrammarSlot::Column));
    }

    #[test]
    fn collects_dml_and_ddl_slots() {
        let expectations = expectations_at_end("UPDATE accounts SET ");
        assert!(expectations.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE TABLE t (c )";
        let expectations = expectations_at(sql, sql.find(')').unwrap());
        assert!(expectations.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn collects_create_alter_and_drop_families() {
        let create_expectations = expectations_at_end("CREATE ");
        assert!(create_expectations.tokens.contains(&TokenKind::Table));
        assert!(create_expectations.tokens.contains(&TokenKind::Function));

        let alter_expectations = expectations_at_end("ALTER ");
        assert!(alter_expectations.tokens.contains(&TokenKind::Table));
        assert!(alter_expectations.tokens.contains(&TokenKind::Role));

        let drop_expectations = expectations_at_end("DROP ");
        assert!(drop_expectations.tokens.contains(&TokenKind::Table));
        assert!(drop_expectations.tokens.contains(&TokenKind::Function));
    }

    #[test]
    fn classifies_common_object_name_positions() {
        let cases = [
            ("ALTER TABLE ", GrammarSlot::Table),
            ("ALTER TABLE t DROP COLUMN ", GrammarSlot::Column),
            ("DROP FUNCTION ", GrammarSlot::Function),
            ("COMMENT ON COLUMN t.", GrammarSlot::Column),
            ("GRANT SELECT ON TABLE t TO ", GrammarSlot::Role),
        ];
        assert_slots_at_end(&cases);

        for sql in ["DROP FUNCTION f(", "ALTER FUNCTION f(", "DROP OPERATOR +("] {
            let expectations = expectations_at_end(sql);
            assert!(
                expectations.slots.contains(&GrammarSlot::Type),
                "{sql}: {:?}",
                expectations.slots
            );
            assert!(
                !expectations.slots.contains(&GrammarSlot::Function)
                    && !expectations.slots.contains(&GrammarSlot::Operator),
                "{sql}: {:?}",
                expectations.slots
            );
        }
    }

    #[test]
    fn publishes_catalog_slots_for_ddl_object_names() {
        let cases = [
            ("CREATE TABLE ", GrammarSlot::Table),
            ("CREATE INDEX ", GrammarSlot::Index),
            ("CREATE SCHEMA ", GrammarSlot::Schema),
            ("CREATE DATABASE ", GrammarSlot::Database),
            ("CREATE SEQUENCE ", GrammarSlot::Sequence),
            ("CREATE TYPE ", GrammarSlot::Type),
            ("CREATE COLLATION ", GrammarSlot::Collation),
            ("CREATE OPERATOR ", GrammarSlot::Operator),
            ("CREATE OPERATOR CLASS ", GrammarSlot::OperatorClass),
            ("CREATE ROLE ", GrammarSlot::Role),
            ("ALTER INDEX ", GrammarSlot::Index),
            ("ALTER SEQUENCE ", GrammarSlot::Sequence),
            ("ALTER DATABASE ", GrammarSlot::Database),
            ("ALTER SCHEMA ", GrammarSlot::Schema),
            ("ALTER COLLATION ", GrammarSlot::Collation),
            ("ALTER ROLE ", GrammarSlot::Role),
            ("DROP VIEW ", GrammarSlot::View),
            ("DROP INDEX ", GrammarSlot::Index),
            ("DROP SCHEMA ", GrammarSlot::Schema),
            ("DROP SEQUENCE ", GrammarSlot::Sequence),
            ("DROP TYPE ", GrammarSlot::Type),
            ("DROP COLLATION ", GrammarSlot::Collation),
            ("DROP OPERATOR ", GrammarSlot::Operator),
            ("DROP ROLE ", GrammarSlot::Role),
        ];
        assert_slots_at_end(&cases);

        let index_target = expectations_at_end("CREATE INDEX i ON ");
        assert!(index_target.slots.contains(&GrammarSlot::MaterializedView));
    }

    #[test]
    fn publishes_catalog_slots_inside_ddl_and_expression_clauses() {
        let cases = [
            ("SELECT 1::", GrammarSlot::Type),
            ("SELECT 1 COLLATE ", GrammarSlot::Collation),
            ("CREATE INDEX i ON ", GrammarSlot::Table),
            ("CREATE INDEX i ON t USING ", GrammarSlot::AccessMethod),
            (
                "CREATE TABLE t (c int) TABLESPACE ",
                GrammarSlot::Tablespace,
            ),
            (
                "CREATE FOREIGN TABLE t (c int) SERVER ",
                GrammarSlot::ForeignServer,
            ),
            ("ALTER DATABASE db SET TABLESPACE ", GrammarSlot::Tablespace),
            ("DO LANGUAGE ", GrammarSlot::Language),
            ("CREATE INDEX i ON t (c COLLATE ", GrammarSlot::Collation),
            ("CREATE TABLE t (c int REFERENCES ", GrammarSlot::Table),
            ("CREATE TABLE t (c int CONSTRAINT ", GrammarSlot::Constraint),
            ("ALTER TABLE t ALTER COLUMN c TYPE ", GrammarSlot::Type),
            (
                "ALTER TABLE t ALTER COLUMN c TYPE text COLLATE ",
                GrammarSlot::Collation,
            ),
            ("COMMENT ON TYPE ", GrammarSlot::Type),
            ("COMMENT ON OPERATOR CLASS ", GrammarSlot::OperatorClass),
            ("CREATE POLICY p ON ", GrammarSlot::Table),
            ("DROP POLICY p ON ", GrammarSlot::Table),
            ("GRANT role_a TO ", GrammarSlot::Role),
        ];
        assert_slots_at_end(&cases);
    }

    #[test]
    fn publishes_catalog_slots_across_utility_object_positions() {
        let cases = [
            ("CREATE ACCESS METHOD ", GrammarSlot::AccessMethod),
            (
                "CREATE ACCESS METHOD am TYPE TABLE HANDLER ",
                GrammarSlot::Function,
            ),
            ("CREATE EXTENSION ", GrammarSlot::Extension),
            ("CREATE EXTENSION ext WITH SCHEMA ", GrammarSlot::Schema),
            ("CREATE SERVER ", GrammarSlot::ForeignServer),
            ("CREATE USER MAPPING FOR ", GrammarSlot::Role),
            (
                "CREATE USER MAPPING FOR role SERVER ",
                GrammarSlot::ForeignServer,
            ),
            ("CREATE LANGUAGE lang HANDLER ", GrammarSlot::Function),
            ("CREATE POLICY p ON t TO ", GrammarSlot::Role),
            ("CREATE PUBLICATION p FOR TABLE ", GrammarSlot::Table),
            (
                "CREATE PUBLICATION p FOR TABLES IN SCHEMA ",
                GrammarSlot::Schema,
            ),
            ("CREATE STATISTICS s ON c FROM ", GrammarSlot::Table),
            ("CREATE TABLE t (LIKE ", GrammarSlot::Table),
            ("CREATE TRIGGER trg BEFORE INSERT ON ", GrammarSlot::Table),
            ("CREATE RULE r AS ON SELECT TO ", GrammarSlot::Table),
            ("CREATE CAST (", GrammarSlot::Type),
            ("DROP CAST (", GrammarSlot::Type),
            ("CREATE CONVERSION ", GrammarSlot::Conversion),
            (
                "CREATE CONVERSION c FOR 'UTF8' TO 'LATIN1' FROM ",
                GrammarSlot::Function,
            ),
            ("CREATE TRANSFORM FOR int LANGUAGE ", GrammarSlot::Language),
            (
                "CREATE TRANSFORM FOR int LANGUAGE sql (FROM SQL WITH FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE POLICY ", GrammarSlot::Policy),
            ("ALTER POLICY ", GrammarSlot::Policy),
            ("CREATE PROPERTY GRAPH ", GrammarSlot::PropertyGraph),
            ("ALTER PROPERTY GRAPH ", GrammarSlot::PropertyGraph),
            (
                "CREATE PROPERTY GRAPH g VERTEX TABLES (",
                GrammarSlot::Table,
            ),
            (
                "CREATE SUBSCRIPTION s CONNECTION 'host=x' PUBLICATION ",
                GrammarSlot::Publication,
            ),
            (
                "ALTER SUBSCRIPTION s SET PUBLICATION ",
                GrammarSlot::Publication,
            ),
            ("CREATE EVENT TRIGGER ", GrammarSlot::EventTrigger),
            (
                "CREATE EVENT TRIGGER trg ON ddl_command_start EXECUTE FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE TRIGGER ", GrammarSlot::Trigger),
            ("CREATE TRIGGER trg BEFORE UPDATE OF ", GrammarSlot::Column),
            (
                "CREATE TRIGGER trg BEFORE INSERT ON t EXECUTE FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE RULE ", GrammarSlot::Rule),
            (
                "CREATE TEXT SEARCH PARSER p (START = ",
                GrammarSlot::Function,
            ),
            (
                "CREATE TEXT SEARCH CONFIGURATION c (PARSER = ",
                GrammarSlot::TextSearchParser,
            ),
            (
                "CREATE TEXT SEARCH DICTIONARY d (TEMPLATE = ",
                GrammarSlot::TextSearchTemplate,
            ),
            ("DECLARE ", GrammarSlot::AnyName),
            ("CLOSE ", GrammarSlot::AnyName),
            ("FETCH FROM ", GrammarSlot::AnyName),
            ("MOVE IN ", GrammarSlot::AnyName),
            ("PREPARE ", GrammarSlot::AnyName),
            ("EXECUTE ", GrammarSlot::AnyName),
            ("DEALLOCATE ", GrammarSlot::AnyName),
            ("SET ROLE ", GrammarSlot::Role),
            ("SET SESSION AUTHORIZATION ", GrammarSlot::Role),
            ("SAVEPOINT ", GrammarSlot::AnyName),
            ("RELEASE SAVEPOINT ", GrammarSlot::AnyName),
            ("ROLLBACK TO SAVEPOINT ", GrammarSlot::AnyName),
            ("LISTEN ", GrammarSlot::AnyName),
            ("UNLISTEN ", GrammarSlot::AnyName),
            ("NOTIFY ", GrammarSlot::AnyName),
            ("CREATE OPERATOR @@ (PROCEDURE = ", GrammarSlot::Function),
            (
                "ALTER OPERATOR @@ (int, int) SET (RESTRICT = ",
                GrammarSlot::Function,
            ),
            (
                "ALTER OPERATOR @@ (int, int) SET (COMMUTATOR = ",
                GrammarSlot::Operator,
            ),
            ("GRANT USAGE ON SCHEMA ", GrammarSlot::Schema),
            ("REINDEX INDEX ", GrammarSlot::Index),
            ("REINDEX SCHEMA ", GrammarSlot::Schema),
            ("REINDEX DATABASE ", GrammarSlot::Database),
            ("VACUUM t (", GrammarSlot::Column),
            ("CREATE FUNCTION ", GrammarSlot::Function),
            ("CREATE FUNCTION f(arg ", GrammarSlot::Type),
            ("CREATE FUNCTION f() RETURNS ", GrammarSlot::Type),
            (
                "CREATE FUNCTION f() RETURNS int LANGUAGE ",
                GrammarSlot::Language,
            ),
            (
                "CREATE FUNCTION f() RETURNS int SUPPORT ",
                GrammarSlot::Function,
            ),
            ("CREATE TABLESPACE ", GrammarSlot::Tablespace),
            ("CREATE TABLESPACE ts OWNER ", GrammarSlot::Role),
            ("CREATE STATISTICS ", GrammarSlot::Statistics),
            ("ALTER STATISTICS ", GrammarSlot::Statistics),
            (
                "CREATE TEXT SEARCH DICTIONARY ",
                GrammarSlot::TextSearchDictionary,
            ),
            ("CREATE SEQUENCE s AS ", GrammarSlot::Type),
            ("CREATE SEQUENCE s OWNED BY ", GrammarSlot::Column),
        ];
        assert_slots_at_end(&cases);
    }

    #[test]
    fn completion_marker_uses_typed_parser_control() {
        let parser = Parser {
            tokens: vec![
                Token::synthetic(TokenKind::Completion, 0),
                Token::synthetic(TokenKind::Eof, 0),
            ],
            pos: 0,
            completion: Some(Rc::new(RefCell::new(CompletionCollector::default()))),
        };
        assert!(matches!(
            parser.error_here("not a syntax error"),
            ParserExit::Completion(_)
        ));
    }

    #[test]
    fn every_dispatched_statement_family_has_completion_boundary_coverage() {
        for family in STATEMENT_FAMILIES {
            let sql = family.sample_sql();
            let tokens = lex(sql).unwrap_or_else(|error| {
                panic!("failed to lex {:?} sample {sql:?}: {error}", family)
            });
            let first_kind = tokens[0].kind;
            let second_kind = tokens.get(1).map_or(TokenKind::Eof, |token| token.kind);
            assert_eq!(
                classify_statement(first_kind, second_kind),
                Some(*family),
                "coverage sample does not dispatch to its registered family: {sql:?}"
            );
            parse_one(sql).unwrap_or_else(|error| {
                panic!("registered completion sample does not parse: {sql:?}: {error}")
            });

            let mut points = tokens
                .iter()
                .flat_map(|token| [token.range.start(), token.range.end()])
                .collect::<Vec<_>>();
            points.sort_unstable();
            points.dedup();
            for point in points {
                collect_expectations(sql, point).unwrap_or_else(|error| {
                    panic!(
                        "completion collection failed for {:?} at byte {}: {error}",
                        family,
                        usize::from(point)
                    )
                });
            }
        }
    }
}
