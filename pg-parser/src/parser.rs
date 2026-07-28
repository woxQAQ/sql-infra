use crate::ast::*;
use crate::lexer::{Token, TokenValue, lex, lookup_keyword};
use crate::{BareLabel, KeywordCategory, TextRange, TextSize, TokenKind};

mod access_method;
mod aggregate_signatures;
mod alter;
mod alter_collation;
mod alter_identity;
mod alter_table;
mod alter_table_partition;
pub mod completion;
pub use completion::{GrammarSlot, ParserExpectations, collect_expectations};
mod constraints;
mod create;
mod create_cast_transform;
mod create_table;
mod create_trigger;
mod cursor;
mod database;
mod define;
mod delete;
mod describe;
mod dml_grammar;
mod domain;
mod drop;
mod expression;
mod expression_call;
mod expression_helpers;
mod expression_json;
mod expression_json_query;
mod expression_prefix;
mod expression_sql;
mod expression_tail;
mod expression_xml;
mod extension;
mod foreign_data;
mod fragment_parser;
mod function_parameters;
mod generic_options;
mod graph;
mod index;
mod insert;
mod json_table;
mod language;
mod maintenance;
mod merge;
mod names;
mod object_helpers;
mod opclass;
mod operator_definition;
mod partition;
mod plpgsql;
mod policy;
mod prepared;
mod privileges;
mod procedural;
mod property_graph;
mod publication;
mod query;
mod query_lists;
mod range;
mod range_tail;
mod rewrite;
mod role_options;
mod routine_alter;
mod routine_create;
mod schema;
mod sequence_options;
mod settings;
mod statistics;
mod table_elements;
mod tablespace;
mod text_search;
mod token_helpers;
mod type_statements;
mod type_tokens;
mod update;
mod window;
mod xmltable_columns;

use aggregate_signatures::*;
use expression::ExprParser;
use expression_helpers::*;
use expression_json::{
    default_json_format, json_behavior_starts, parse_json_value_expr_tokens_with_completion,
};
use fragment_parser::*;
use function_parameters::*;
use index::*;
use object_helpers::*;
use settings::{parse_setting_value_tokens, parse_time_zone_value_tokens};
use table_elements::*;
use token_helpers::*;
use type_tokens::*;
use xmltable_columns::*;

// ── ParseError ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub message: std::string::String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParserExit {
    Syntax(ParseError),
    Completion(TextRange),
}

impl ParseError {
    fn syntax(location: usize, message: impl Into<std::string::String>) -> Self {
        ParseError {
            range: TextRange::empty(
                TextSize::try_from(location).expect("parser locations come from validated input"),
            ),
            message: message.into(),
        }
    }

    pub(super) fn syntax_exit(
        location: usize,
        message: impl Into<std::string::String>,
    ) -> ParserExit {
        ParserExit::Syntax(Self::syntax(location, message))
    }

    pub(super) fn ranged(range: TextRange, message: impl Into<std::string::String>) -> ParserExit {
        ParserExit::Syntax(ParseError {
            range,
            message: message.into(),
        })
    }

    pub fn location(&self) -> usize {
        self.range.start().into()
    }
}

impl ParserExit {
    fn completion(range: TextRange) -> Self {
        Self::Completion(range)
    }

    fn location(&self) -> usize {
        match self {
            Self::Syntax(error) => error.location(),
            Self::Completion(range) => range.start().into(),
        }
    }

    pub(super) fn reanchor(&mut self, location: usize) {
        let range = TextRange::empty(
            TextSize::try_from(location).expect("parser locations come from validated input"),
        );
        match self {
            Self::Syntax(error) => error.range = range,
            Self::Completion(completion_range) => *completion_range = range,
        }
    }

    fn into_parse_error(self) -> ParseError {
        match self {
            Self::Syntax(error) => error,
            Self::Completion(range) => ParseError {
                message: "unexpected synthetic completion marker".to_owned(),
                range,
            },
        }
    }
}

impl From<ParseError> for ParserExit {
    fn from(error: ParseError) -> Self {
        Self::Syntax(error)
    }
}

impl From<crate::lexer::LexError> for ParserExit {
    fn from(error: crate::lexer::LexError) -> Self {
        Self::Syntax(error.into())
    }
}

impl From<crate::lexer::LexError> for ParseError {
    fn from(value: crate::lexer::LexError) -> Self {
        Self {
            message: value.message,
            range: value.range,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.location())
    }
}

impl std::error::Error for ParseError {}

// ── Public API ────────────────────────────────────────────────────────────

type PResult<T> = Result<T, ParserExit>;
type JsonBehaviorPair = (Option<Box<JsonBehavior>>, Option<Box<JsonBehavior>>);

pub fn parse(sql: &str) -> Result<Vec<RawStmt>, ParseError> {
    Ok(parse_with_ranges(sql)?
        .into_iter()
        .map(|statement| statement.raw)
        .collect())
}

/// Parse SQL while retaining complete source ranges for tooling.
///
/// PostgreSQL-compatible `RawStmt::stmt_len` semantics remain unchanged;
/// callers that need the real range of an unterminated final statement should
/// use this interface.
pub fn parse_with_ranges(sql: &str) -> Result<Vec<ParsedStatement>, ParseError> {
    Parser::new(sql)?.parse_with_ranges()
}

pub fn parse_one(sql: &str) -> Result<RawStmt, ParseError> {
    let mut stmts = parse(sql)?;
    if stmts.len() != 1 {
        return Err(ParseError::syntax(
            stmts.get(1).map_or(0, |stmt| stmt.stmt_location as usize),
            format!("expected one statement, found {}", stmts.len()),
        ));
    }
    Ok(stmts.remove(0))
}

pub fn parse_plpgsql_assignment(sql: &str, nnames: i32) -> Result<RawStmt, ParseError> {
    plpgsql::parse_assignment(sql, nnames).map_err(ParserExit::into_parse_error)
}

pub fn parse_plpgsql_expression(sql: &str) -> Result<RawStmt, ParseError> {
    plpgsql::parse_expression(sql).map_err(ParserExit::into_parse_error)
}

pub fn parse_type_name(sql: &str) -> Result<TypeName, ParseError> {
    let mut tokens = lex(sql)?;
    tokens.pop();
    parse_type_name_tokens(tokens).map_err(ParserExit::into_parse_error)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WithTarget {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

#[derive(Clone, Copy)]
enum DescribedIdentityKind {
    AnyName,
    Name,
}

// ── Parser ────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    completion: Option<completion::SharedCollector>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatementRange {
    /// From the first statement token to the terminator or EOF, excluding the
    /// terminating semicolon but retaining trivia within those bounds.
    pub syntax: TextRange,
    /// The semicolon range, when the statement was terminated.
    pub terminator: Option<TextRange>,
}

impl StatementRange {
    pub fn full(self) -> TextRange {
        TextRange::new(
            self.syntax.start(),
            self.terminator.map_or(self.syntax.end(), TextRange::end),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedStatement {
    pub raw: RawStmt,
    pub range: StatementRange,
}

impl Parser {
    pub fn new(sql: &str) -> Result<Self, ParseError> {
        Ok(Self {
            tokens: lex(sql)?,
            pos: 0,
            completion: None,
        })
    }

    pub fn parse(&mut self) -> Result<Vec<RawStmt>, ParseError> {
        self.parse_controlled()
            .map_err(ParserExit::into_parse_error)
    }

    fn parse_controlled(&mut self) -> PResult<Vec<RawStmt>> {
        Ok(self
            .parse_with_ranges_controlled()?
            .into_iter()
            .map(|statement| statement.raw)
            .collect())
    }

    pub fn parse_with_ranges(&mut self) -> Result<Vec<ParsedStatement>, ParseError> {
        self.parse_with_ranges_controlled()
            .map_err(ParserExit::into_parse_error)
    }

    fn parse_with_ranges_controlled(&mut self) -> PResult<Vec<ParsedStatement>> {
        let mut stmts = Vec::new();
        while !self.at(TokenKind::Eof) {
            while self.consume(TokenKind::Char(';')) {}
            if self.at(TokenKind::Eof) {
                break;
            }

            let start = self.location();
            let stmt = self.parse_statement(None)?;
            let end = self.location();
            if !self.at_statement_end() {
                return Err(self.error_here(format!(
                    "expected ';' between statements, found {:?}",
                    self.peek_kind()
                )));
            }
            let terminator = if self.at(TokenKind::Char(';')) {
                Some(self.advance().range)
            } else {
                None
            };
            let syntax = TextRange::new(
                TextSize::try_from(start).expect("validated parser offset"),
                TextSize::try_from(end).expect("validated parser offset"),
            );
            let raw = RawStmt {
                node_tag: NodeTag::RawStmt,
                stmt: Some(Box::new(stmt)),
                stmt_location: start as ParseLoc,
                stmt_len: if terminator.is_some() {
                    end.saturating_sub(start) as ParseLoc
                } else {
                    0
                },
            };
            stmts.push(ParsedStatement {
                raw,
                range: StatementRange { syntax, terminator },
            });
        }
        Ok(stmts)
    }
}

// ── Cursor primitives ─────────────────────────────────────────────────────
//
// Low-level lookahead / match / consume primitives for the hand-written
// recursive-descent parser.  All production rules (parse_statement,
// parse_create, parse_select, …) read / advance the cursor exclusively
// through these methods, never touching `pos` directly — this decouples
// grammar dispatch from cursor bookkeeping.
//
// LL(1)-style predictive recursive descent: usually `peek_kind()` (LA(1))
// suffices to choose a production branch; a few ambiguous spots use
// `peek_kind_n` / `has_top_level_token_before` for bounded extra lookahead.
// No backtracking.
//
// Production code follows these cursor conventions:
// - `consume` expresses optional syntax, repetition, delimiters, and compact
//   binary choices;
// - `match peek_kind()` dispatches required or multi-way grammar alternatives;
// - `expect` consumes mandatory tokens after a production has been selected;
// - fallible `consume_* -> Option<_>` helpers leave the cursor unchanged when
//   returning `None`.

impl Parser {
    /// Consume tokens from the current position until a **top-level** token in
    /// `stops` is found, cloning all consumed tokens into the returned vec.
    ///
    /// "Top-level" is tracked via bracket depth: a stop token only takes
    /// effect at `depth == 0`; the same token nested inside `()` / `[]` is
    /// swallowed.  This allows scooping up an entire SQL fragment for
    /// downstream processing (fragment parser, deferred function bodies, etc.)
    /// without knowing the sub-production structure.
    ///
    /// ## Keyword-pair special cases
    ///
    /// Some stop keywords can also appear as legitimate intra-clause tokens
    /// and must be disambiguated by the preceding token:
    /// - `GROUP`  following `WITHIN`    → `WITHIN GROUP`, not a boundary
    /// - `FOR`    following `COLLATION` → `COLLATION FOR`, not a boundary
    /// - `FROM`   following `DISTINCT`  → `DISTINCT FROM`, not a boundary
    /// - `NOT`    following `IS`        → `IS NOT` predicate, not a boundary
    pub(super) fn take_until_top_level(&mut self, stops: &[TokenKind]) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            if self.at_completion() {
                break;
            }
            let kind = self.peek_kind();
            // Two-word combinations that need the previous token to disambiguate.
            let within_group = kind == TokenKind::GroupP
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Within);
            let collation_for = kind == TokenKind::For
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Collation);
            let distinct_from = kind == TokenKind::From
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Distinct);
            let is_not_predicate = kind == TokenKind::Not
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Is);
            // Top-level and a stop word (but not one of the special combos) → stop.
            if depth == 0
                && stops.contains(&kind)
                && !within_group
                && !collation_for
                && !distinct_from
                && !is_not_predicate
            {
                break;
            }
            // Bracket depth tracking.  Note: if a closing bracket at depth 0 is
            // itself a stop word, we must break *before* decrementing depth,
            // otherwise we'd incorrectly swallow it.
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 && stops.contains(&kind) {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            out.push(self.advance().clone());
        }
        out
    }

    /// Preserve the synthetic completion marker when a deferred fragment is
    /// handed to another parser. The outer parser keeps its cursor at the
    /// marker; the nested parser receives a clone and shares the collector.
    pub(super) fn append_completion_marker(&self, tokens: &mut Vec<Token>) {
        if self.at_completion() {
            tokens.push(self.peek().clone());
        }
    }

    /// True if the cursor is at a statement boundary (`;` or EOF).
    pub(super) fn at_statement_end(&self) -> bool {
        if self.at_completion() {
            self.record_completion_tokens(&[TokenKind::Char(';')]);
        }
        self.at(TokenKind::Char(';')) || self.at(TokenKind::Eof)
    }

    /// Assert we are at a statement boundary; emit "unexpected token after
    /// statement" otherwise.  Does not consume.
    pub(super) fn expect_statement_end(&self) -> PResult<()> {
        if self.at_statement_end() {
            Ok(())
        } else {
            Err(self.error_here(format!(
                "unexpected token {:?} after statement",
                self.peek_kind()
            )))
        }
    }

    /// LA(1): does the current token equal `kind`?  Does not advance.
    pub(super) fn at(&self, kind: TokenKind) -> bool {
        if self.peek_kind() == TokenKind::Completion {
            self.record_completion_lookahead_tokens(&[kind]);
        }
        self.peek_kind() == kind
    }

    /// LA(1): is the current token one of `kinds`?  Does not advance.
    pub(super) fn at_any(&self, kinds: &[TokenKind]) -> bool {
        if self.peek_kind() == TokenKind::Completion {
            self.record_completion_lookahead_tokens(kinds);
        }
        kinds.contains(&self.peek_kind())
    }

    pub(super) fn at_completion(&self) -> bool {
        self.peek_kind() == TokenKind::Completion
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

    pub(super) fn record_completion_slot(&self, slot: completion::GrammarSlot) {
        if !self.at_completion() {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
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

    pub(super) fn record_completion_slot_before(
        &self,
        slot: completion::GrammarSlot,
        stops: &[TokenKind],
    ) {
        if self.top_level_token_before_completion(stops) != Some(TokenKind::Char('.')) {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
        }
    }

    /// Publish a slot when the completion marker is anywhere inside the
    /// fragment delimited by top-level `stops`, not only at its first token.
    pub(super) fn record_completion_slot_within(
        &self,
        slot: completion::GrammarSlot,
        stops: &[TokenKind],
    ) {
        let follows_fragment_separator = matches!(
            self.top_level_token_before_completion(stops),
            Some(TokenKind::Char('.') | TokenKind::Char(',') | TokenKind::Char('('))
        );
        if !self.at_completion() && !follows_fragment_separator {
            return;
        }
        if let Some(collector) = &self.completion {
            collector.borrow_mut().slot(slot);
        }
    }

    fn top_level_token_before_completion(&self, stops: &[TokenKind]) -> Option<TokenKind> {
        let mut depth = 0usize;
        let mut previous = None;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Completion if depth == 0 => return previous,
                kind if depth == 0 && stops.contains(&kind) => return None,
                kind if depth == 0 => previous = Some(kind),
                _ => {}
            }
        }
        None
    }

    /// Read-ahead (non-consuming): does `needle` appear at the **top level**
    /// before any token in `stops`?
    ///
    /// Uses the same bracket-depth notion as [`take_until_top_level`].
    /// Returns `false` if a stop token appears first, or on EOF.
    /// Useful for aggressive lookahead when dispatching productions.
    pub(super) fn has_top_level_token_before(
        &self,
        needle: TokenKind,
        stops: &[TokenKind],
    ) -> bool {
        let mut depth = 0usize;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    depth = depth.saturating_sub(1);
                }
                kind if depth == 0 && kind == needle => return true,
                kind if depth == 0 && stops.contains(&kind) => return false,
                _ => {}
            }
        }
        false
    }

    /// Optional match: if the current token is `kind`, consume it and return
    /// `true`; otherwise leave the cursor unchanged and return `false`.
    /// Corresponds to "optional / see-one-consume-one" in grammar productions.
    pub(super) fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at_completion() {
            self.record_completion_tokens(&[kind]);
            return false;
        }
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Match an optional token that follows an already complete production.
    /// Unlike `consume`, the token remains hidden from empty-prefix editor
    /// completion and is recovered once the user starts typing it.
    pub(super) fn consume_follow(&mut self, kind: TokenKind) -> bool {
        if self.at_completion() {
            self.record_completion_follow_tokens(&[kind]);
            return false;
        }
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Optional match of a fixed multi-token unit: if the head token matches,
    /// every following token is required. Publishes the whole phrase as one
    /// completion unit so adapters can render `GROUP BY` instead of `GROUP`.
    pub(super) fn consume_phrase(&mut self, phrase: &'static [TokenKind]) -> PResult<bool> {
        self.record_completion_phrase(phrase);
        if !self.consume(phrase[0]) {
            return Ok(false);
        }
        for kind in &phrase[1..] {
            self.expect(*kind)?;
        }
        Ok(true)
    }

    /// Required match: if the current token is `kind`, consume it and return
    /// a clone; otherwise emit a syntax error with the expected vs actual kind.
    /// Corresponds to mandatory tokens in productions.
    pub(super) fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at_completion() {
            self.record_completion_tokens(&[kind]);
            return Err(self.error_here(format!("completion point before required {:?}", kind)));
        }
        if self.at(kind) {
            Ok(self.advance().clone())
        } else {
            Err(self.error_here(format!("expected {:?}, found {:?}", kind, self.peek_kind())))
        }
    }

    /// Unconditionally consume the current token and return a reference to it.
    /// At EOF stays at the last position (no out-of-bounds advance).
    /// This is the lowest-level consume; `consume` / `expect` are built on it.
    pub(super) fn advance(&mut self) -> &Token {
        if !matches!(self.peek_kind(), TokenKind::Eof | TokenKind::Completion) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    /// Reference to the current token (LA(1)).  Does not consume.
    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// The current token's `TokenKind` — the most common lookahead entry point.
    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    /// The `n`-th token's `TokenKind` from the current position (LA(n+1)).
    /// Returns `Eof` on overflow.  For the few productions that need extra
    /// lookahead to disambiguate.
    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    /// Byte offset of the current token in the source text.  Commonly used as
    /// the `location` field on AST nodes.
    pub(super) fn location(&self) -> usize {
        self.peek().location()
    }

    /// Byte offset of the most recently consumed token.  Useful after
    /// `advance` when you still need the start position of the just-parsed
    /// node.  Falls back to [`location`] when the cursor hasn't moved yet.
    pub(super) fn previous_location(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.location())
            .unwrap_or(self.location())
    }

    /// Construct parser control flow anchored at the current token position.
    pub(super) fn error_here(&self, message: impl Into<std::string::String>) -> ParserExit {
        if self.at_completion() && self.completion.is_some() {
            ParserExit::completion(self.peek().range)
        } else {
            ParseError::ranged(self.peek().range, message)
        }
    }
}

// ── Statement dispatch ────────────────────────────────────────────────────
//
// Top-level production: peek LA(1) and dispatch to the matching statement
// parser.  A few keywords are ambiguous and need bounded extra lookahead
// (`peek_kind_n`) to disambiguate before committing to a branch.

macro_rules! define_statement_families {
    ($($family:ident => [$($starter:expr),+ $(,)?]),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum StatementFamily {
            $($family),+
        }

        const STATEMENT_FAMILIES: &[StatementFamily] = &[
            $(StatementFamily::$family),+
        ];

        impl StatementFamily {
            fn starters(self) -> &'static [TokenKind] {
                match self {
                    $(Self::$family => &[$($starter),+]),+
                }
            }

            #[cfg(test)]
            fn coverage_sample(self) -> &'static str {
                match self {
                    Self::With => "WITH x AS (SELECT 1) SELECT * FROM x",
                    Self::Query => "SELECT 1",
                    Self::Insert => "INSERT INTO t VALUES (1)",
                    Self::Update => "UPDATE t SET id = 1",
                    Self::Delete => "DELETE FROM t",
                    Self::Merge => "MERGE INTO t USING s ON true WHEN MATCHED THEN DO NOTHING",
                    Self::Create => "CREATE TABLE t (id int)",
                    Self::Alter => "ALTER TABLE t ADD COLUMN c int",
                    Self::Drop => "DROP TABLE t",
                    Self::SetConstraints => "SET CONSTRAINTS ALL DEFERRED",
                    Self::VariableSet => "SET work_mem = '4MB'",
                    Self::VariableReset => "RESET work_mem",
                    Self::VariableShow => "SHOW work_mem",
                    Self::Transaction => "BEGIN",
                    Self::PrepareTransaction => "PREPARE TRANSACTION 'tx'",
                    Self::Prepare => "PREPARE q AS SELECT 1",
                    Self::Execute => "EXECUTE q",
                    Self::Deallocate => "DEALLOCATE q",
                    Self::Declare => "DECLARE c CURSOR FOR SELECT 1",
                    Self::Close => "CLOSE c",
                    Self::FetchMove => "FETCH NEXT FROM c",
                    Self::Copy => "COPY t FROM STDIN",
                    Self::Vacuum => "ANALYZE t",
                    Self::Explain => "EXPLAIN SELECT 1",
                    Self::Call => "CALL f()",
                    Self::Checkpoint => "CHECKPOINT",
                    Self::Discard => "DISCARD ALL",
                    Self::Lock => "LOCK TABLE t",
                    Self::Listen => "LISTEN channel",
                    Self::Unlisten => "UNLISTEN *",
                    Self::Notify => "NOTIFY channel",
                    Self::Load => "LOAD 'library'",
                    Self::Refresh => "REFRESH MATERIALIZED VIEW mv",
                    Self::Reindex => "REINDEX TABLE t",
                    Self::Repack => "CLUSTER t",
                    Self::Reassign => "REASSIGN OWNED BY old_role TO new_role",
                    Self::Truncate => "TRUNCATE TABLE t",
                    Self::Comment => "COMMENT ON TABLE t IS 'comment'",
                    Self::SecurityLabel => "SECURITY LABEL ON TABLE t IS 'label'",
                    Self::Grant => "GRANT SELECT ON TABLE t TO role_name",
                    Self::Revoke => "REVOKE SELECT ON TABLE t FROM role_name",
                    Self::Import => "IMPORT FOREIGN SCHEMA s FROM SERVER srv INTO public",
                    Self::Do => "DO 'BEGIN END'",
                    Self::Wait => "WAIT FOR LSN '0/0'",
                }
            }
        }
    };
}

define_statement_families! {
    With => [TokenKind::With],
    Query => [TokenKind::Select, TokenKind::Values, TokenKind::Table, TokenKind::Char('(')],
    Insert => [TokenKind::Insert],
    Update => [TokenKind::Update],
    Delete => [TokenKind::DeleteP],
    Merge => [TokenKind::Merge],
    Create => [TokenKind::Create],
    Alter => [TokenKind::Alter],
    Drop => [TokenKind::Drop],
    SetConstraints => [TokenKind::Set],
    VariableSet => [TokenKind::Set],
    VariableReset => [TokenKind::Reset],
    VariableShow => [TokenKind::Show],
    Transaction => [
        TokenKind::BeginP,
        TokenKind::Start,
        TokenKind::Commit,
        TokenKind::EndP,
        TokenKind::Rollback,
        TokenKind::AbortP,
        TokenKind::Savepoint,
        TokenKind::Release,
    ],
    PrepareTransaction => [TokenKind::Prepare],
    Prepare => [TokenKind::Prepare],
    Execute => [TokenKind::Execute],
    Deallocate => [TokenKind::Deallocate],
    Declare => [TokenKind::Declare],
    Close => [TokenKind::Close],
    FetchMove => [TokenKind::Fetch, TokenKind::Move],
    Copy => [TokenKind::Copy],
    Vacuum => [TokenKind::Vacuum, TokenKind::Analyze, TokenKind::Analyse],
    Explain => [TokenKind::Explain],
    Call => [TokenKind::Call],
    Checkpoint => [TokenKind::Checkpoint],
    Discard => [TokenKind::Discard],
    Lock => [TokenKind::LockP],
    Listen => [TokenKind::Listen],
    Unlisten => [TokenKind::Unlisten],
    Notify => [TokenKind::Notify],
    Load => [TokenKind::Load],
    Refresh => [TokenKind::Refresh],
    Reindex => [TokenKind::Reindex],
    Repack => [TokenKind::Cluster, TokenKind::Repack],
    Reassign => [TokenKind::Reassign],
    Truncate => [TokenKind::Truncate],
    Comment => [TokenKind::Comment],
    SecurityLabel => [TokenKind::Security],
    Grant => [TokenKind::Grant],
    Revoke => [TokenKind::Revoke],
    Import => [TokenKind::ImportP],
    Do => [TokenKind::Do],
    Wait => [TokenKind::Wait],
}

fn classify_statement(first: TokenKind, second: TokenKind) -> Option<StatementFamily> {
    Some(match first {
        TokenKind::With => StatementFamily::With,
        TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::Char('(') => {
            StatementFamily::Query
        }
        TokenKind::Insert => StatementFamily::Insert,
        TokenKind::Update => StatementFamily::Update,
        TokenKind::DeleteP => StatementFamily::Delete,
        TokenKind::Merge => StatementFamily::Merge,
        TokenKind::Create => StatementFamily::Create,
        TokenKind::Alter => StatementFamily::Alter,
        TokenKind::Drop => StatementFamily::Drop,
        TokenKind::Set if second == TokenKind::Constraints => StatementFamily::SetConstraints,
        TokenKind::Set => StatementFamily::VariableSet,
        TokenKind::Reset => StatementFamily::VariableReset,
        TokenKind::Show => StatementFamily::VariableShow,
        TokenKind::BeginP
        | TokenKind::Start
        | TokenKind::Commit
        | TokenKind::EndP
        | TokenKind::Rollback
        | TokenKind::AbortP
        | TokenKind::Savepoint
        | TokenKind::Release => StatementFamily::Transaction,
        TokenKind::Prepare if second == TokenKind::Transaction => {
            StatementFamily::PrepareTransaction
        }
        TokenKind::Prepare => StatementFamily::Prepare,
        TokenKind::Execute => StatementFamily::Execute,
        TokenKind::Deallocate => StatementFamily::Deallocate,
        TokenKind::Declare => StatementFamily::Declare,
        TokenKind::Close => StatementFamily::Close,
        TokenKind::Fetch | TokenKind::Move => StatementFamily::FetchMove,
        TokenKind::Copy => StatementFamily::Copy,
        TokenKind::Vacuum | TokenKind::Analyze | TokenKind::Analyse => StatementFamily::Vacuum,
        TokenKind::Explain => StatementFamily::Explain,
        TokenKind::Call => StatementFamily::Call,
        TokenKind::Checkpoint => StatementFamily::Checkpoint,
        TokenKind::Discard => StatementFamily::Discard,
        TokenKind::LockP => StatementFamily::Lock,
        TokenKind::Listen => StatementFamily::Listen,
        TokenKind::Unlisten => StatementFamily::Unlisten,
        TokenKind::Notify => StatementFamily::Notify,
        TokenKind::Load => StatementFamily::Load,
        TokenKind::Refresh => StatementFamily::Refresh,
        TokenKind::Reindex => StatementFamily::Reindex,
        TokenKind::Cluster | TokenKind::Repack => StatementFamily::Repack,
        TokenKind::Reassign => StatementFamily::Reassign,
        TokenKind::Truncate => StatementFamily::Truncate,
        TokenKind::Comment => StatementFamily::Comment,
        TokenKind::Security => StatementFamily::SecurityLabel,
        TokenKind::Grant => StatementFamily::Grant,
        TokenKind::Revoke => StatementFamily::Revoke,
        TokenKind::ImportP => StatementFamily::Import,
        TokenKind::Do => StatementFamily::Do,
        TokenKind::Wait => StatementFamily::Wait,
        _ => return None,
    })
}

impl Parser {
    pub(super) fn parse_statement(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        if self.at_completion() {
            for family in STATEMENT_FAMILIES {
                self.record_completion_tokens(family.starters());
            }
            return Err(self.error_here("completion point at statement start"));
        }
        let first = self.peek_kind();
        let Some(family) = classify_statement(first, self.peek_kind_n(1)) else {
            return Err(self.error_here(format!("unexpected token {:?}", first)));
        };
        match family {
            StatementFamily::With => self.parse_with_statement(),
            StatementFamily::Query => Ok(Node::SelectStmt(self.parse_select(with_clause)?)),
            StatementFamily::Insert => self.parse_insert(with_clause),
            StatementFamily::Update => self.parse_update(with_clause),
            StatementFamily::Delete => self.parse_delete(with_clause),
            StatementFamily::Merge => self.parse_merge(with_clause),
            StatementFamily::Create => self.parse_create(),
            StatementFamily::Alter => self.parse_alter(),
            StatementFamily::Drop => self.parse_drop(),
            StatementFamily::SetConstraints => self.parse_set_constraints(),
            StatementFamily::VariableSet => self.parse_variable_set(),
            StatementFamily::VariableReset => self.parse_variable_reset(),
            StatementFamily::VariableShow => self.parse_variable_show(),
            StatementFamily::Transaction | StatementFamily::PrepareTransaction => {
                self.parse_transaction()
            }
            StatementFamily::Prepare => self.parse_prepare(),
            StatementFamily::Execute => self.parse_execute(),
            StatementFamily::Deallocate => self.parse_deallocate(),
            StatementFamily::Declare => self.parse_declare_cursor(),
            StatementFamily::Close => self.parse_close(),
            StatementFamily::FetchMove => self.parse_fetch_or_move(),
            StatementFamily::Copy => self.parse_copy(),
            StatementFamily::Vacuum => self.parse_vacuum(),
            StatementFamily::Explain => self.parse_explain(),
            StatementFamily::Call => self.parse_call(),
            StatementFamily::Checkpoint => self.parse_checkpoint(),
            StatementFamily::Discard => self.parse_discard(),
            StatementFamily::Lock => self.parse_lock(),
            StatementFamily::Listen => self.parse_listen(),
            StatementFamily::Unlisten => self.parse_unlisten(),
            StatementFamily::Notify => self.parse_notify(),
            StatementFamily::Load => self.parse_load(),
            StatementFamily::Refresh => self.parse_refresh(),
            StatementFamily::Reindex => self.parse_reindex(),
            StatementFamily::Repack => self.parse_repack(),
            StatementFamily::Reassign => self.parse_reassign_owned(),
            StatementFamily::Truncate => self.parse_truncate(),
            StatementFamily::Comment => self.parse_comment(),
            StatementFamily::SecurityLabel => self.parse_security_label(),
            StatementFamily::Grant => self.parse_grant(true),
            StatementFamily::Revoke => self.parse_grant(false),
            StatementFamily::Import => self.parse_import_foreign_schema(),
            StatementFamily::Do => self.parse_do(),
            StatementFamily::Wait => self.parse_wait(),
        }
    }
}

// ── Integration tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn first_node(sql: &str) -> Node {
        let stmt = parse_one(sql).unwrap();
        *stmt.stmt.unwrap()
    }

    #[test]
    fn parses_basic_select_insert_update_delete() {
        assert!(matches!(
            first_node("select a, b from t where id = 1"),
            Node::SelectStmt(_)
        ));
        assert!(matches!(
            first_node("insert into t (a) values (1) returning a"),
            Node::InsertStmt(_)
        ));
        assert!(matches!(
            first_node("update t set a = 1 where id = 2"),
            Node::UpdateStmt(_)
        ));
        assert!(matches!(
            first_node("delete from t where id = 3"),
            Node::DeleteStmt(_)
        ));
    }

    #[test]
    fn parses_multiple_raw_statements() {
        let stmts = parse("select 1; select 2;").unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            *stmts[0].stmt.clone().unwrap(),
            Node::SelectStmt(_)
        ));
        assert!(matches!(
            *stmts[1].stmt.clone().unwrap(),
            Node::SelectStmt(_)
        ));
    }

    #[test]
    fn optional_consume_helpers_do_not_advance_when_they_return_none() {
        let mut setting = Parser::new("foo.").unwrap();
        let start = setting.pos;
        assert_eq!(setting.consume_setting_name(), None);
        assert_eq!(setting.pos, start);

        let mut role = Parser::new("none").unwrap();
        let start = role.pos;
        assert_eq!(role.consume_role_spec(), None);
        assert_eq!(role.pos, start);

        let mut object_type = Parser::new("text search unknown").unwrap();
        let start = object_type.pos;
        assert_eq!(object_type.consume_object_type(), None);
        assert_eq!(object_type.pos, start);
    }

    #[test]
    fn parses_common_create_alter_drop_forms() {
        assert!(matches!(
            first_node("create table s.t (id int, name text)"),
            Node::CreateStmt(_)
        ));
        assert!(matches!(
            first_node("create unique index idx on t (id)"),
            Node::IndexStmt(_)
        ));
        assert!(matches!(
            first_node("create view v as select 1"),
            Node::ViewStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t add column x int"),
            Node::AlterTableStmt(_)
        ));
        assert!(matches!(
            first_node("drop table if exists t cascade"),
            Node::DropStmt(_)
        ));
    }

    #[test]
    fn parses_utility_statements() {
        let cases = [
            ("set search_path to public", "set"),
            ("show search_path", "show"),
            ("begin", "begin"),
            ("commit", "commit"),
            ("prepare q as select 1", "prepare"),
            ("execute q", "execute"),
            ("deallocate q", "deallocate"),
            ("explain select 1", "explain"),
            ("copy t from 'file.csv'", "copy"),
            ("vacuum t", "vacuum"),
            ("call f(1)", "call"),
            ("listen chan", "listen"),
            ("notify chan, 'payload'", "notify"),
        ];
        for (sql, label) in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{label}: {err}"));
        }
    }

    #[test]
    fn dispatches_broad_statement_family() {
        let cases = [
            "create schema s",
            "create database d",
            "create extension e",
            "create role r",
            "create sequence s",
            "create domain d as int",
            "create type mood as enum ('sad','ok')",
            "create publication p",
            "create subscription s connection 'x' publication p",
            "drop database if exists d",
            "drop role if exists r",
            "drop owned by r",
            "truncate table t",
            "comment on table t is 'x'",
            "security label on table t is 'x'",
            "grant select on table t to r",
            "revoke select on table t from r",
            "refresh materialized view mv",
            "reindex table t",
            "discard all",
            "lock table t",
            "load 'x'",
            "wait for lsn '0/0'",
        ];
        for sql in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
        }
    }

    #[test]
    fn builds_expression_ast_for_common_precedence() {
        let Node::SelectStmt(stmt) =
            first_node("select a + 1 * 2 from t where b::int >= 3 and not c")
        else {
            panic!("expected select");
        };
        let Node::ResTarget(target) = &stmt.target_list[0] else {
            panic!("expected target");
        };
        assert!(matches!(target.val.as_deref(), Some(Node::AExpr(_))));
        assert!(matches!(
            stmt.where_clause.as_deref(),
            Some(Node::BoolExpr(_))
        ));
    }

    #[test]
    fn dispatches_official_top_level_statement_families() {
        let cases = [
            "alter event trigger et disable",
            "alter collation c refresh version",
            "alter database d refresh collation version",
            "alter database d set search_path to public",
            "alter default privileges grant select on tables to r",
            "alter domain d set default 1",
            "alter type mood add value 'ok'",
            "alter extension e add table t",
            "alter foreign data wrapper fdw options (foo 'bar')",
            "alter server s options (foo 'bar')",
            "alter function f() stable",
            "alter group g add user u",
            "alter function f() depends on extension e",
            "alter table t set schema s",
            "alter table t owner to r",
            "alter operator +(int, int) set (commutator = +)",
            "alter type t set (receive = r)",
            "alter policy p on t using (true)",
            "alter property graph g add vertex tables (t)",
            "alter sequence s restart",
            "alter system set work_mem = '4MB'",
            "alter table t add column c int",
            "alter tablespace ts set (random_page_cost = 2)",
            "alter type ct add attribute a int",
            "alter publication p set table t",
            "alter role r set search_path to public",
            "alter subscription s refresh publication",
            "alter statistics st set statistics 10",
            "alter text search dictionary d (template = simple)",
            "alter user mapping for u server s options (foo 'bar')",
            "analyze t",
            "call f(1)",
            "checkpoint",
            "close c",
            "comment on table t is 'x'",
            "set constraints all deferred",
            "copy t from 'file.csv'",
            "create access method am type table handler h",
            "create table ct_as as select 1",
            "create cast (int as text) without function",
            "create conversion conv for 'utf8' to 'latin1' from f",
            "create domain d as int",
            "create extension e",
            "create foreign data wrapper fdw",
            "create server s foreign data wrapper fdw",
            "create foreign table ft (id int) server s",
            "create function f() returns int language sql as 'select 1'",
            "create group g",
            "create materialized view mv as select 1",
            "create operator class opc for type int using btree as operator 1 =",
            "create operator family opf using btree",
            "alter operator family opf using btree add operator 1 =(int,int)",
            "create policy p on t using (true)",
            "create language plpgsql handler plpgsql_call_handler",
            "create property graph g vertex tables (t)",
            "create schema s",
            "create sequence seq",
            "create table t (id int)",
            "create subscription sub connection 'c' publication p",
            "create statistics st on a from t",
            "create tablespace ts location '/tmp'",
            "create transform for int language plpgsql (from sql with function f(int))",
            "create trigger tr before insert on t execute function f()",
            "create event trigger et on ddl_command_start execute function f()",
            "create role r",
            "create user u",
            "create user mapping for u server s",
            "create database d",
            "deallocate q",
            "declare c cursor for select 1",
            "create aggregate agg(int) (sfunc = f, stype = int)",
            "delete from t where id = 1",
            "discard all",
            "do 'begin end'",
            "drop cast (int as text)",
            "drop operator class opc using btree",
            "drop operator family opf using btree",
            "drop owned by r",
            "drop table if exists t",
            "drop subscription if exists sub",
            "drop tablespace if exists ts",
            "drop transform for int language plpgsql",
            "drop role if exists r",
            "drop user mapping if exists for u server s",
            "drop database if exists d",
            "execute q",
            "explain select 1",
            "fetch next from c",
            "grant select on table t to r",
            "grant r to u",
            "import foreign schema s from server srv into public",
            "create index idx on t (id)",
            "insert into t values (1)",
            "listen ch",
            "refresh materialized view mv",
            "load 'x'",
            "lock table t",
            "merge into t using s on t.id = s.id when matched then update set id = s.id",
            "notify ch, 'payload'",
            "prepare q as select 1",
            "reassign owned by r to u",
            "reindex table t",
            "drop aggregate if exists agg(int)",
            "drop function if exists f()",
            "drop operator if exists +(int, int)",
            "alter table t rename to t2",
            "repack t using index idx",
            "revoke select on table t from r",
            "revoke r from u",
            "create rule r as on update to t do notify ch",
            "security label on table t is 'x'",
            "select 1",
            "begin",
            "truncate table t",
            "unlisten *",
            "update t set id = 2",
            "vacuum t",
            "reset search_path",
            "set search_path to public",
            "show search_path",
            "create view v as select 1",
            "wait for lsn '0/0'",
        ];

        for sql in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
        }
    }

    #[test]
    fn dispatches_specific_extended_statement_nodes() {
        assert!(matches!(
            first_node("create table t as select 1"),
            Node::CreateTableAsStmt(_)
        ));
        assert!(matches!(
            first_node("create foreign data wrapper fdw"),
            Node::CreateFdwStmt(_)
        ));
        assert!(matches!(
            first_node("create property graph g vertex tables (t)"),
            Node::CreatePropGraphStmt(_)
        ));
        assert!(matches!(
            first_node("alter extension e add table t"),
            Node::AlterExtensionContentsStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t set schema s"),
            Node::AlterObjectSchemaStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t owner to r"),
            Node::AlterTableStmt(AlterTableStmt { cmds, .. })
                if matches!(cmds.first(), Some(Node::AlterTableCmd(AlterTableCmd {
                    subtype: AlterTableType::ChangeOwner,
                    ..
                })))
        ));
        assert!(matches!(
            first_node("alter role r set search_path to public"),
            Node::AlterRoleSetStmt(_)
        ));
        assert!(matches!(
            first_node("alter type ct add attribute a int"),
            Node::AlterTableStmt(AlterTableStmt {
                objtype: ObjectType::Type,
                ..
            })
        ));
        assert!(matches!(
            first_node("drop cast (int as text)"),
            Node::DropStmt(DropStmt {
                remove_type: ObjectType::Cast,
                ..
            })
        ));
        assert!(matches!(
            first_node("create rule r as on update to t do notify ch"),
            Node::RuleStmt(_)
        ));
        assert!(matches!(first_node("repack t"), Node::RepackStmt(_)));
        assert!(matches!(
            first_node("create recursive view v (n) as select 1"),
            Node::ViewStmt(_)
        ));
    }

    #[test]
    fn fills_complex_create_and_alter_fields() {
        let Node::CreateCastStmt(cast) =
            first_node("create cast (int as text) with inout as assignment")
        else {
            panic!("expected cast");
        };
        assert!(cast.sourcetype.is_some());
        assert!(cast.targettype.is_some());
        assert!(cast.inout);
        assert_eq!(cast.context, CoercionContext::Assignment);

        let Node::CreateForeignServerStmt(server) = first_node(
            "create server if not exists s type 't' version '1' foreign data wrapper fdw options (host 'x')",
        ) else {
            panic!("expected server");
        };
        assert_eq!(server.servername.as_deref(), Some("s"));
        assert_eq!(server.fdwname.as_deref(), Some("fdw"));
        assert!(server.if_not_exists);
        assert!(!server.options.is_empty());

        let Node::CreatePolicyStmt(policy) =
            first_node("create policy p on t for select to r using (id > 0) with check (id > 0)")
        else {
            panic!("expected policy");
        };
        assert_eq!(policy.policy_name.as_deref(), Some("p"));
        assert!(policy.table.is_some());
        assert!(policy.qual.is_some());
        assert!(policy.with_check.is_some());

        let Node::AlterPolicyStmt(policy) = first_node("alter policy p on t to r using (id > 1)")
        else {
            panic!("expected alter policy");
        };
        assert_eq!(policy.policy_name.as_deref(), Some("p"));
        assert!(policy.table.is_some());
        assert!(policy.qual.is_some());

        let Node::SelectStmt(select) = first_node(
            "select * from (select 1) s join f(1) g on true window w as (partition by a order by b) order by a fetch first 2 rows with ties for update of s nowait",
        ) else {
            panic!("expected select");
        };
        assert!(matches!(
            select.from_clause.first(),
            Some(Node::JoinExpr(_))
        ));
        assert!(!select.window_clause.is_empty());
        assert!(!select.locking_clause.is_empty());
        assert_eq!(select.limit_option, LimitOption::WithTies);

        let Node::AlterTableStmt(alter) = first_node(
            "alter table t add column c int, alter column c set default 1, drop column if exists d cascade",
        ) else {
            panic!("expected alter table");
        };
        assert_eq!(alter.cmds.len(), 3);
        assert!(matches!(
            alter.cmds.first(),
            Some(Node::AlterTableCmd(AlterTableCmd {
                subtype: AlterTableType::AddColumn,
                ..
            }))
        ));
    }
}
