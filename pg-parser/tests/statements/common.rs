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

pub fn assert_parse_errors(cases: &[&str]) {
    for sql in cases {
        let error = parse_error(sql);
        assert!(!error.message.is_empty(), "{sql:?} returned an empty error");
    }
}

// Keep AST field assertions in each scenario, but centralize the repetitive
// top-level and nested Node variant extraction here.
macro_rules! parse_node {
    ($sql:expr, $variant:ident) => {{
        let sql = $sql;
        match $crate::common::parse_statement(sql) {
            pg_parser::Node::$variant(value) => value,
            node => panic!(
                "expected {} for {sql:?}, got {:?}",
                stringify!($variant),
                node.tag()
            ),
        }
    }};
}

macro_rules! expect_node {
    ($node:expr, Some($variant:ident)) => {{
        match $node {
            Some(pg_parser::Node::$variant(value)) => value,
            Some(node) => panic!(
                "expected Some({}), got Some({:?})",
                stringify!($variant),
                node.tag()
            ),
            None => panic!("expected Some({}), got None", stringify!($variant)),
        }
    }};
    ($node:expr, $variant:ident) => {{
        match $node {
            pg_parser::Node::$variant(value) => value,
            node => panic!("expected {}, got {:?}", stringify!($variant), node.tag()),
        }
    }};
}

pub(crate) use expect_node;
pub(crate) use parse_node;
