use super::*;

impl Parser {
    pub(super) fn parse_define_text_search(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TextP)?;
        self.expect(TokenKind::Search)?;
        let kind = match self.advance().kind {
            TokenKind::Parser => ObjectType::Tsparser,
            TokenKind::Dictionary => ObjectType::Tsdictionary,
            TokenKind::Template => ObjectType::Tstemplate,
            TokenKind::Configuration => ObjectType::Tsconfiguration,
            _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
        };
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if defnames.is_empty() {
            return Err(self.error_here("TEXT SEARCH object requires a name"));
        }
        let definition = self.parse_parenthesized_definition()?;
        Ok(Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind,
            defnames,
            definition,
            ..DefineStmt::default()
        }))
    }

    pub(super) fn parse_alter_ts_dictionary(&mut self) -> PResult<Node> {
        let dictname = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if dictname.is_empty() {
            return Err(self.error_here("text search dictionary requires a name"));
        }
        let options = self.parse_parenthesized_definition()?;
        self.expect_statement_end()?;
        Ok(Node::AlterTsDictionaryStmt(AlterTsDictionaryStmt {
            node_tag: NodeTag::AlterTsDictionaryStmt,
            dictname,
            options,
        }))
    }

    pub(super) fn parse_alter_ts_configuration(&mut self) -> PResult<Node> {
        let cfgname = self.parse_name_list_until_keywords(&[
            TokenKind::AddP,
            TokenKind::Alter,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if cfgname.is_empty() {
            return Err(self.error_here("text search configuration requires a name"));
        }
        let mut stmt = AlterTsConfigurationStmt {
            node_tag: NodeTag::AlterTsConfigurationStmt,
            cfgname,
            ..AlterTsConfigurationStmt::default()
        };
        if self.consume(TokenKind::AddP) {
            self.expect(TokenKind::Mapping)?;
            self.expect(TokenKind::For)?;
            stmt.tokentype = self.parse_simple_name_list_until(&[TokenKind::With])?;
            self.expect(TokenKind::With)?;
            stmt.dicts = self.parse_any_name_list_until(&[TokenKind::Char(';'), TokenKind::Eof])?;
            stmt.kind = AlterTsConfigType::AddMapping;
        } else if self.consume(TokenKind::Alter) {
            self.expect(TokenKind::Mapping)?;
            if self.consume(TokenKind::For) {
                stmt.tokentype =
                    self.parse_simple_name_list_until(&[TokenKind::With, TokenKind::Replace])?;
                if self.consume(TokenKind::Replace) {
                    let old = self.parse_one_any_name(&[TokenKind::With])?;
                    self.expect(TokenKind::With)?;
                    let new = self.parse_one_any_name(&[TokenKind::Char(';'), TokenKind::Eof])?;
                    stmt.kind = AlterTsConfigType::ReplaceDictForToken;
                    stmt.dicts = vec![old, new];
                    stmt.replace = true;
                } else {
                    self.expect(TokenKind::With)?;
                    stmt.kind = AlterTsConfigType::AlterMappingForToken;
                    stmt.dicts =
                        self.parse_any_name_list_until(&[TokenKind::Char(';'), TokenKind::Eof])?;
                    stmt.override_ = true;
                }
            } else {
                self.expect(TokenKind::Replace)?;
                let old = self.parse_one_any_name(&[TokenKind::With])?;
                self.expect(TokenKind::With)?;
                let new = self.parse_one_any_name(&[TokenKind::Char(';'), TokenKind::Eof])?;
                stmt.kind = AlterTsConfigType::ReplaceDict;
                stmt.dicts = vec![old, new];
                stmt.replace = true;
            }
        } else if self.consume(TokenKind::Drop) {
            self.expect(TokenKind::Mapping)?;
            stmt.missing_ok = self.consume_if_exists()?;
            self.expect(TokenKind::For)?;
            stmt.tokentype =
                self.parse_simple_name_list_until(&[TokenKind::Char(';'), TokenKind::Eof])?;
            stmt.kind = AlterTsConfigType::DropMapping;
        } else {
            return Err(self.error_here("configuration alteration requires ADD, ALTER, or DROP"));
        }
        self.expect_statement_end()?;
        Ok(Node::AlterTsConfigurationStmt(stmt))
    }
}
