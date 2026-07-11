use pg_parser::{Node, NodeTag, ParseError, parse_one};

#[derive(Clone, Copy)]
pub struct StatementCase {
    pub expected: NodeTag,
    pub sql: &'static str,
}

pub fn parse_statement(sql: &str) -> Node {
    let raw = parse_one(sql).unwrap_or_else(|error| panic!("failed to parse {sql:?}: {error}"));
    *raw.stmt
        .unwrap_or_else(|| panic!("parser returned an empty RawStmt for {sql:?}"))
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
        assert_eq!(node.tag(), case.expected, "wrong node for {:?}", case.sql);
    }
}
