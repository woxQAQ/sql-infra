use pg_parser::GrammarSlot;
use pg_parser::KEYWORDS;
use pg_parser::KeywordCategory;
use pg_parser::Node;
use pg_parser::ParseError;
use pg_parser::ParserExpectations;
use pg_parser::TextSize;
use pg_parser::TokenKind;
use pg_parser::collect_expectations;
use pg_parser::lex;
use pg_parser::parse_one;

#[derive(Clone, Copy)]
pub struct StatementCase {
    pub expected_name: &'static str,
    pub expected: fn(&Node) -> bool,
    pub sql: &'static str,
}

pub fn parse_statement(sql: &str) -> Node {
    let raw = parse_one(sql).unwrap_or_else(|error| panic!("failed to parse {sql:?}: {error}"));
    assert_completion_boundaries(sql);
    *raw.stmt
        .unwrap_or_else(|| panic!("parser returned an empty RawStmt for {sql:?}"))
}

/// Every successful statement parser test is also a completion grammar
/// witness. This keeps completion coverage growing with parser coverage and
/// catches productions that parse a token but fail to publish it at the same
/// cursor boundary.
fn assert_completion_boundaries(sql: &str) {
    let tokens = lex(sql).unwrap_or_else(|error| panic!("failed to lex {sql:?}: {error}"));

    for token in tokens
        .iter()
        .filter(|token| !matches!(token.kind, TokenKind::Eof | TokenKind::Char(';')))
    {
        let before = collect_expectations(sql, token.range.start()).unwrap_or_else(|error| {
            panic!(
                "completion failed before {:?} at byte {} in {sql:?}: {error}",
                token.kind,
                usize::from(token.range.start())
            )
        });
        assert_expectation_provenance(sql, token.range.start(), &before);
        assert_token_position_is_reachable(sql, token, &before);

        let after = collect_expectations(sql, token.range.end()).unwrap_or_else(|error| {
            panic!(
                "completion failed after {:?} at byte {} in {sql:?}: {error}",
                token.kind,
                usize::from(token.range.end())
            )
        });
        assert_expectation_provenance(sql, token.range.end(), &after);
    }
}

fn assert_token_position_is_reachable(
    sql: &str,
    token: &pg_parser::Token,
    expectations: &ParserExpectations,
) {
    let is_identifier = matches!(token.kind, TokenKind::Ident | TokenKind::UIdent);
    let keyword = KEYWORDS.iter().find(|keyword| keyword.kind == token.kind);
    if !is_identifier && keyword.is_none() {
        return;
    }

    let token_reachable = expectations.tokens.contains(&token.kind);
    let can_be_name = is_identifier
        || keyword.is_some_and(|keyword| keyword.category != KeywordCategory::Reserved);
    let is_special_role = matches!(
        token.kind,
        TokenKind::CurrentRole | TokenKind::CurrentUser | TokenKind::SessionUser
    );
    let name_reachable = (can_be_name && !expectations.slots.is_empty())
        // Role is a semantic grammar slot. Some productions accept the
        // tokenized special role forms without making them eager keyword
        // suggestions at an otherwise empty role-name position.
        || (is_special_role && expectations.slots.contains(&GrammarSlot::Role))
        || expectations.slots.contains(&GrammarSlot::AnyName)
        // Output aliases use PostgreSQL's ColLabel production, which accepts
        // reserved keywords as user-written names. Alias is intentionally a
        // slot rather than a keyword candidate: an editor should not suggest
        // every keyword as an alias merely because the spelling is legal.
        || expectations.slots.contains(&GrammarSlot::Alias);
    let qualified_name_reachable = sql[..usize::from(token.range.start())]
        .trim_end()
        .ends_with('.')
        && !expectations.slots.is_empty();
    if !token_reachable && !name_reachable && !qualified_name_reachable {
        panic!(
            "token {:?} at byte {} is not reachable through completion in {sql:?}: {expectations:?}",
            token.kind,
            usize::from(token.range.start())
        );
    }
}

fn assert_expectation_provenance(sql: &str, point: TextSize, expectations: &ParserExpectations) {
    for token in &expectations.tokens {
        assert!(
            expectations.direct_tokens.contains(token)
                || expectations.lookahead_tokens.contains(token)
                || expectations.expression_start_tokens.contains(token)
                || expectations.expression_continuation_tokens.contains(token)
                || expectations.follow_tokens.contains(token),
            "token {token:?} has no provenance at byte {} in {sql:?}: {expectations:?}",
            usize::from(point)
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
            "provenance token {token:?} is absent from the union at byte {} in {sql:?}: {expectations:?}",
            usize::from(point)
        );
    }
}

pub fn parse_error(sql: &str) -> ParseError {
    match parse_one(sql) {
        Err(error) => error,
        Ok(_) => panic!("expected {sql:?} to return a parse error"),
    }
}

pub fn assert_statement_cases(cases: &[StatementCase]) {
    for case in cases {
        let node = parse_statement(case.sql);
        assert!((case.expected)(&node), "wrong node for {:?}", case.sql);
    }
}
