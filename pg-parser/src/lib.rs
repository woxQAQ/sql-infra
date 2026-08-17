//! PostgreSQL-compatible SQL infrastructure built around a hand-written parser.
//!
//! The crate exposes raw PostgreSQL-shaped AST nodes, source locs, strict and
//! completion-aware lexing, and entry points for full statements and fragments.

pub mod ast;
pub mod deparse;
pub mod lexer;
pub mod parser;
pub mod source;
mod statement_splitter;

pub use ast::*;
pub use deparse::*;
pub use lexer::*;
pub use parser::*;
pub use source::*;
pub use statement_splitter::split_statement_locs;
