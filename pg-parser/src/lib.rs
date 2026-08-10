//! PostgreSQL-compatible SQL infrastructure built around a hand-written parser.
//!
//! The crate exposes raw PostgreSQL-shaped AST nodes, source ranges, strict and
//! completion-aware lexing, and entry points for full statements and fragments.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod source;

pub use ast::*;
pub use lexer::*;
pub use parser::*;
pub use source::*;
