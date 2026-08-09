//! `CREATE TABLE` and `CREATE TABLE AS` orchestration.
//!
//! Relation options, table elements, query sources, persistence, and `WITH DATA`
//! suffixes are assembled here from focused helper modules.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtable.html
    // CREATE [ [ GLOBAL | LOCAL ] { TEMPORARY | TEMP } | UNLOGGED ] TABLE [ IF NOT EXISTS ]
    // table_name ( [   { column_name data_type [ STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN |
    // DEFAULT } ] [ COMPRESSION compression_method ] [ COLLATE collation ] [ column_constraint [
    // ... ] ]     | table_constraint
    //     | LIKE source_table [ like_option ... ] }
    //     [, ... ]
    // ] )
    // [ INHERITS ( parent_table [, ... ] ) ]
    // [ PARTITION BY { RANGE | LIST | HASH } ( { column_name | ( expression ) } [ COLLATE collation
    // ] [ opclass ] [, ... ] ) ] [ USING method ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) | WITHOUT OIDS ]
    // [ ON COMMIT { PRESERVE ROWS | DELETE ROWS | DROP } ]
    // [ TABLESPACE tablespace_name ]
    //
    // CREATE [ [ GLOBAL | LOCAL ] { TEMPORARY | TEMP } | UNLOGGED ] TABLE [ IF NOT EXISTS ]
    // table_name     OF type_name [ (
    //   { column_name [ WITH OPTIONS ] [ column_constraint [ ... ] ]
    //     | table_constraint }
    //     [, ... ]
    // ) ]
    // [ PARTITION BY { RANGE | LIST | HASH } ( { column_name | ( expression ) } [ COLLATE collation
    // ] [ opclass ] [, ... ] ) ] [ USING method ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) | WITHOUT OIDS ]
    // [ ON COMMIT { PRESERVE ROWS | DELETE ROWS | DROP } ]
    // [ TABLESPACE tablespace_name ]
    //
    // CREATE [ [ GLOBAL | LOCAL ] { TEMPORARY | TEMP } | UNLOGGED ] TABLE [ IF NOT EXISTS ]
    // table_name     PARTITION OF parent_table [ (
    //   { column_name [ WITH OPTIONS ] [ column_constraint [ ... ] ]
    //     | table_constraint }
    //     [, ... ]
    // ) ] { FOR VALUES partition_bound_spec | DEFAULT }
    // [ PARTITION BY { RANGE | LIST | HASH } ( { column_name | ( expression ) } [ COLLATE collation
    // ] [ opclass ] [, ... ] ) ] [ USING method ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) | WITHOUT OIDS ]
    // [ ON COMMIT { PRESERVE ROWS | DELETE ROWS | DROP } ]
    // [ TABLESPACE tablespace_name ]
    //
    // where column_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL [ NO INHERIT ]  |
    //   NULL |
    //   CHECK ( expression ) [ NO INHERIT ] |
    //   DEFAULT default_expr |
    //   GENERATED ALWAYS AS ( generation_expr ) [ STORED | VIRTUAL ] |
    //   GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY [ ( sequence_options ) ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] index_parameters |
    //   PRIMARY KEY index_parameters |
    //   REFERENCES reftable [ ( refcolumn ) ] [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ]
    //     [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
    // [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT
    // ENFORCED ]
    //
    // and table_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { CHECK ( expression ) [ NO INHERIT ] |
    //   NOT NULL column_name [ NO INHERIT ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] ( column_name [, ... ] [, column_name WITHOUT OVERLAPS ]
    // ) index_parameters |   PRIMARY KEY ( column_name [, ... ] [, column_name WITHOUT OVERLAPS
    // ] ) index_parameters |   EXCLUDE [ USING index_method ] ( exclude_element WITH operator
    // [, ... ] ) index_parameters [ WHERE ( predicate ) ] |   FOREIGN KEY ( column_name [, ...
    // ] [, PERIOD column_name ] ) REFERENCES reftable [ ( refcolumn [, ... ] [, PERIOD refcolumn ]
    // ) ]     [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ] [ ON DELETE referential_action ] [
    // ON UPDATE referential_action ] } [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED |
    // INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //
    // and like_option is:
    //
    // { INCLUDING | EXCLUDING } { COMMENTS | COMPRESSION | CONSTRAINTS | DEFAULTS | GENERATED |
    // IDENTITY | INDEXES | STATISTICS | STORAGE | ALL }
    //
    // and partition_bound_spec is:
    //
    // IN ( partition_bound_expr [, ...] ) |
    // FROM ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] )
    //   TO ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] ) |
    // WITH ( MODULUS numeric_literal, REMAINDER numeric_literal )
    //
    // index_parameters in UNIQUE, PRIMARY KEY, and EXCLUDE constraints are:
    //
    // [ INCLUDE ( column_name [, ... ] ) ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) ]
    // [ USING INDEX TABLESPACE tablespace_name ]
    //
    // exclude_element in an EXCLUDE constraint is:
    //
    // { column_name | ( expression ) } [ COLLATE collation ] [ opclass [ ( opclass_parameter =
    // value [, ... ] ) ] ] [ ASC | DESC ] [ NULLS { FIRST | LAST } ]
    //
    // referential_action in a FOREIGN KEY/REFERENCES constraint is:
    //
    // { NO ACTION | RESTRICT | CASCADE | SET NULL [ ( column_name [, ... ] ) ] | SET DEFAULT [ (
    // column_name [, ... ] ) ] }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createforeigntable.html
    // CREATE FOREIGN TABLE [ IF NOT EXISTS ] table_name ( [
    //   { column_name data_type [ OPTIONS ( option 'value' [, ... ] ) ] [ COLLATE collation ] [
    // column_constraint [ ... ] ]     | table_constraint
    //     | LIKE source_table [ like_option ... ] }
    //     [, ... ]
    // ] )
    // [ INHERITS ( parent_table [, ... ] ) ]
    //   SERVER server_name
    // [ OPTIONS ( option 'value' [, ... ] ) ]
    //
    // CREATE FOREIGN TABLE [ IF NOT EXISTS ] table_name
    //   PARTITION OF parent_table [ (
    //   { column_name [ WITH OPTIONS ] [ column_constraint [ ... ] ]
    //     | table_constraint }
    //     [, ... ]
    // ) ]
    // { FOR VALUES partition_bound_spec | DEFAULT }
    //   SERVER server_name
    // [ OPTIONS ( option 'value' [, ... ] ) ]
    //
    // where column_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL [ NO INHERIT ] |
    //   NULL |
    //   CHECK ( expression ) [ NO INHERIT ] |
    //   DEFAULT default_expr |
    //   GENERATED ALWAYS AS ( generation_expr ) [ STORED | VIRTUAL ] }
    // [ ENFORCED | NOT ENFORCED ]
    //
    // and table_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // {  NOT NULL column_name [ NO INHERIT ] |
    //    CHECK ( expression ) [ NO INHERIT ] }
    // [ ENFORCED | NOT ENFORCED ]
    //
    // and like_option is:
    //
    // { INCLUDING | EXCLUDING } { COMMENTS | CONSTRAINTS | DEFAULTS | GENERATED | STATISTICS | ALL
    // }
    //
    // and partition_bound_spec is:
    //
    // IN ( partition_bound_expr [, ...] ) |
    // FROM ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] )
    //   TO ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] ) |
    // WITH ( MODULUS numeric_literal, REMAINDER numeric_literal )
    pub(super) fn parse_create_table(
        &mut self,
        foreign: bool,
        relpersistence: u8,
    ) -> PResult<Node> {
        self.expect(TokenKind::Table)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let target_slot = if foreign {
            completion::GrammarSlot::ForeignTable
        } else {
            completion::GrammarSlot::Table
        };
        let mut relation_node = self
            .try_parse_qualified_range_var_with_slot(target_slot)
            .ok_or_else(|| self.error_here("CREATE TABLE requires a relation name"))?;
        relation_node.relpersistence = relpersistence;
        let relation = Some(Box::new(relation_node));
        if !foreign
            && (self.has_top_level_token_before(TokenKind::As, STATEMENT_END_TOKENS)
                || self.completion_follows_create_table_as_target())
        {
            return self.parse_create_table_as_target(relation, if_not_exists);
        }
        let mut inh_relations = Vec::new();
        let mut partbound = None;
        let mut of_typename = None;
        self.record_completion_tokens(&[TokenKind::Char('('), TokenKind::Partition, TokenKind::Of]);
        if !foreign {
            self.record_completion_tokens(&[TokenKind::As]);
        }
        let table_elts = match self.peek_kind() {
            TokenKind::Partition => {
                self.advance();
                self.expect(TokenKind::Of)?;
                let parent = self
                    .try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                    .ok_or_else(|| self.error_here("expected a partitioned parent table"))?;
                inh_relations.push(Node::RangeVar(parent));
                let elements = if self.consume(TokenKind::Char('(')) {
                    self.parse_typed_table_elements()?
                } else {
                    Vec::new()
                };
                partbound = Some(Box::new(self.parse_partition_bound()?));
                elements
            }
            TokenKind::Of => {
                self.advance();
                let type_location = self.location();
                let names = self.consume_name_parts();
                if names.is_empty() {
                    return Err(self.error_here("CREATE TABLE OF requires a type name"));
                }
                of_typename = Some(Box::new(TypeName {
                    names: names.into_iter().map(make_string_node).collect(),
                    location: type_location as ParseLoc,
                    ..TypeName::default()
                }));
                if self.consume(TokenKind::Char('(')) {
                    self.parse_typed_table_elements()?
                } else {
                    Vec::new()
                }
            }
            TokenKind::Char('(') => {
                self.advance();
                self.parse_table_elements()?
            }
            _ => {
                return Err(self.error_here(
                    "CREATE TABLE requires a table element list, OF, or PARTITION OF",
                ));
            }
        };
        if self.consume(TokenKind::Inherits) {
            self.expect(TokenKind::Char('('))?;
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("INHERITS requires at least one parent relation"));
            }
            while !self.at(TokenKind::Char(')')) {
                let parent = self
                    .try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                    .ok_or_else(|| self.error_here("expected an inherited relation"))?;
                inh_relations.push(Node::RangeVar(parent));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
                if self.at(TokenKind::Char(')')) {
                    return Err(self.error_here("expected an inherited relation after ','"));
                }
            }
            self.expect(TokenKind::Char(')'))?;
        }
        let (partspec, access_method, options, oncommit, tablespacename) = if foreign {
            (None, None, Vec::new(), OnCommitAction::Noop, None)
        } else {
            self.record_completion_follow_phrase(&[TokenKind::Partition, TokenKind::By]);
            let partspec = if self.at(TokenKind::Partition) {
                Some(Box::new(self.parse_partition_spec()?))
            } else {
                None
            };
            let access_method = if self.consume_follow(TokenKind::Using) {
                self.record_completion_slot(completion::GrammarSlot::AccessMethod);
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected an access method"))?,
                )
            } else {
                None
            };
            let options = if self.consume_follow(TokenKind::With) {
                self.parse_parenthesized_reloptions()?
            } else {
                if self.consume_follow(TokenKind::Without) {
                    self.expect(TokenKind::Oids)?;
                }
                Vec::new()
            };
            let oncommit = self.parse_on_commit_option()?;
            let tablespacename = if self.consume_follow(TokenKind::Tablespace) {
                self.record_completion_slot(completion::GrammarSlot::Tablespace);
                Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("expected a tablespace name"))?,
                )
            } else {
                None
            };
            (partspec, access_method, options, oncommit, tablespacename)
        };
        let create = CreateStmt {
            relation,
            table_elts,
            inh_relations,
            partbound,
            partspec,
            of_typename,
            options,
            oncommit,
            tablespacename,
            access_method,
            if_not_exists,
            ..CreateStmt::default()
        };
        if foreign {
            self.expect(TokenKind::Server)?;
            self.record_completion_slot(completion::GrammarSlot::ForeignServer);
            let servername = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("expected a foreign server name"))?,
            );
            let foreign_options = if self.at(TokenKind::Options) {
                self.parse_create_generic_options()?
            } else {
                Vec::new()
            };
            Ok(node!(CreateForeignTableStmt {
                base: create,
                servername,
                options: foreign_options,
            }))
        } else {
            Ok(Node::CreateStmt(create))
        }
    }

    fn parse_create_table_as_target(
        &mut self,
        relation: Option<Box<RangeVar>>,
        if_not_exists: bool,
    ) -> PResult<Node> {
        let col_names = if self.consume(TokenKind::Char('(')) {
            let names = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            names
        } else {
            Vec::new()
        };
        let access_method = if self.consume(TokenKind::Using) {
            self.record_completion_slot(completion::GrammarSlot::AccessMethod);
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            None
        };
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            if self.consume(TokenKind::Without) {
                self.expect(TokenKind::Oids)?;
            }
            Vec::new()
        };
        let on_commit = self.parse_on_commit_option()?;
        let table_space_name = self.parse_optional_tablespace_name()?;
        self.expect(TokenKind::As)?;
        if self.at(TokenKind::Execute) {
            let query = Some(Box::new(Node::ExecuteStmt(self.parse_execute_core()?)));
            let skip_data = self.parse_optional_with_data()?;
            self.expect_statement_end()?;
            return Ok(node!(CreateTableAsStmt {
                query,
                into: Some(Box::new(IntoClause {
                    rel: relation,
                    col_names,
                    access_method,
                    options,
                    on_commit,
                    table_space_name,
                    skip_data,
                    ..IntoClause::default()
                })),
                objtype: ObjectType::Table,
                if_not_exists,
                ..CreateTableAsStmt::default()
            }));
        }
        let query_tokens = self.take_until_top_level(STATEMENT_END_TOKENS);
        self.record_with_data_suffix_completion(&query_tokens);
        let (query_tokens, skip_data) = split_with_data_suffix(query_tokens);
        let query = Some(Box::new(self.parse_select_fragment_tokens(query_tokens)?));
        Ok(node!(CreateTableAsStmt {
            query,
            into: Some(Box::new(IntoClause {
                rel: relation,
                col_names,
                access_method,
                options,
                on_commit,
                table_space_name,
                skip_data,
                ..IntoClause::default()
            })),
            objtype: ObjectType::Table,
            if_not_exists,
            ..CreateTableAsStmt::default()
        }))
    }

    fn completion_follows_create_table_as_target(&self) -> bool {
        let mut depth = 0usize;
        let mut completion = None;
        for (offset, token) in self.tokens[self.pos..].iter().enumerate() {
            match token.kind {
                TokenKind::Completion if depth == 0 => {
                    completion = Some(self.pos + offset);
                    break;
                }
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                TokenKind::Char(';') | TokenKind::Eof if depth == 0 => break,
                _ => {}
            }
        }
        let Some(completion) = completion else {
            return false;
        };
        if completion == self.pos {
            return false;
        }
        let mut tokens = self.tokens[self.pos..completion].to_vec();
        tokens.push(Token::synthetic(
            TokenKind::Eof,
            self.tokens[completion].location(),
        ));
        let mut probe = Parser {
            tokens,
            pos: 0,
            completion: None,
        };
        probe.parse_create_table_as_target_prefix().is_ok()
    }

    fn parse_create_table_as_target_prefix(&mut self) -> PResult<()> {
        if self.consume(TokenKind::Char('(')) {
            self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
        }
        if self.consume(TokenKind::Using) {
            self.consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?;
        }
        if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?;
        } else if self.consume(TokenKind::Without) {
            self.expect(TokenKind::Oids)?;
        }
        self.parse_on_commit_option()?;
        if self.consume(TokenKind::Tablespace) {
            self.consume_col_id()
                .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?;
        }
        self.expect(TokenKind::Eof)?;
        Ok(())
    }

    fn parse_on_commit_option(&mut self) -> PResult<OnCommitAction> {
        if !self.consume(TokenKind::On) {
            return Ok(OnCommitAction::Noop);
        }
        self.expect(TokenKind::Commit)?;
        if self.consume(TokenKind::Drop) {
            Ok(OnCommitAction::Drop)
        } else if self.consume(TokenKind::DeleteP) {
            self.expect(TokenKind::Rows)?;
            Ok(OnCommitAction::DeleteRows)
        } else {
            self.expect(TokenKind::Preserve)?;
            self.expect(TokenKind::Rows)?;
            Ok(OnCommitAction::PreserveRows)
        }
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtableas.html
    // CREATE [ [ GLOBAL | LOCAL ] { TEMPORARY | TEMP } | UNLOGGED ] TABLE [ IF NOT EXISTS ]
    // table_name     [ (column_name [, ...] ) ]
    //     [ USING method ]
    //     [ WITH ( storage_parameter [= value] [, ... ] ) | WITHOUT OIDS ]
    //     [ ON COMMIT { PRESERVE ROWS | DELETE ROWS | DROP } ]
    //     [ TABLESPACE tablespace_name ]
    //     AS query
    //     [ WITH [ NO ] DATA ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-creatematerializedview.html
    // CREATE MATERIALIZED VIEW [ IF NOT EXISTS ] table_name
    //     [ (column_name [, ...] ) ]
    //     [ USING method ]
    //     [ WITH ( storage_parameter [= value] [, ... ] ) ]
    //     [ TABLESPACE tablespace_name ]
    //     AS query
    //     [ WITH [ NO ] DATA ]
    pub(super) fn parse_create_table_as(
        &mut self,
        objtype: ObjectType,
        relpersistence: u8,
    ) -> PResult<Node> {
        self.expect(TokenKind::View)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let mut relation = self
            .try_parse_qualified_range_var_with_slot(completion::object_type_slot(objtype))
            .ok_or_else(|| self.error_here("CREATE MATERIALIZED VIEW requires a name"))?;
        relation.relpersistence = relpersistence;
        let rel = Some(Box::new(relation));
        let col_names = if self.consume(TokenKind::Char('(')) {
            let names = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            names
        } else {
            Vec::new()
        };
        let access_method = if self.consume(TokenKind::Using) {
            self.record_completion_slot(completion::GrammarSlot::AccessMethod);
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            None
        };
        let options = if self.consume(TokenKind::With) {
            self.parse_parenthesized_reloptions()?
        } else {
            Vec::new()
        };
        let table_space_name = self.parse_optional_tablespace_name()?;
        self.expect(TokenKind::As)?;
        let query_tokens = self.take_until_top_level(STATEMENT_END_TOKENS);
        self.record_with_data_suffix_completion(&query_tokens);
        let (query_tokens, skip_data) = split_with_data_suffix(query_tokens);
        let query = Some(Box::new(self.parse_select_fragment_tokens(query_tokens)?));
        Ok(node!(CreateTableAsStmt {
            query,
            into: Some(Box::new(IntoClause {
                rel,
                col_names,
                access_method,
                options,
                table_space_name,
                skip_data,
                ..IntoClause::default()
            })),
            objtype,
            if_not_exists,
            ..CreateTableAsStmt::default()
        }))
    }

    fn record_with_data_suffix_completion(&self, tokens: &[Token]) {
        if !self.at_completion() {
            return;
        }
        if parse_select_statement_tokens(tokens.to_vec()).is_ok() {
            self.record_completion_tokens(&[TokenKind::With]);
            return;
        }
        let kinds = tokens.iter().map(|token| token.kind).collect::<Vec<_>>();
        if kinds.last() == Some(&TokenKind::With)
            && parse_select_statement_tokens(tokens[..tokens.len() - 1].to_vec()).is_ok()
        {
            self.record_completion_tokens(&[TokenKind::No, TokenKind::DataP]);
        } else if kinds.len() >= 2
            && kinds[kinds.len() - 2..] == [TokenKind::With, TokenKind::No]
            && parse_select_statement_tokens(tokens[..tokens.len() - 2].to_vec()).is_ok()
        {
            self.record_completion_tokens(&[TokenKind::DataP]);
        }
    }
}
pub(super) fn split_with_data_suffix(mut tokens: Vec<Token>) -> (Vec<Token>, bool) {
    let len = tokens.len();
    if len >= 3
        && tokens[len - 3].kind == TokenKind::With
        && tokens[len - 2].kind == TokenKind::No
        && tokens[len - 1].kind == TokenKind::DataP
    {
        tokens.truncate(len - 3);
        return (tokens, true);
    }
    if len >= 2
        && tokens[len - 2].kind == TokenKind::With
        && tokens[len - 1].kind == TokenKind::DataP
    {
        tokens.truncate(len - 2);
    }
    (tokens, false)
}
