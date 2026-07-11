use crate::ast::*;
use crate::lexer::{Token, TokenValue, lex, lookup_keyword};
use crate::{BareLabel, KeywordCategory, TokenKind};

mod aggregate_signatures;
mod alter;
mod alter_identity;
mod alter_table;
mod alter_table_partition;
mod ast_helpers;
mod constraints;
mod create;
mod create_cast_transform;
mod create_helpers;
mod create_table;
mod create_trigger;
mod cursor;
mod database;
mod define;
mod describe;
mod dml;
mod dml_grammar;
mod domain;
mod drop;
mod error;
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
mod infrastructure;
mod json_table;
mod maintenance;
mod names;
mod object_helpers;
mod opclass;
mod operator_definition;
mod parser_cursor;
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
mod statement;
mod statistics;
mod table_elements;
mod text_search;
mod token_helpers;
mod type_statements;
mod type_tokens;
mod utility_helpers;
mod window;
mod xmltable_columns;
use aggregate_signatures::*;
use ast_helpers::*;
pub use error::ParseError;
use expression::ExprParser;
use expression_helpers::*;
use expression_json::{default_json_format, json_behavior_starts, parse_json_value_expr_tokens};
use fragment_parser::*;
use function_parameters::*;
use index::*;
use object_helpers::*;
use settings::{parse_setting_value_tokens, parse_time_zone_value_tokens};
use table_elements::*;
use token_helpers::*;
use type_tokens::*;
use xmltable_columns::*;

type PResult<T> = Result<T, ParseError>;
type JsonBehaviorPair = (Option<Box<JsonBehavior>>, Option<Box<JsonBehavior>>);

pub fn parse(sql: &str) -> PResult<Vec<RawStmt>> {
    Parser::new(sql)?.parse()
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

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(sql: &str) -> PResult<Self> {
        Ok(Self {
            tokens: lex(sql)?,
            pos: 0,
        })
    }

    pub fn parse(&mut self) -> PResult<Vec<RawStmt>> {
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
            let terminated = self.consume(TokenKind::Char(';'));
            stmts.push(RawStmt {
                node_tag: NodeTag::RawStmt,
                stmt: Some(Box::new(stmt)),
                stmt_location: start as ParseLoc,
                stmt_len: if terminated {
                    end.saturating_sub(start) as ParseLoc
                } else {
                    0
                },
            });
        }
        Ok(stmts)
    }
}

#[cfg(test)]
mod tests;
