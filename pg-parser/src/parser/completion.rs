use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GrammarSlot {
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
    pub object_types: Vec<ObjectType>,
    pub name: Vec<Token>,
}

/// The grammar-level membership relation at the completion point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrammarMembership {
    pub member_slots: Vec<GrammarSlot>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParserExpectations {
    pub tokens: Vec<TokenKind>,
    /// Tokens introduced directly by the active grammar production.
    pub direct_tokens: Vec<TokenKind>,
    /// Keyword alternatives observed through parser lookahead predicates.
    /// These are syntactically reachable but are not eager editor items until
    /// the user starts typing a prefix.
    pub lookahead_tokens: Vec<TokenKind>,
    /// Tokens that can start the active expression.
    pub expression_start_tokens: Vec<TokenKind>,
    /// Tokens that extend the already parsed expression.
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
    pub slots: Vec<GrammarSlot>,
    pub membership: Option<GrammarMembership>,
}

#[derive(Debug, Default)]
pub(super) struct CompletionCollector {
    expectations: ParserExpectations,
    recover_holes: bool,
    recovered_holes: usize,
    membership_owners: Vec<(Vec<GrammarSlot>, GrammarObjectReference)>,
    needs_membership_recovery: bool,
}

pub(super) type SharedCollector = std::rc::Rc<std::cell::RefCell<CompletionCollector>>;

#[derive(Clone, Copy)]
enum CompletionPass {
    Initial,
    MembershipRecovery,
}

impl CompletionPass {
    fn removes_token_at_point(self, token: &Token, point: TextSize) -> bool {
        let intersects_point = token.kind != TokenKind::Eof
            && token.range.start() <= point
            && (point < token.range.end()
                || (token.kind == TokenKind::Incomplete && point == token.range.end()));
        if !intersects_point {
            return false;
        }

        match self {
            Self::Initial => true,
            // The recovery pass must keep complete punctuation and numeric
            // tokens that start at the point: they belong to the owner syntax
            // to the right of the recovered name hole. Names and incomplete
            // tokens are still editor prefixes and therefore get replaced.
            Self::MembershipRecovery => {
                token.range.start() < point
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
    pub(super) fn tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.direct_tokens, kinds);
    }

    pub(super) fn expression_start_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.expression_start_tokens, kinds);
    }

    pub(super) fn lookahead_tokens(&mut self, kinds: &[TokenKind]) {
        let keywords = kinds
            .iter()
            .copied()
            .filter(|kind| crate::KEYWORDS.iter().any(|keyword| keyword.kind == *kind))
            .collect::<Vec<_>>();
        Self::insert_tokens(&mut self.expectations.tokens, &keywords);
        Self::insert_tokens(&mut self.expectations.lookahead_tokens, &keywords);
    }

    pub(super) fn expression_continuation_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.expression_continuation_tokens, kinds);
    }

    pub(super) fn expression_continuation_phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.expression_continuation_tokens(&phrase[..1]);
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

    pub(super) fn phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    pub(super) fn follow_tokens(&mut self, kinds: &[TokenKind]) {
        Self::insert_tokens(&mut self.expectations.tokens, kinds);
        Self::insert_tokens(&mut self.expectations.follow_tokens, kinds);
    }

    pub(super) fn follow_phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.follow_tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    pub(super) fn slot(&mut self, slot: GrammarSlot) {
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
            .membership_owners
            .iter()
            .rev()
            .find(|(member_slots, _)| member_slots.contains(&slot))
            .cloned()
        {
            self.attach_membership_owner(member_slots, owner);
        }
    }

    pub(super) fn membership(
        &mut self,
        member_slots: &[GrammarSlot],
        owner: GrammarObjectReference,
    ) {
        if self.expectations.membership.is_none() {
            self.expectations.membership = Some(GrammarMembership {
                member_slots: member_slots.to_vec(),
                owner,
            });
        }
    }

    pub(super) fn push_membership_owner(
        &mut self,
        member_slots: &[GrammarSlot],
        owner: GrammarObjectReference,
    ) {
        self.membership_owners
            .push((member_slots.to_vec(), owner.clone()));
        if self.recovered_holes > 0
            || member_slots
                .iter()
                .any(|member| self.expectations.slots.contains(member))
        {
            self.attach_membership_owner(member_slots.to_vec(), owner);
        }
    }

    pub(super) fn pop_membership_owner(&mut self) {
        self.membership_owners.pop();
    }

    fn clear_membership_owners(&mut self) {
        self.membership_owners.clear();
    }

    pub(super) fn request_membership_recovery(&mut self) {
        self.needs_membership_recovery = true;
    }

    pub(super) fn recover_hole(&mut self) -> bool {
        if !self.recover_holes {
            return false;
        }
        self.recovered_holes += 1;
        true
    }

    fn attach_membership_owner(
        &mut self,
        member_slots: Vec<GrammarSlot>,
        owner: GrammarObjectReference,
    ) {
        let member_slots = member_slots
            .into_iter()
            .filter(|member| self.expectations.slots.contains(member))
            .collect::<Vec<_>>();
        if !member_slots.is_empty() {
            self.membership(&member_slots, owner);
        }
    }
}

fn tokens_with_completion_marker(
    source: &str,
    point: TextSize,
    pass: CompletionPass,
) -> Result<Vec<Token>, crate::lexer::LexError> {
    let mut tokens = crate::lexer::lex_for_completion(source, point)?.into_tokens();
    if let Some(index) = tokens
        .iter()
        .position(|token| pass.removes_token_at_point(token, point))
    {
        tokens.remove(index);
    }

    let insertion = tokens
        .iter()
        .position(|token| token.range.start() >= point)
        .unwrap_or_else(|| tokens.len().saturating_sub(1));
    tokens.insert(
        insertion,
        Token::synthetic(TokenKind::Completion, usize::from(point)),
    );
    Ok(tokens)
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
            collector.borrow_mut().tokens(kinds);
        }
    }

    pub(super) fn record_completion_lookahead_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().lookahead_tokens(kinds);
        }
    }

    pub(super) fn record_completion_follow_tokens(&self, kinds: &[TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().follow_tokens(kinds);
        }
    }

    pub(super) fn record_completion_follow_phrase(&self, phrase: &'static [TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().follow_phrase(phrase);
        }
    }

    pub(super) fn record_completion_phrase(&self, phrase: &'static [TokenKind]) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().phrase(phrase);
        }
    }

    pub(super) fn record_completion_slot(&self, slot: GrammarSlot) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
        }
    }

    pub(super) fn record_completion_slot_before(
        &self,
        slot: GrammarSlot,
        stop_tokens: &[TokenKind],
    ) {
        if self.top_level_token_before_completion(stop_tokens) != Some(TokenKind::Char('.')) {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
        }
    }

    /// Publish a slot when the completion marker is anywhere inside the
    /// fragment delimited by top-level `stop_tokens`, not only at its first token.
    pub(super) fn record_completion_slot_within(
        &self,
        slot: GrammarSlot,
        stop_tokens: &[TokenKind],
    ) {
        let follows_fragment_separator = matches!(
            self.top_level_token_before_completion(stop_tokens),
            Some(TokenKind::Char('.') | TokenKind::Char(',') | TokenKind::Char('('))
        );
        if !self.at_completion() && !follows_fragment_separator {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
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

    pub(super) fn push_completion_membership_owner_range(
        &self,
        member_slots: &[GrammarSlot],
        object_types: &[ObjectType],
        start: usize,
        end: usize,
    ) {
        self.push_completion_membership_owner_name(
            member_slots,
            object_types,
            self.completion_name_tokens(start, end),
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
        let recovered = self
            .completion
            .as_ref()
            .is_some_and(|collector| collector.borrow_mut().recover_hole());
        if !recovered {
            return None;
        }
        let location = self.peek().location();
        self.pos += 1;
        Some(Token::completion_hole(location))
    }

    fn completion_name_tokens(&self, start: usize, end: usize) -> Vec<Token> {
        self.tokens[start..end]
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

    fn top_level_token_before_completion(&self, stop_tokens: &[TokenKind]) -> Option<TokenKind> {
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
                kind if delimiter_depth == 0 && stop_tokens.contains(&kind) => return None,
                kind if delimiter_depth == 0 => previous_top_level_kind = Some(kind),
                _ => {}
            }
        }
        None
    }
}

/// Collect grammar candidates at a UTF-8 byte offset.
///
/// A token intersecting the point is treated as the editor prefix and removed
/// from the parser input. Callers normally pass the replacement-range start.
pub fn collect_expectations(
    source: &str,
    point: TextSize,
) -> Result<ParserExpectations, crate::lexer::LexError> {
    let point_usize = usize::from(point).min(source.len());
    let point = TextSize::try_from(point_usize).expect("point was bounded by source length");
    let tokens = tokens_with_completion_marker(source, point, CompletionPass::Initial)?;

    let collector = std::rc::Rc::new(std::cell::RefCell::new(CompletionCollector::default()));
    let mut parser = Parser {
        tokens: tokens.clone(),
        pos: 0,
        completion: Some(collector.clone()),
    };
    let _outcome = parser.parse_statements_with_ranges();
    let baseline = collector.borrow().expectations.clone();
    let needs_membership_recovery = collector.borrow().needs_membership_recovery;
    if baseline.membership.is_some() || !needs_membership_recovery {
        return Ok(baseline);
    }

    let recovery_tokens =
        tokens_with_completion_marker(source, point, CompletionPass::MembershipRecovery)?;

    let recovery_collector = std::rc::Rc::new(std::cell::RefCell::new(CompletionCollector {
        expectations: baseline,
        recover_holes: true,
        ..CompletionCollector::default()
    }));
    let mut recovery_parser = Parser {
        tokens: recovery_tokens,
        pos: 0,
        completion: Some(recovery_collector.clone()),
    };
    let _outcome = recovery_parser.parse_statements_with_ranges();
    let recovered = recovery_collector.borrow().expectations.clone();
    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn collects_statement_starters() {
        let candidates = collect_expectations("", TextSize::ZERO).unwrap();
        let actual = candidates
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

            let complete = collect_expectations(
                source,
                TextSize::try_from(source.len()).expect("sample length fits TextSize"),
            )
            .unwrap();
            assert!(
                !complete.tokens.contains(&TokenKind::Char(';')),
                "complete family sample published the statement terminator: {source:?}: {complete:?}"
            );
            assert!(
                complete
                    .slots
                    .iter()
                    .all(|slot| *slot == GrammarSlot::Alias),
                "complete family sample published a stale catalog slot: {source:?}: {complete:?}"
            );
        }
    }

    #[test]
    fn collects_select_and_from_slots() {
        let candidates = collect_expectations("SELECT ", TextSize::new(7)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
        assert!(candidates.tokens.contains(&TokenKind::From));

        let candidates = collect_expectations("SELECT * FROM ", TextSize::new(14)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Relation));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
    }

    #[test]
    fn collects_relation_slot_after_schema_qualifier() {
        let sql = "SELECT * FROM public.";
        let candidates = collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Relation));
    }

    #[test]
    fn publishes_membership_before_the_completion_point() {
        let sql = "ALTER TABLE app.accounts DROP COLUMN ";
        let expectations =
            collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
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
            let expectations =
                collect_expectations(sql, TextSize::try_from(point).unwrap()).unwrap();
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
        let expectations =
            collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();

        assert!(expectations.slots.contains(&GrammarSlot::Column));
        assert_eq!(expectations.membership, None);
    }

    #[test]
    fn collects_alias_slots_without_leaking_past_explicit_as() {
        let implicit = "SELECT * FROM public.orders o";
        let point = TextSize::try_from(implicit.rfind('o').unwrap()).unwrap();
        let implicit = collect_expectations(implicit, point).unwrap();
        assert!(implicit.slots.contains(&GrammarSlot::Alias));

        let explicit = "SELECT * FROM public.orders AS ";
        let explicit =
            collect_expectations(explicit, TextSize::try_from(explicit.len()).unwrap()).unwrap();
        assert_eq!(explicit.slots, [GrammarSlot::Alias]);
        assert!(explicit.tokens.is_empty(), "{explicit:?}");
        assert!(explicit.phrases.is_empty(), "{explicit:?}");
    }

    #[test]
    fn collects_join_starter_after_relation_alias() {
        for sql in [
            "SELECT * FROM public.orders o ",
            "SELECT * FROM public.orders AS o ",
        ] {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(candidates.tokens.contains(&TokenKind::Join), "{sql}");
        }
    }

    #[test]
    fn separates_expression_continuations_from_enclosing_follows() {
        let sql = "SELECT * FROM public.users JOIN public.orders ON users.id ";
        let candidates = collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();

        for continuation in [TokenKind::Char('='), TokenKind::Between, TokenKind::And] {
            assert!(candidates.tokens.contains(&continuation), "{candidates:?}");
            assert!(
                candidates
                    .expression_continuation_tokens
                    .contains(&continuation),
                "{continuation:?}: {candidates:?}"
            );
            assert!(
                !candidates.follow_tokens.contains(&continuation),
                "{continuation:?}: {candidates:?}"
            );
        }
        for follow in [TokenKind::Join, TokenKind::Where, TokenKind::GroupP] {
            assert!(
                candidates.follow_tokens.contains(&follow),
                "{follow:?}: {candidates:?}"
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
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            for phrase in *phrases {
                assert!(
                    candidates.phrases.contains(phrase),
                    "{sql}: {:?}",
                    candidates.phrases
                );
                assert!(
                    candidates.tokens.contains(&phrase[0]),
                    "{sql}: phrase head missing from tokens: {:?}",
                    candidates.tokens
                );
            }
        }
    }

    #[test]
    fn complete_expression_fragments_publish_outer_follow_tokens() {
        let select = collect_expectations("SELECT 1", TextSize::new(8)).unwrap();
        assert!(select.tokens.contains(&TokenKind::Char(',')));
        assert!(select.tokens.contains(&TokenKind::From));
        assert!(select.tokens.contains(&TokenKind::And));
        assert!(select.tokens.contains(&TokenKind::TypeCast));
        assert!(select.follow_tokens.contains(&TokenKind::From));
        assert!(select.follow_tokens.contains(&TokenKind::Char(',')));
        assert!(
            select
                .expression_continuation_tokens
                .contains(&TokenKind::And)
        );
        assert!(
            select
                .expression_continuation_tokens
                .contains(&TokenKind::TypeCast)
        );
        assert!(!select.slots.contains(&GrammarSlot::Operator));

        let sql = "SELECT * FROM t WHERE true";
        let where_clause =
            collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
        assert!(where_clause.tokens.contains(&TokenKind::GroupP));
        assert!(where_clause.tokens.contains(&TokenKind::Order));
    }

    #[test]
    fn completed_names_and_restricted_calls_do_not_publish_stale_slots() {
        let drop_table = collect_expectations(
            "DROP TABLE target ",
            TextSize::try_from("DROP TABLE target ".len()).unwrap(),
        )
        .unwrap();
        assert!(!drop_table.slots.contains(&GrammarSlot::Table));
        assert!(!drop_table.tokens.contains(&TokenKind::Char(';')));

        let alter = "ALTER TABLE t ADD COLUMN c int ";
        let alter = collect_expectations(alter, TextSize::try_from(alter.len()).unwrap()).unwrap();
        assert!(!alter.slots.contains(&GrammarSlot::Type));
        assert!(!alter.tokens.contains(&TokenKind::Char(';')));

        let setting = "SET work_mem = '4MB' ";
        let setting =
            collect_expectations(setting, TextSize::try_from(setting.len()).unwrap()).unwrap();
        assert!(!setting.slots.contains(&GrammarSlot::AnyName));
        assert!(!setting.tokens.contains(&TokenKind::Default));
        assert!(setting.tokens.contains(&TokenKind::Char(',')));
        assert!(!setting.tokens.contains(&TokenKind::Char(';')));

        let call = "CALL f() ";
        let call = collect_expectations(call, TextSize::try_from(call.len()).unwrap()).unwrap();
        assert!(call.tokens.is_empty());
        assert!(call.slots.is_empty());

        let signature = "DROP FUNCTION f(int) ";
        let signature =
            collect_expectations(signature, TextSize::try_from(signature.len()).unwrap()).unwrap();
        assert!(!signature.slots.contains(&GrammarSlot::Type));
        assert!(!signature.slots.contains(&GrammarSlot::Function));
        assert!(!signature.tokens.contains(&TokenKind::Char(';')));
    }

    #[test]
    fn collects_slot_inside_an_expression_fragment() {
        let sql = "SELECT u.na FROM users AS u";
        let point = TextSize::try_from(sql.find("na").unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE INDEX i ON t ((lower(x)) COLLATE c)";
        let point = TextSize::try_from(sql.find("x").unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
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
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&GrammarSlot::Column),
                "{sql}: {:?}",
                candidates.slots
            );
            assert!(
                candidates.slots.contains(&GrammarSlot::Function),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn propagates_completion_into_copy_option_fragments() {
        let option = "COPY source_table TO STDOUT WITH (";
        let option =
            collect_expectations(option, TextSize::try_from(option.len()).unwrap()).unwrap();
        assert!(option.tokens.contains(&TokenKind::Format));
        assert!(option.slots.contains(&GrammarSlot::AnyName));

        let columns = "COPY source_table TO STDOUT WITH (force_quote (";
        let columns =
            collect_expectations(columns, TextSize::try_from(columns.len()).unwrap()).unwrap();
        assert!(columns.slots.contains(&GrammarSlot::Column));
        assert!(!columns.slots.contains(&GrammarSlot::AnyName));
    }

    #[test]
    fn xmltable_column_fragment_shares_the_expression_collector() {
        let mut tokens = crate::lex("c text DEFAULT ").unwrap();
        let eof = tokens.pop().unwrap();
        tokens.push(Token::synthetic(TokenKind::Completion, eof.location()));
        let collector = std::rc::Rc::new(std::cell::RefCell::new(CompletionCollector::default()));
        let _ = xmltable_column_from_tokens_with_completion(tokens, Some(collector.clone()));
        let slots = &collector.borrow().expectations.slots;
        assert!(slots.contains(&GrammarSlot::Column), "{slots:?}");
        assert!(slots.contains(&GrammarSlot::Function), "{slots:?}");
    }

    #[test]
    fn collects_json_array_query_suffixes_after_the_nested_query() {
        let format_sql = "SELECT JSON_ARRAY(SELECT 1 FORMAT ";
        let format =
            collect_expectations(format_sql, TextSize::try_from(format_sql.len()).unwrap())
                .unwrap();
        assert!(format.tokens.contains(&TokenKind::Json));

        let returning_sql = "SELECT JSON_ARRAY(SELECT 1 RETURNING ";
        let returning = collect_expectations(
            returning_sql,
            TextSize::try_from(returning_sql.len()).unwrap(),
        )
        .unwrap();
        assert!(returning.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn recovers_an_unterminated_token_at_the_point() {
        let sql = "SELECT \"na";
        let candidates = collect_expectations(sql, TextSize::new(7)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
    }

    #[test]
    fn collects_dml_and_ddl_slots() {
        let candidates = collect_expectations("UPDATE accounts SET ", TextSize::new(20)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE TABLE t (c )";
        let point = TextSize::try_from(sql.find(')').unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn collects_create_alter_and_drop_families() {
        let create = collect_expectations("CREATE ", TextSize::new(7)).unwrap();
        assert!(create.tokens.contains(&TokenKind::Table));
        assert!(create.tokens.contains(&TokenKind::Function));

        let alter = collect_expectations("ALTER ", TextSize::new(6)).unwrap();
        assert!(alter.tokens.contains(&TokenKind::Table));
        assert!(alter.tokens.contains(&TokenKind::Role));

        let drop = collect_expectations("DROP ", TextSize::new(5)).unwrap();
        assert!(drop.tokens.contains(&TokenKind::Table));
        assert!(drop.tokens.contains(&TokenKind::Function));
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
        for (sql, slot) in cases {
            let point = TextSize::try_from(sql.len()).unwrap();
            let candidates = collect_expectations(sql, point).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }

        for sql in ["DROP FUNCTION f(", "ALTER FUNCTION f(", "DROP OPERATOR +("] {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&GrammarSlot::Type),
                "{sql}: {:?}",
                candidates.slots
            );
            assert!(
                !candidates.slots.contains(&GrammarSlot::Function)
                    && !candidates.slots.contains(&GrammarSlot::Operator),
                "{sql}: {:?}",
                candidates.slots
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
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }

        let index_target = collect_expectations(
            "CREATE INDEX i ON ",
            TextSize::try_from("CREATE INDEX i ON ".len()).unwrap(),
        )
        .unwrap();
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
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }
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
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn completion_marker_uses_typed_parser_control() {
        let parser = Parser {
            tokens: vec![
                Token::synthetic(TokenKind::Completion, 0),
                Token::synthetic(TokenKind::Eof, 0),
            ],
            pos: 0,
            completion: Some(std::rc::Rc::new(std::cell::RefCell::new(
                CompletionCollector::default(),
            ))),
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
