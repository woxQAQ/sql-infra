use super::common::assert_parse_errors;

#[path = "syntax_errors/alter.rs"]
mod alter;
#[path = "syntax_errors/create.rs"]
mod create;
#[path = "syntax_errors/drop.rs"]
mod drop;
#[path = "syntax_errors/query_dml.rs"]
mod query_dml;
#[path = "syntax_errors/utility.rs"]
mod utility;
