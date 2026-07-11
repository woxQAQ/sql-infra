use super::*;

impl Parser {
    pub(super) fn parse_import_foreign_schema(&mut self) -> PResult<Node> {
        self.expect(TokenKind::ImportP)?;
        self.expect(TokenKind::Foreign)?;
        self.expect(TokenKind::Schema)?;
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
        let server_name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("IMPORT FOREIGN SCHEMA requires a server"))?,
        );
        self.expect(TokenKind::Into)?;
        let local_schema = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("IMPORT FOREIGN SCHEMA requires a local schema"))?,
        );
        let options = if self.at(TokenKind::Options) {
            self.parse_create_generic_options()?
        } else {
            Vec::new()
        };
        Ok(Node::ImportForeignSchemaStmt(ImportForeignSchemaStmt {
            node_tag: NodeTag::ImportForeignSchemaStmt,
            server_name,
            remote_schema,
            local_schema,
            list_type,
            table_list,
            options,
        }))
    }

    pub(super) fn parse_do(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Do)?;
        let mut args = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            if self.at(TokenKind::SConst) {
                let value = self.consume_string_like().unwrap_or_default();
                args.push(make_def_elem("as", Some(make_string_node(value)), location));
            } else if self.consume(TokenKind::Language) {
                let language = self
                    .consume_non_reserved_word_or_sconst()
                    .ok_or_else(|| self.error_here("DO LANGUAGE requires a language name"))?;
                args.push(make_def_elem(
                    "language",
                    Some(make_string_node(language)),
                    location,
                ));
            } else {
                return Err(self.error_here("expected a DO code block or LANGUAGE clause"));
            }
        }
        if args.is_empty() {
            return Err(self.error_here("DO requires a code block or LANGUAGE clause"));
        }
        Ok(Node::DoStmt(DoStmt {
            node_tag: NodeTag::DoStmt,
            args,
        }))
    }

    pub(super) fn parse_return(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Return)?;
        let returnval =
            Some(self.parse_expr_box_strict_until(&[TokenKind::Char(';'), TokenKind::Eof])?);
        Ok(Node::ReturnStmt(ReturnStmt {
            node_tag: NodeTag::ReturnStmt,
            returnval,
        }))
    }

    pub(super) fn parse_wait(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Wait)?;
        self.expect(TokenKind::For)?;
        self.expect(TokenKind::LsnP)?;
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here("WAIT FOR LSN requires a string literal"));
        }
        let lsn_literal = self.consume_string_like();
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        Ok(Node::WaitStmt(WaitStmt {
            node_tag: NodeTag::WaitStmt,
            lsn_literal,
            options,
        }))
    }
}
