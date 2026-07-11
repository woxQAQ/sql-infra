use super::*;

impl Parser {
    pub(super) fn parse_alter_collation(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Collation)?;
        let collname = self.parse_name_list_until_keywords(&[
            TokenKind::Refresh,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if collname.is_empty() {
            return Err(self.error_here("ALTER COLLATION requires a collation name"));
        }
        self.expect(TokenKind::Refresh)?;
        self.expect(TokenKind::VersionP)?;
        self.expect_statement_end()?;
        Ok(Node::AlterCollationStmt(AlterCollationStmt {
            node_tag: NodeTag::AlterCollationStmt,
            collname,
        }))
    }

    pub(super) fn parse_create_language(
        &mut self,
        replace: bool,
        pltrusted: bool,
    ) -> PResult<Node> {
        self.expect(TokenKind::Language)?;
        let plname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE LANGUAGE requires a name"))?,
        );
        if !self.consume(TokenKind::Handler) {
            return Ok(Node::CreateExtensionStmt(CreateExtensionStmt {
                node_tag: NodeTag::CreateExtensionStmt,
                extname: plname,
                if_not_exists: replace,
                options: Vec::new(),
            }));
        }
        let plhandler = self.parse_name_list();
        if plhandler.is_empty() {
            return Err(self.error_here("HANDLER requires a function name"));
        }
        let plinline = if self.consume(TokenKind::InlineP) {
            let name = self.parse_name_list();
            if name.is_empty() {
                return Err(self.error_here("INLINE requires a function name"));
            }
            name
        } else {
            Vec::new()
        };
        let plvalidator = if self.consume(TokenKind::Validator) {
            let name = self.parse_name_list();
            if name.is_empty() {
                return Err(self.error_here("VALIDATOR requires a function name"));
            }
            name
        } else if self.consume(TokenKind::No) {
            self.expect(TokenKind::Validator)?;
            Vec::new()
        } else {
            Vec::new()
        };
        Ok(Node::CreatePLangStmt(CreatePLangStmt {
            node_tag: NodeTag::CreatePLangStmt,
            replace,
            plname,
            plhandler,
            plinline,
            plvalidator,
            pltrusted,
        }))
    }

    pub(super) fn parse_create_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE TABLESPACE requires a name"))?,
        );
        let owner = if self.consume(TokenKind::Owner) {
            Some(Box::new(
                self.consume_role_spec()
                    .ok_or_else(|| self.error_here("OWNER requires a role"))?,
            ))
        } else {
            None
        };
        self.expect(TokenKind::Location)?;
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here("TABLESPACE LOCATION requires a string"));
        }
        let location = self.consume_string_like();
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            Vec::new()
        };
        Ok(Node::CreateTableSpaceStmt(CreateTableSpaceStmt {
            node_tag: NodeTag::CreateTableSpaceStmt,
            tablespacename,
            owner,
            location,
            options,
        }))
    }

    pub(super) fn parse_alter_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER TABLESPACE requires a tablespace name"))?,
        );
        if self.consume(TokenKind::Rename) {
            self.expect(TokenKind::To)?;
            let newname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME TO requires a new name"))?,
            );
            return Ok(Node::RenameStmt(RenameStmt {
                node_tag: NodeTag::RenameStmt,
                rename_type: ObjectType::Tablespace,
                subname: tablespacename,
                newname,
                ..RenameStmt::default()
            }));
        }
        let is_reset = if self.consume(TokenKind::Set) {
            false
        } else if self.consume(TokenKind::Reset) {
            true
        } else {
            return Err(self.error_here("ALTER TABLESPACE requires SET or RESET"));
        };
        let options = self.parse_parenthesized_reloptions()?;
        Ok(Node::AlterTableSpaceOptionsStmt(
            AlterTableSpaceOptionsStmt {
                node_tag: NodeTag::AlterTableSpaceOptionsStmt,
                tablespacename,
                options,
                is_reset,
            },
        ))
    }

    pub(super) fn parse_drop_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let missing_ok = self.consume_if_exists()?;
        let tablespacename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DROP TABLESPACE requires a name"))?,
        );
        self.expect_statement_end()?;
        Ok(Node::DropTableSpaceStmt(DropTableSpaceStmt {
            node_tag: NodeTag::DropTableSpaceStmt,
            tablespacename,
            missing_ok,
        }))
    }

    pub(super) fn parse_create_am(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Method)?;
        let amname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE ACCESS METHOD requires a name"))?,
        );
        self.expect(TokenKind::TypeP)?;
        let amtype = if self.consume(TokenKind::Index) {
            b'i'
        } else if self.consume(TokenKind::Table) {
            b't'
        } else {
            return Err(self.error_here("access method TYPE must be INDEX or TABLE"));
        };
        self.expect(TokenKind::Handler)?;
        let handler_name = self.parse_name_list();
        if handler_name.is_empty() {
            return Err(self.error_here("access method HANDLER requires a function name"));
        }
        Ok(Node::CreateAmStmt(CreateAmStmt {
            node_tag: NodeTag::CreateAmStmt,
            amname,
            handler_name,
            amtype,
        }))
    }
}
