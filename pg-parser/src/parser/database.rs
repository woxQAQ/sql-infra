//! Database and system-configuration DDL parsing.
//!
//! This module owns create/alter/drop database forms, database options, and
//! `ALTER SYSTEM` rather than treating their option grammars as generic DDL.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createdatabase.html
    // CREATE DATABASE name
    //     [ WITH ] [ OWNER [=] user_name ]
    //            [ TEMPLATE [=] template ]
    //            [ ENCODING [=] encoding ]
    //            [ STRATEGY [=] strategy ]
    //            [ LOCALE [=] locale ]
    //            [ LC_COLLATE [=] lc_collate ]
    //            [ LC_CTYPE [=] lc_ctype ]
    //            [ BUILTIN_LOCALE [=] builtin_locale ]
    //            [ ICU_LOCALE [=] icu_locale ]
    //            [ ICU_RULES [=] icu_rules ]
    //            [ LOCALE_PROVIDER [=] locale_provider ]
    //            [ COLLATION_VERSION = collation_version ]
    //            [ TABLESPACE [=] tablespace_name ]
    //            [ ALLOW_CONNECTIONS [=] allowconn ]
    //            [ CONNECTION LIMIT [=] connlimit ]
    //            [ IS_TEMPLATE [=] istemplate ]
    //            [ OID [=] oid ]
    pub(super) fn parse_createdb(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        self.record_completion_slot(completion::GrammarSlot::Database);
        let dbname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE DATABASE requires a database name"))?,
        );
        self.consume(TokenKind::With);
        let options = self.parse_database_options()?;
        Ok(Node::CreatedbStmt(CreatedbStmt { dbname, options }))
    }

    fn parse_database_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end() {
            let location = self.location();
            let name = if self.consume(TokenKind::Connection) {
                self.expect(TokenKind::Limit)?;
                "connection_limit".to_owned()
            } else if matches!(
                self.peek_kind(),
                TokenKind::Encoding
                    | TokenKind::Location
                    | TokenKind::Owner
                    | TokenKind::Tablespace
                    | TokenKind::Template
            ) {
                let token = self.advance().clone();
                token_name(&token).unwrap_or_else(|| token_text(&token))
            } else {
                self.consume_identifier()
                    .ok_or_else(|| self.error_here("expected a database option name"))?
            };
            self.consume(TokenKind::Char('='));
            match name.as_str() {
                "owner" => self.record_completion_slot(completion::GrammarSlot::Role),
                "tablespace" => self.record_completion_slot(completion::GrammarSlot::Tablespace),
                "template" => self.record_completion_slot(completion::GrammarSlot::Database),
                _ => {}
            }
            let arg = if self.consume(TokenKind::Default) {
                None
            } else if matches!(
                self.peek_kind(),
                TokenKind::IConst | TokenKind::FConst | TokenKind::Char('+') | TokenKind::Char('-')
            ) {
                Some(Box::new(self.parse_numeric_only()?))
            } else {
                let value = self
                    .consume_opt_boolean_or_string()
                    .ok_or_else(|| self.error_here("database option requires a value"))?;
                Some(Box::new(make_string_node(value)))
            };
            options.push(Node::DefElem(DefElem {
                defname: Some(name),
                arg,
                location: location as ParseLoc,
                ..DefElem::default()
            }));
        }
        Ok(options)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdatabase.html
    // ALTER DATABASE name [ [ WITH ] option [ ... ] ]
    //
    // where option can be:
    //
    //     ALLOW_CONNECTIONS allowconn
    //     CONNECTION LIMIT connlimit
    //     IS_TEMPLATE istemplate
    //
    // ALTER DATABASE name RENAME TO new_name
    //
    // ALTER DATABASE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER DATABASE name SET TABLESPACE new_tablespace
    //
    // ALTER DATABASE name REFRESH COLLATION VERSION
    //
    // ALTER DATABASE name SET configuration_parameter { TO | = } { value | DEFAULT }
    // ALTER DATABASE name SET configuration_parameter FROM CURRENT
    // ALTER DATABASE name RESET configuration_parameter
    // ALTER DATABASE name RESET ALL
    pub(super) fn parse_alter_database(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        self.record_completion_slot(completion::GrammarSlot::Database);
        let dbname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("ALTER DATABASE requires a database name"))?,
        );
        self.record_completion_tokens(&[TokenKind::Rename, TokenKind::Owner]);
        if self.consume(TokenKind::Refresh) {
            self.expect(TokenKind::Collation)?;
            self.expect(TokenKind::VersionP)?;
            Ok(Node::AlterDatabaseRefreshCollStmt(
                AlterDatabaseRefreshCollStmt { dbname },
            ))
        } else if self.peek_kind() == TokenKind::Set && self.peek_kind_n(1) == TokenKind::Tablespace
        {
            self.expect(TokenKind::Set)?;
            self.expect(TokenKind::Tablespace)?;
            self.record_completion_slot(completion::GrammarSlot::Tablespace);
            let location = self.location();
            let tablespace = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("SET TABLESPACE requires a tablespace name"))?;
            Ok(Node::AlterDatabaseStmt(AlterDatabaseStmt {
                dbname,
                options: vec![make_def_elem(
                    "tablespace",
                    Some(make_string_node(tablespace)),
                    location,
                )],
            }))
        } else if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            let setstmt = Some(Box::new(self.parse_variable_set_like(false)?));
            Ok(Node::AlterDatabaseSetStmt(AlterDatabaseSetStmt {
                dbname,
                setstmt,
            }))
        } else {
            self.consume(TokenKind::With);
            let options = self.parse_database_options()?;
            if options.is_empty() {
                return Err(self.error_here("ALTER DATABASE requires an action or option"));
            }
            Ok(Node::AlterDatabaseStmt(AlterDatabaseStmt {
                dbname,
                options,
            }))
        }
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersystem.html
    // ALTER SYSTEM SET configuration_parameter { TO | = } { value [, ...] | DEFAULT }
    //
    // ALTER SYSTEM RESET configuration_parameter
    // ALTER SYSTEM RESET ALL
    pub(super) fn parse_alter_system(&mut self) -> PResult<Node> {
        self.expect(TokenKind::SystemP)?;
        self.record_completion_tokens(&[TokenKind::Set, TokenKind::Reset]);
        if !matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            return Err(self.error_here("ALTER SYSTEM requires SET or RESET"));
        }
        let setstmt = Some(Box::new(self.parse_generic_set_reset_clause()?));
        Ok(Node::AlterSystemStmt(AlterSystemStmt { setstmt }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-dropdatabase.html
    // DROP DATABASE [ IF EXISTS ] name [ [ WITH ] ( option [, ...] ) ]
    //
    // where option can be:
    //
    //     FORCE
    pub(super) fn parse_drop_database(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        let missing_ok = self.consume_if_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Database);
        let dbname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DROP DATABASE requires a database name"))?,
        );
        let has_with = self.consume(TokenKind::With);
        let options = if has_with || self.at(TokenKind::Char('(')) {
            self.expect(TokenKind::Char('('))?;
            let mut options = Vec::new();
            loop {
                let location = self.expect(TokenKind::Force)?.location();
                options.push(make_def_elem("force", None, location));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected a DROP DATABASE option after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
            options
        } else {
            Vec::new()
        };
        self.expect_statement_end()?;
        Ok(Node::DropdbStmt(DropdbStmt {
            dbname,
            missing_ok,
            options,
        }))
    }
}
