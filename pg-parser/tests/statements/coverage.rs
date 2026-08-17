use pg_parser::KEYWORDS;
use pg_parser::KeywordCategory;
use pg_parser::TextSize;
use pg_parser::collect_expectations;
use pg_parser::lex;

use super::smoke::CASES;

#[test]
fn completion_collection_handles_every_smoke_statement_token_boundary() {
    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        let mut points = tokens
            .iter()
            .flat_map(|token| [token.loc.start(), token.loc.end()])
            .collect::<Vec<_>>();
        points.sort_unstable();
        points.dedup();

        for point in points {
            collect_expectations(case.sql, point).unwrap_or_else(|error| {
                panic!(
                    "completion collection failed for {:?} at byte {}: {error}",
                    case.sql,
                    usize::from(point)
                )
            });
        }

        let complete = collect_expectations(
            case.sql,
            TextSize::try_from(case.sql.len()).expect("smoke SQL length fits TextSize"),
        )
        .unwrap_or_else(|error| {
            panic!(
                "completion collection failed for complete smoke case {:?}: {error}",
                case.sql
            )
        });
        assert!(
            !complete.tokens.contains(&pg_parser::TokenKind::Char(';')),
            "complete smoke case published the statement terminator for {:?}: {:?}",
            case.sql,
            complete.tokens
        );
        assert!(
            complete.slots.iter().all(|slot| matches!(
                slot,
                pg_parser::GrammarSlot::Alias | pg_parser::GrammarSlot::AnyName
            )),
            "complete smoke case published a stale object slot for {:?}: {:?}",
            case.sql,
            complete.slots
        );
    }
}

#[test]
fn completion_publishes_every_reserved_keyword_in_smoke_statements() {
    let reserved = KEYWORDS
        .iter()
        .filter(|keyword| keyword.category == KeywordCategory::Reserved)
        .map(|keyword| keyword.kind)
        .collect::<Vec<_>>();
    let mut missing = Vec::new();

    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        for token in tokens.iter().filter(|token| reserved.contains(&token.kind)) {
            let expectations = collect_expectations(case.sql, token.loc.start())
                .unwrap_or_else(|error| panic!("completion failed for {:?}: {error}", case.sql));
            if !expectations.tokens.contains(&token.kind) {
                missing.push(format!(
                    "{:?} at byte {} in {:?}: {:?}",
                    token.kind,
                    usize::from(token.loc.start()),
                    case.sql,
                    expectations
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "reserved keyword completion gaps:\n{}",
        missing.join("\n")
    );
}

#[test]
fn completion_publishes_every_punctuation_token_in_smoke_statements() {
    let mut missing = Vec::new();

    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        for token in tokens.iter().filter(
            |token| matches!(token.kind, pg_parser::TokenKind::Char(character) if character != ';'),
        ) {
            let expectations = collect_expectations(case.sql, token.loc.start())
                .unwrap_or_else(|error| panic!("completion failed for {:?}: {error}", case.sql));
            let operator_name = expectations
                .slots
                .contains(&pg_parser::GrammarSlot::Operator)
                && matches!(
                    token.kind,
                    pg_parser::TokenKind::Char(
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
                );
            if !expectations.tokens.contains(&token.kind) && !operator_name {
                missing.push(format!(
                    "{:?} at byte {} in {:?}: {:?}",
                    token.kind,
                    usize::from(token.loc.start()),
                    case.sql,
                    expectations
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "punctuation completion gaps:\n{}",
        missing.join("\n")
    );
}
