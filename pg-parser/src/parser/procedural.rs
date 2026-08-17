//! Miscellaneous procedural and extension-facing utility statements.
//!
//! This module parses `IMPORT FOREIGN SCHEMA`, `DO`, `RETURN`, and `WAIT`, whose
//! grammars do not belong to the core query or DDL families.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-importforeignschema.html
    // IMPORT FOREIGN SCHEMA remote_schema
    //     [ { LIMIT TO | EXCEPT } ( table_name [, ...] ) ]
    //     FROM SERVER server_name
    //     INTO local_schema
    //     [ OPTIONS ( option 'value' [, ... ] ) ]
    pub(super) fn parse_import_foreign_schema(&mut self) -> PResult<Node> {
        self.expect(TokenKind::ImportP)?;
        self.expect(TokenKind::Foreign)?;
        self.expect(TokenKind::Schema)?;
        self.record_completion_slot(GrammarSlot::Schema);
        let remote_schema =
            Some(self.consume_col_id().ok_or_else(|| {
                self.error_here("IMPORT FOREIGN SCHEMA requires a remote schema")
            })?);
        let (list_type, table_list) = if self.consume(TokenKind::Limit) {
            self.expect(TokenKind::To)?;
            self.expect(TokenKind::Char('('))?;
            let tables = self.parse_relation_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            (ImportForeignSchemaType::LimitTo, tables)
        } else if self.consume(TokenKind::Except) {
            self.expect(TokenKind::Char('('))?;
            let tables = self.parse_relation_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            (ImportForeignSchemaType::Except, tables)
        } else {
            (ImportForeignSchemaType::All, Vec::new())
        };
        self.expect(TokenKind::From)?;
        self.expect(TokenKind::Server)?;
        self.record_completion_slot(GrammarSlot::ForeignServer);
        let server_name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("IMPORT FOREIGN SCHEMA requires a server"))?,
        );
        self.expect(TokenKind::Into)?;
        self.record_completion_slot(GrammarSlot::Schema);
        let local_schema = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("IMPORT FOREIGN SCHEMA requires a local schema"))?,
        );
        let options = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };
        Ok(node!(ImportForeignSchemaStmt {
            server_name,
            remote_schema,
            local_schema,
            list_type,
            table_list,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-do.html
    // DO [ LANGUAGE lang_name ] code
    pub(super) fn parse_do(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Do)?;
        let mut args = Vec::new();
        let mut saw_body = false;
        let mut saw_language = false;
        while !self.at_statement_end() {
            let offset = self.offset();
            if !saw_body {
                self.record_completion_tokens(&[TokenKind::SConst]);
            }
            if !saw_language {
                self.record_completion_tokens(&[TokenKind::Language]);
            }
            if self.at(TokenKind::SConst) {
                let value = self.consume_string_like().unwrap_or_default();
                args.push(make_def_elem("as", Some(make_string_node(value)), offset));
                saw_body = true;
            } else if self.at(TokenKind::Language) {
                self.advance();
                self.record_completion_slot(GrammarSlot::Language);
                let language = self
                    .consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("DO LANGUAGE requires a language name"))?;
                args.push(make_def_elem(
                    "language",
                    Some(make_string_node(language)),
                    offset,
                ));
                saw_language = true;
            } else {
                return Err(self.error_here("expected a DO code block or LANGUAGE clause"));
            }
        }
        if args.is_empty() {
            return Err(self.error_here("DO requires a code block or LANGUAGE clause"));
        }
        Ok(node!(DoStmt { args }))
    }

    pub(super) fn parse_return(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Return)?;
        let returnval = Some(self.parse_expr_box_strict_until(STATEMENT_END_TOKENS)?);
        Ok(node!(ReturnStmt { returnval }))
    }

    pub(super) fn parse_wait(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Wait)?;
        self.expect(TokenKind::For)?;
        self.expect(TokenKind::LsnP)?;
        let lsn_literal =
            Some(self.consume_required_string("WAIT FOR LSN requires a string literal")?);
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        Ok(node!(WaitStmt {
            lsn_literal,
            options,
        }))
    }
}
