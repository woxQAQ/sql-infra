use crate::ast::*;
use crate::completion::{ColumnContext, Expectation, NameExpectation, SharedCompletionRecorder};
use crate::lexer::{Token, TokenValue, lex, lookup_keyword};
use crate::{BareLabel, KeywordCategory, TextRange, TextSize, TokenKind};
use std::ops::Range;
use std::rc::Rc;

mod access_method;
mod aggregate_signatures;
mod alter;
mod alter_collation;
mod alter_identity;
mod alter_table;
mod alter_table_partition;
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
use expression_json::{default_json_format, json_behavior_starts};
use fragment_parser::*;
use function_parameters::*;
use object_helpers::*;
use settings::{parse_setting_value_tokens, parse_time_zone_value_tokens};
use token_helpers::*;
use type_tokens::*;
use xmltable_columns::*;

// ── ParseError ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub message: std::string::String,
    pub range: TextRange,
}

impl ParseError {
    pub(super) fn new(location: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            range: TextRange::empty(
                TextSize::try_from(location).expect("parser locations come from validated input"),
            ),
            message: message.into(),
        }
    }

    pub(super) fn ranged(range: TextRange, message: impl Into<std::string::String>) -> Self {
        Self {
            range,
            message: message.into(),
        }
    }

    pub fn location(&self) -> usize {
        self.range.start().into()
    }

    pub(super) fn reanchor(&mut self, location: usize) {
        self.range = TextRange::empty(
            TextSize::try_from(location).expect("parser locations come from validated input"),
        );
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

type PResult<T> = Result<T, ParseError>;
type JsonBehaviorPair = (Option<Box<JsonBehavior>>, Option<Box<JsonBehavior>>);

pub fn parse(sql: &str) -> PResult<Vec<RawStmt>> {
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
pub fn parse_with_ranges(sql: &str) -> PResult<Vec<ParsedStatement>> {
    Parser::new(sql)?.parse_with_ranges()
}

pub fn parse_one(sql: &str) -> PResult<RawStmt> {
    let mut stmts = parse(sql)?;
    if stmts.len() != 1 {
        return Err(ParseError::new(
            stmts.get(1).map_or(0, |stmt| stmt.stmt_location as usize),
            format!("expected one statement, found {}", stmts.len()),
        ));
    }
    Ok(stmts.remove(0))
}

pub fn parse_plpgsql_assignment(sql: &str, nnames: i32) -> PResult<RawStmt> {
    plpgsql::parse_assignment(sql, nnames)
}

pub fn parse_plpgsql_expression(sql: &str) -> PResult<RawStmt> {
    plpgsql::parse_expression(sql)
}

pub fn parse_type_name(sql: &str) -> PResult<TypeName> {
    let mut tokens = lex(sql)?;
    tokens.pop();
    parse_type_name_tokens(tokens)
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
    // Every parser is a bounded `[start, end)` view over shared token storage.
    // `eof` is virtual and never inserted into `tokens`; nested views inherit
    // the completion recorder, whose cursor location distinguishes the real
    // completion boundary from ordinary fragment boundaries.
    tokens: Rc<[Token]>,
    start: usize,
    pos: usize,
    end: usize,
    eof: Token,
    completion: Option<SharedCompletionRecorder>,
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
    pub fn new(sql: &str) -> PResult<Self> {
        Ok(Self::from_tokens(lex(sql)?, None))
    }

    pub(crate) fn for_completion(tokens: Vec<Token>, recorder: SharedCompletionRecorder) -> Self {
        Self::from_tokens(tokens, Some(recorder))
    }

    /// Build a new root parser for a deliberately transformed token stream.
    /// Ordinary nested grammar must use [`Self::bounded_view`] instead.
    pub(super) fn from_transformed_tokens(tokens: Vec<Token>) -> Self {
        Self::from_tokens(tokens, None)
    }

    pub(super) fn from_shared_range(
        tokens: Rc<[Token]>,
        range: Range<usize>,
        eof_location: usize,
        completion: Option<SharedCompletionRecorder>,
    ) -> Self {
        assert!(range.start <= range.end && range.end <= tokens.len());
        Self {
            tokens,
            start: range.start,
            pos: range.start,
            end: range.end,
            eof: Token::synthetic(TokenKind::Eof, eof_location),
            completion,
        }
    }

    fn from_tokens(mut tokens: Vec<Token>, completion: Option<SharedCompletionRecorder>) -> Self {
        let eof = match tokens.last() {
            Some(token) if token.kind == TokenKind::Eof => token.clone(),
            Some(token) => Token::synthetic(TokenKind::Eof, token.end_location()),
            None => Token::synthetic(TokenKind::Eof, 0),
        };
        if let Some(recorder) = &completion {
            recorder.borrow_mut().set_cursor(eof.location());
        }
        if tokens
            .last()
            .is_some_and(|token| token.kind == TokenKind::Eof)
        {
            tokens.pop();
        }
        let end = tokens.len();
        Self {
            tokens: Rc::from(tokens),
            start: 0,
            pos: 0,
            end,
            eof,
            completion,
        }
    }

    pub(super) fn bounded_view(&self, range: Range<usize>) -> Self {
        assert!(
            self.start <= range.start && range.start <= range.end && range.end <= self.end,
            "parser view must be contained in its parent"
        );
        let eof_location = self
            .tokens
            .get(range.end)
            .map_or_else(|| self.eof.location(), Token::location);
        Self {
            tokens: self.tokens.clone(),
            start: range.start,
            pos: range.start,
            end: range.end,
            eof: Token::synthetic(TokenKind::Eof, eof_location),
            completion: self.completion.clone(),
        }
    }

    fn expression_view(&self, range: Range<usize>) -> ExprParser {
        self.expression_view_with_completion(range, self.completion.clone())
    }

    fn expression_view_without_completion(&self, range: Range<usize>) -> ExprParser {
        self.expression_view_with_completion(range, None)
    }

    fn expression_view_with_completion(
        &self,
        range: Range<usize>,
        completion: Option<SharedCompletionRecorder>,
    ) -> ExprParser {
        assert!(
            self.start <= range.start && range.start <= range.end && range.end <= self.end,
            "expression view must be contained in its parent"
        );
        let eof_location = self
            .tokens
            .get(range.end)
            .map_or_else(|| self.eof.location(), Token::location);
        ExprParser::from_shared_range(self.tokens.clone(), range, eof_location, completion)
    }

    pub(super) fn expression_range_is_valid(&self, range: Range<usize>) -> bool {
        !range.is_empty()
            && self
                .expression_view_without_completion(range)
                .parse()
                .is_ok()
    }

    pub(super) fn parse_expression_range(&self, range: Range<usize>) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            return Err(ParseError::new(location, "expected an expression"));
        }
        self.expression_view(range).parse().map_err(|mut error| {
            if error.location() == 0 {
                error.reanchor(location);
            }
            error
        })
    }

    pub(super) fn parse_b_expression_range(&self, range: Range<usize>) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            return Err(ParseError::new(
                location,
                "expected a restricted expression",
            ));
        }
        self.expression_view(range).parse_b().map_err(|mut error| {
            if error.location() == 0 {
                error.reanchor(location);
            }
            error
        })
    }

    pub(super) fn parse_c_expression_range(&self, range: Range<usize>) -> PResult<Node> {
        let location = self
            .tokens
            .get(range.start)
            .map_or_else(|| self.location(), Token::location);
        if range.is_empty() {
            if self.at_completion_cursor() {
                self.record_expression_completion();
            }
            return Err(ParseError::new(location, "expected a common expression"));
        }
        self.expression_view(range).parse_c().map_err(|mut error| {
            if error.location() == 0 {
                error.reanchor(location);
            }
            error
        })
    }

    pub(super) fn split_explicit_alias_range(
        &self,
        range: Range<usize>,
    ) -> (Option<std::string::String>, Range<usize>) {
        let tokens = &self.tokens[range.clone()];
        let mut depth = 0usize;
        let mut alias_index = None;
        for (index, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                TokenKind::As if depth == 0 => alias_index = Some(index),
                _ => {}
            }
        }
        if let Some(index) = alias_index
            && index + 2 == tokens.len()
            && let Some(alias) = tokens.get(index + 1)
        {
            let accepted = matches!(alias.kind, TokenKind::Ident | TokenKind::UIdent)
                || match &alias.value {
                    Some(TokenValue::Keyword(word)) => lookup_keyword(word).is_some(),
                    _ => false,
                };
            if accepted && let Some(name) = token_name(alias) {
                return (Some(name), range.start..range.start + index);
            }
        }
        (None, range)
    }

    pub(crate) fn parse_completion_statement(&mut self) -> PResult<Node> {
        self.parse_statement(None)
    }

    pub fn parse(&mut self) -> PResult<Vec<RawStmt>> {
        Ok(self
            .parse_with_ranges()?
            .into_iter()
            .map(|statement| statement.raw)
            .collect())
    }

    pub fn parse_with_ranges(&mut self) -> PResult<Vec<ParsedStatement>> {
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
        let range = self.take_until_top_level_range(stops);
        self.tokens[range].to_vec()
    }

    /// Advance to the same boundary as [`Self::take_until_top_level`] but
    /// return a bounded range into the shared token buffer instead of cloning
    /// the fragment. The range can be passed to [`Self::bounded_view`].
    pub(super) fn take_until_top_level_range(&mut self, stops: &[TokenKind]) -> Range<usize> {
        let start = self.pos;
        let mut depth = 0usize;
        let mut previous = None;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            // Two-word combinations that need the previous token to disambiguate.
            let within_group = kind == TokenKind::GroupP && previous == Some(TokenKind::Within);
            let collation_for = kind == TokenKind::For && previous == Some(TokenKind::Collation);
            let distinct_from = kind == TokenKind::From && previous == Some(TokenKind::Distinct);
            let is_not_predicate = kind == TokenKind::Not && previous == Some(TokenKind::Is);
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
            previous = Some(kind);
            self.advance();
        }
        start..self.pos
    }

    /// True if the cursor is at a statement boundary (`;` or EOF).
    pub(super) fn at_statement_end(&self) -> bool {
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
        self.peek_kind() == kind
    }

    /// LA(1): is the current token one of `kinds`?  Does not advance.
    pub(super) fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek_kind())
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
        for token in &self.tokens[self.pos..self.end] {
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
        if self.at_completion_cursor() {
            self.record_completion(Expectation::Token(kind));
            return false;
        }
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Required match: if the current token is `kind`, consume it and return
    /// a clone; otherwise emit a syntax error with the expected vs actual kind.
    /// Corresponds to mandatory tokens in productions.
    pub(super) fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at_completion_cursor() {
            self.record_completion(Expectation::Token(kind));
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
        if self.pos < self.end {
            let consumed = self.pos;
            self.pos += 1;
            &self.tokens[consumed]
        } else {
            &self.eof
        }
    }

    /// Reference to the current token (LA(1)).  Does not consume.
    pub(super) fn peek(&self) -> &Token {
        if self.pos < self.end {
            &self.tokens[self.pos]
        } else {
            &self.eof
        }
    }

    /// The current token's `TokenKind` — the most common lookahead entry point.
    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    /// The `n`-th token's `TokenKind` from the current position (LA(n+1)).
    /// Returns `Eof` on overflow.  For the few productions that need extra
    /// lookahead to disambiguate.
    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        let index = self.pos.saturating_add(n);
        if index < self.end {
            self.tokens[index].kind
        } else {
            TokenKind::Eof
        }
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
        if self.pos > self.start {
            self.tokens[self.pos - 1].location()
        } else {
            self.location()
        }
    }

    /// Construct a `ParseError` anchored at the current token position.
    /// The single entry point for all parser error reporting.
    pub(super) fn error_here(&self, message: impl Into<std::string::String>) -> ParseError {
        ParseError::ranged(self.peek().range, message)
    }

    pub(super) fn at_completion_cursor(&self) -> bool {
        self.at(TokenKind::Eof)
            && self
                .completion
                .as_ref()
                .is_some_and(|recorder| recorder.borrow().is_cursor(self.location()))
    }

    pub(super) fn record_completion(&self, expectation: Expectation) {
        if let Some(recorder) = &self.completion {
            recorder.borrow_mut().record(expectation);
        }
    }

    pub(super) fn record_expression_completion(&self) {
        self.record_completion(Expectation::Expression);
        self.record_completion(Expectation::Name(NameExpectation::Column(
            ColumnContext::VisibleScope,
        )));
        self.record_completion(Expectation::Name(NameExpectation::Function {
            schema: None,
        }));
    }

    pub(super) fn record_relation_completion(&self) {
        self.record_completion(Expectation::Name(NameExpectation::Relation {
            schema: None,
        }));
        self.record_completion(Expectation::Name(NameExpectation::Schema));
    }
}

// ── Statement dispatch ────────────────────────────────────────────────────
//
// Top-level production: peek LA(1) and dispatch to the matching statement
// parser.  A few keywords are ambiguous and need bounded extra lookahead
// (`peek_kind_n`) to disambiguate before committing to a branch.

impl Parser {
    pub(super) fn parse_statement(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        if self.at_completion_cursor() {
            for token in [
                TokenKind::Select,
                TokenKind::With,
                TokenKind::Insert,
                TokenKind::Update,
                TokenKind::DeleteP,
                TokenKind::Create,
                TokenKind::Alter,
                TokenKind::Drop,
                TokenKind::Values,
                TokenKind::Explain,
            ] {
                self.record_completion(Expectation::Token(token));
            }
            return Err(self.error_here("completion cursor"));
        }
        match self.peek_kind() {
            TokenKind::With => self.parse_with_statement(),
            TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::Char('(') => {
                Ok(Node::SelectStmt(self.parse_select(with_clause)?))
            }
            TokenKind::Insert => self.parse_insert(with_clause),
            TokenKind::Update => self.parse_update(with_clause),
            TokenKind::DeleteP => self.parse_delete(with_clause),
            TokenKind::Merge => self.parse_merge(with_clause),
            TokenKind::Create => self.parse_create(),
            TokenKind::Alter => self.parse_alter(),
            TokenKind::Drop => self.parse_drop(),
            TokenKind::Set if self.peek_kind_n(1) == TokenKind::Constraints => {
                self.parse_set_constraints()
            }
            TokenKind::Set => self.parse_variable_set(),
            TokenKind::Reset => self.parse_variable_reset(),
            TokenKind::Show => self.parse_variable_show(),
            TokenKind::BeginP
            | TokenKind::Start
            | TokenKind::Commit
            | TokenKind::EndP
            | TokenKind::Rollback
            | TokenKind::AbortP
            | TokenKind::Savepoint
            | TokenKind::Release => self.parse_transaction(),
            TokenKind::Prepare if self.peek_kind_n(1) == TokenKind::Transaction => {
                self.parse_transaction()
            }
            TokenKind::Prepare => self.parse_prepare(),
            TokenKind::Execute => self.parse_execute(),
            TokenKind::Deallocate => self.parse_deallocate(),
            TokenKind::Declare => self.parse_declare_cursor(),
            TokenKind::Close => self.parse_close(),
            TokenKind::Fetch | TokenKind::Move => self.parse_fetch_or_move(),
            TokenKind::Copy => self.parse_copy(),
            TokenKind::Vacuum | TokenKind::Analyze | TokenKind::Analyse => self.parse_vacuum(),
            TokenKind::Explain => self.parse_explain(),
            TokenKind::Call => self.parse_call(),
            TokenKind::Checkpoint => self.parse_checkpoint(),
            TokenKind::Discard => self.parse_discard(),
            TokenKind::LockP => self.parse_lock(),
            TokenKind::Listen => self.parse_listen(),
            TokenKind::Unlisten => self.parse_unlisten(),
            TokenKind::Notify => self.parse_notify(),
            TokenKind::Load => self.parse_load(),
            TokenKind::Refresh => self.parse_refresh(),
            TokenKind::Reindex => self.parse_reindex(),
            TokenKind::Cluster | TokenKind::Repack => self.parse_repack(),
            TokenKind::Reassign => self.parse_reassign_owned(),
            TokenKind::Truncate => self.parse_truncate(),
            TokenKind::Comment => self.parse_comment(),
            TokenKind::Security => self.parse_security_label(),
            TokenKind::Grant => self.parse_grant(true),
            TokenKind::Revoke => self.parse_grant(false),
            TokenKind::ImportP => self.parse_import_foreign_schema(),
            TokenKind::Do => self.parse_do(),
            TokenKind::Wait => self.parse_wait(),
            other => Err(self.error_here(format!("unexpected token {:?}", other))),
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
    fn bounded_view_uses_virtual_eof_without_reading_parent_tokens() {
        let parent = Parser::new("select value").unwrap();
        let mut nested = parent.bounded_view(0..1);

        assert_eq!(nested.advance().kind, TokenKind::Select);
        assert_eq!(nested.peek_kind(), TokenKind::Eof);
        assert_eq!(nested.peek_kind_n(1), TokenKind::Eof);
        assert_eq!(parent.peek_kind(), TokenKind::Select);
        assert_ne!(parent.peek_kind_n(1), TokenKind::Eof);
    }

    #[test]
    fn empty_bounded_view_anchors_eof_at_its_boundary() {
        let parent = Parser::new("select value").unwrap();
        let boundary = parent.tokens[1].location();
        let nested = parent.bounded_view(1..1);

        assert_eq!(nested.peek_kind(), TokenKind::Eof);
        assert_eq!(nested.location(), boundary);
    }

    #[test]
    fn bounded_view_shares_completion_but_only_records_at_the_real_cursor() {
        let tokens = lex("select value").unwrap();
        let recorder = Rc::new(std::cell::RefCell::new(
            crate::completion::CompletionRecorder::default(),
        ));
        let parent = Parser::for_completion(tokens, recorder.clone());
        let artificial_eof = parent.bounded_view(0..1);
        let cursor_eof = parent.bounded_view(parent.end..parent.end);

        assert!(Rc::ptr_eq(
            artificial_eof.completion.as_ref().unwrap(),
            &recorder
        ));
        assert!(!artificial_eof.at_completion_cursor());
        assert!(cursor_eof.at_completion_cursor());
    }

    #[test]
    fn expression_view_shares_tokens_and_cannot_read_its_suffix() {
        let parent = Parser::new("1 + 2, 3").unwrap();
        let mut expression = parent.expression_view(0..3);

        assert!(Rc::ptr_eq(&parent.tokens, &expression.tokens));
        assert!(matches!(expression.parse_expr(0), Some(Node::AExpr(_))));
        assert_eq!(expression.peek_kind(), TokenKind::Eof);
        assert_eq!(parent.peek_kind(), TokenKind::IConst);
        assert_eq!(parent.tokens[3].kind, TokenKind::Char(','));
    }

    #[test]
    fn shared_nested_parser_views_cover_statement_and_expression_fragments() {
        for sql in [
            "with x as (select 1) select * from x",
            "select * from (a join b on a.id = b.id) j",
            "create rule r as on update to t do (notify ch; notify ch2)",
            "create table t (id int primary key, x text)",
            "copy t from 'f' with (format csv, header true)",
            "select sum(x) over (partition by y order by z rows between 1 preceding and current row) from t",
        ] {
            parse_one(sql).unwrap_or_else(|error| panic!("{sql}: {error}"));
        }
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
