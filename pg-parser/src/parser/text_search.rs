use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtsconfig.html
    // CREATE TEXT SEARCH CONFIGURATION name (
    //     PARSER = parser_name |
    //     COPY = source_config
    // )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtsdictionary.html
    // CREATE TEXT SEARCH DICTIONARY name (
    //     TEMPLATE = template
    //     [, option = value [, ... ]]
    // )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtsparser.html
    // CREATE TEXT SEARCH PARSER name (
    //     START = start_function ,
    //     GETTOKEN = gettoken_function ,
    //     END = end_function ,
    //     LEXTYPES = lextypes_function
    //     [, HEADLINE = headline_function ]
    // )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtstemplate.html
    // CREATE TEXT SEARCH TEMPLATE name (
    //     [ INIT = init_function , ]
    //     LEXIZE = lexize_function
    // )
    pub(super) fn parse_define_text_search(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TextP)?;
        self.expect(TokenKind::Search)?;
        self.record_completion_tokens(&[
            TokenKind::Parser,
            TokenKind::Dictionary,
            TokenKind::Template,
            TokenKind::Configuration,
        ]);
        let kind = match self.peek_kind() {
            TokenKind::Parser => ObjectType::Tsparser,
            TokenKind::Dictionary => ObjectType::Tsdictionary,
            TokenKind::Template => ObjectType::Tstemplate,
            TokenKind::Configuration => ObjectType::Tsconfiguration,
            _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
        };
        self.advance();
        let name_stops = [TokenKind::Char('('), TokenKind::Char(';'), TokenKind::Eof];
        let slot = completion::object_type_slot(kind);
        self.record_completion_slot(slot);
        self.record_completion_slot_before(slot, &name_stops);
        let defnames = self.parse_name_list_until_keywords(&name_stops);
        if defnames.is_empty() {
            return Err(self.error_here("TEXT SEARCH object requires a name"));
        }
        let definition = self.parse_parenthesized_definition_for(Some(kind))?;
        Ok(Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind,
            defnames,
            definition,
            ..DefineStmt::default()
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsdictionary.html
    // ALTER TEXT SEARCH DICTIONARY name (
    //     option [ = value ] [, ... ]
    // )
    // ALTER TEXT SEARCH DICTIONARY name RENAME TO new_name
    // ALTER TEXT SEARCH DICTIONARY name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH DICTIONARY name SET SCHEMA new_schema
    pub(super) fn parse_alter_ts_dictionary(&mut self) -> PResult<Node> {
        let name_stops = [TokenKind::Char('('), TokenKind::Char(';'), TokenKind::Eof];
        self.record_completion_slot(completion::GrammarSlot::TextSearchDictionary);
        self.record_completion_slot_before(
            completion::GrammarSlot::TextSearchDictionary,
            &name_stops,
        );
        let dictname = self.parse_name_list_until_keywords(&name_stops);
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

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsconfig.html
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ADD MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     DROP MAPPING [ IF EXISTS ] FOR token_type [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name RENAME TO new_name
    // ALTER TEXT SEARCH CONFIGURATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH CONFIGURATION name SET SCHEMA new_schema
    pub(super) fn parse_alter_ts_configuration(&mut self) -> PResult<Node> {
        let name_stops = [
            TokenKind::AddP,
            TokenKind::Alter,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        self.record_completion_slot(completion::GrammarSlot::TextSearchConfiguration);
        self.record_completion_slot_before(
            completion::GrammarSlot::TextSearchConfiguration,
            &name_stops,
        );
        let cfgname = self.parse_name_list_until_keywords(&name_stops);
        if cfgname.is_empty() {
            return Err(self.error_here("text search configuration requires a name"));
        }
        let mut stmt = AlterTsConfigurationStmt {
            node_tag: NodeTag::AlterTsConfigurationStmt,
            cfgname,
            ..AlterTsConfigurationStmt::default()
        };
        self.record_completion_tokens(&[
            TokenKind::AddP,
            TokenKind::Alter,
            TokenKind::Drop,
            TokenKind::Rename,
            TokenKind::Set,
            TokenKind::Owner,
        ]);
        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                self.expect(TokenKind::Mapping)?;
                self.expect(TokenKind::For)?;
                stmt.tokentype = self.parse_simple_name_list_until(
                    &[TokenKind::With],
                    completion::GrammarSlot::AnyName,
                )?;
                self.expect(TokenKind::With)?;
                stmt.dicts = self.parse_any_name_list_until_with_slot(
                    &[TokenKind::Char(';'), TokenKind::Eof],
                    completion::GrammarSlot::TextSearchDictionary,
                )?;
                stmt.kind = AlterTsConfigType::AddMapping;
            }
            TokenKind::Alter => {
                self.advance();
                self.expect(TokenKind::Mapping)?;
                if self.consume(TokenKind::For) {
                    stmt.tokentype = self.parse_simple_name_list_until(
                        &[TokenKind::With, TokenKind::Replace],
                        completion::GrammarSlot::AnyName,
                    )?;
                    if self.consume(TokenKind::Replace) {
                        let old = self.parse_one_any_name_with_slot(
                            &[TokenKind::With],
                            completion::GrammarSlot::TextSearchDictionary,
                        )?;
                        self.expect(TokenKind::With)?;
                        let new = self.parse_one_any_name_with_slot(
                            &[TokenKind::Char(';'), TokenKind::Eof],
                            completion::GrammarSlot::TextSearchDictionary,
                        )?;
                        stmt.kind = AlterTsConfigType::ReplaceDictForToken;
                        stmt.dicts = vec![old, new];
                        stmt.replace = true;
                    } else {
                        self.expect(TokenKind::With)?;
                        stmt.kind = AlterTsConfigType::AlterMappingForToken;
                        stmt.dicts = self.parse_any_name_list_until_with_slot(
                            &[TokenKind::Char(';'), TokenKind::Eof],
                            completion::GrammarSlot::TextSearchDictionary,
                        )?;
                        stmt.override_ = true;
                    }
                } else {
                    self.expect(TokenKind::Replace)?;
                    let old = self.parse_one_any_name_with_slot(
                        &[TokenKind::With],
                        completion::GrammarSlot::TextSearchDictionary,
                    )?;
                    self.expect(TokenKind::With)?;
                    let new = self.parse_one_any_name_with_slot(
                        &[TokenKind::Char(';'), TokenKind::Eof],
                        completion::GrammarSlot::TextSearchDictionary,
                    )?;
                    stmt.kind = AlterTsConfigType::ReplaceDict;
                    stmt.dicts = vec![old, new];
                    stmt.replace = true;
                }
            }
            TokenKind::Drop => {
                self.advance();
                self.expect(TokenKind::Mapping)?;
                stmt.missing_ok = self.consume_if_exists()?;
                self.expect(TokenKind::For)?;
                stmt.tokentype = self.parse_simple_name_list_until(
                    &[TokenKind::Char(';'), TokenKind::Eof],
                    completion::GrammarSlot::AnyName,
                )?;
                stmt.kind = AlterTsConfigType::DropMapping;
            }
            _ => {
                return Err(
                    self.error_here("configuration alteration requires ADD, ALTER, or DROP")
                );
            }
        }
        self.expect_statement_end()?;
        Ok(Node::AlterTsConfigurationStmt(stmt))
    }
}
