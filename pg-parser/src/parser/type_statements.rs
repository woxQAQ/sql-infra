use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtype.html
    // CREATE TYPE name AS
    //     ( [ attribute_name data_type [ COLLATE collation ] [, ... ] ] )
    //
    // CREATE TYPE name AS ENUM
    //     ( [ 'label' [, ... ] ] )
    //
    // CREATE TYPE name AS RANGE (
    //     SUBTYPE = subtype
    //     [ , SUBTYPE_OPCLASS = subtype_operator_class ]
    //     [ , COLLATION = collation ]
    //     [ , CANONICAL = canonical_function ]
    //     [ , SUBTYPE_DIFF = subtype_diff_function ]
    //     [ , MULTIRANGE_TYPE_NAME = multirange_type_name ]
    // )
    //
    // CREATE TYPE name (
    //     INPUT = input_function,
    //     OUTPUT = output_function
    //     [ , RECEIVE = receive_function ]
    //     [ , SEND = send_function ]
    //     [ , TYPMOD_IN = type_modifier_input_function ]
    //     [ , TYPMOD_OUT = type_modifier_output_function ]
    //     [ , ANALYZE = analyze_function ]
    //     [ , SUBSCRIPT = subscript_function ]
    //     [ , INTERNALLENGTH = { internallength | VARIABLE } ]
    //     [ , PASSEDBYVALUE ]
    //     [ , ALIGNMENT = alignment ]
    //     [ , STORAGE = storage ]
    //     [ , LIKE = like_type ]
    //     [ , CATEGORY = category ]
    //     [ , PREFERRED = preferred ]
    //     [ , DEFAULT = default ]
    //     [ , ELEMENT = element ]
    //     [ , DELIMITER = delimiter ]
    //     [ , COLLATABLE = collatable ]
    // )
    //
    // CREATE TYPE name
    pub(super) fn parse_create_type(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TypeP)?;
        let type_location = self.location();
        let type_name = self.parse_name_list();
        if type_name.is_empty() {
            return Err(self.error_here("CREATE TYPE requires a type name"));
        }
        if !self.consume(TokenKind::As) {
            let definition = if self.at(TokenKind::Char('(')) {
                self.parse_parenthesized_definition()?
            } else {
                Vec::new()
            };
            return Ok(Node::DefineStmt(DefineStmt {
                node_tag: NodeTag::DefineStmt,
                kind: ObjectType::Type,
                defnames: type_name,
                definition,
                ..DefineStmt::default()
            }));
        }

        match self.peek_kind() {
            TokenKind::EnumP => {
                self.advance();
                self.expect(TokenKind::Char('('))?;
                let mut vals = Vec::new();
                while !self.at(TokenKind::Char(')')) {
                    if !self.at(TokenKind::SConst) {
                        return Err(self.error_here("enum labels must be string literals"));
                    }
                    let value = self.consume_string_like().unwrap_or_default();
                    vals.push(make_string_node(value));
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at(TokenKind::Char(')')) {
                        return Err(self.error_here("expected an enum label after ','"));
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                Ok(Node::CreateEnumStmt(CreateEnumStmt {
                    node_tag: NodeTag::CreateEnumStmt,
                    type_name,
                    vals,
                }))
            }
            TokenKind::Range => {
                self.advance();
                let params = self.parse_parenthesized_definition()?;
                Ok(Node::CreateRangeStmt(CreateRangeStmt {
                    node_tag: NodeTag::CreateRangeStmt,
                    type_name,
                    params,
                }))
            }
            TokenKind::Char('(') => {
                self.advance();
                let mut coldeflist = Vec::new();
                while !self.at(TokenKind::Char(')')) {
                    coldeflist.push(*self.parse_table_func_element_until(&[
                        TokenKind::Char(','),
                        TokenKind::Char(')'),
                    ])?);
                    if !self.consume(TokenKind::Char(',')) {
                        break;
                    }
                    if self.at(TokenKind::Char(')')) {
                        return Err(self.error_here("expected a composite attribute after ','"));
                    }
                }
                self.expect(TokenKind::Char(')'))?;
                Ok(Node::CompositeTypeStmt(CompositeTypeStmt {
                    node_tag: NodeTag::CompositeTypeStmt,
                    typevar: Some(Box::new(range_var_from_parts(
                        list_to_names(&type_name),
                        type_location,
                    ))),
                    coldeflist,
                }))
            }
            _ => Err(self.error_here("expected ENUM, RANGE, or a composite attribute list")),
        }
    }

    // PostgreSQL 18 Synopsis subset — base type properties
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name SET ( property = value [, ... ] )
    pub(super) fn parse_alter_type(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TypeP)?;
        let type_name = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if type_name.is_empty() {
            return Err(self.error_here("ALTER TYPE requires a type name"));
        }
        self.expect(TokenKind::Set)?;
        let options = self.parse_operator_definition_list()?;
        self.expect_statement_end()?;
        Ok(Node::AlterTypeStmt(AlterTypeStmt {
            node_tag: NodeTag::AlterTypeStmt,
            type_name,
            options,
        }))
    }

    // PostgreSQL 18 Synopsis subset — enum values
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name
    //     ADD VALUE [ IF NOT EXISTS ] new_enum_value
    //         [ { BEFORE | AFTER } neighbor_enum_value ]
    // ALTER TYPE name RENAME VALUE existing_enum_value TO new_enum_value
    pub(super) fn parse_alter_enum(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TypeP)?;
        let type_name = self.parse_name_list_until_keywords(&[
            TokenKind::AddP,
            TokenKind::Rename,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let mut stmt = AlterEnumStmt {
            node_tag: NodeTag::AlterEnumStmt,
            type_name,
            ..AlterEnumStmt::default()
        };
        if stmt.type_name.is_empty() {
            return Err(self.error_here("ALTER TYPE requires an enum type name"));
        }

        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                self.expect(TokenKind::ValueP)?;
                stmt.skip_if_new_val_exists = self.consume_if_not_exists()?;
                stmt.new_val = Some(self.consume_required_string("ADD VALUE requires a string")?);
                if self.consume(TokenKind::Before) {
                    stmt.new_val_neighbor =
                        Some(self.consume_required_string("BEFORE requires an enum value string")?);
                    stmt.new_val_is_after = false;
                } else if self.consume(TokenKind::After) {
                    stmt.new_val_neighbor =
                        Some(self.consume_required_string("AFTER requires an enum value string")?);
                    stmt.new_val_is_after = true;
                } else {
                    stmt.new_val_is_after = true;
                }
            }
            TokenKind::Rename => {
                self.advance();
                self.expect(TokenKind::ValueP)?;
                stmt.old_val =
                    Some(self.consume_required_string("RENAME VALUE requires a string")?);
                self.expect(TokenKind::To)?;
                stmt.new_val = Some(self.consume_required_string("TO requires a string")?);
            }
            TokenKind::Drop => {
                self.advance();
                self.expect(TokenKind::ValueP)?;
                self.consume_required_string("DROP VALUE requires a string")?;
                return Err(ParseError::new(
                    self.previous_location(),
                    "dropping an enum value is not implemented",
                ));
            }
            _ => {
                return Err(self.error_here("ALTER TYPE enum requires ADD, RENAME, or DROP VALUE"));
            }
        }
        self.expect_statement_end()?;
        Ok(Node::AlterEnumStmt(stmt))
    }

    // PostgreSQL 18 Synopsis subset — composite attributes
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name action [, ... ]
    //
    // where action is one of:
    //     ADD ATTRIBUTE attribute_name data_type [ COLLATE collation ]
    //         [ CASCADE | RESTRICT ]
    //     DROP ATTRIBUTE [ IF EXISTS ] attribute_name [ CASCADE | RESTRICT ]
    //     ALTER ATTRIBUTE attribute_name [ SET DATA ] TYPE data_type
    //         [ COLLATE collation ] [ CASCADE | RESTRICT ]
    pub(super) fn parse_alter_composite_type(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TypeP)?;
        let type_location = self.location();
        let names = self.parse_name_list_until_keywords(&[
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Alter,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if names.is_empty() {
            return Err(self.error_here("ALTER TYPE requires a composite type name"));
        }
        let relation = Some(Box::new(range_var_from_parts(
            list_to_names(&names),
            type_location,
        )));
        let mut cmds = Vec::new();
        loop {
            cmds.push(Node::AlterTableCmd(self.parse_alter_composite_type_cmd()?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_statement_end() {
                return Err(self.error_here("expected an ALTER TYPE command after ','"));
            }
        }
        self.expect_statement_end()?;
        Ok(Node::AlterTableStmt(AlterTableStmt {
            node_tag: NodeTag::AlterTableStmt,
            relation,
            cmds,
            objtype: ObjectType::Type,
            ..AlterTableStmt::default()
        }))
    }

    fn parse_alter_composite_type_cmd(&mut self) -> PResult<AlterTableCmd> {
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                self.expect(TokenKind::Attribute)?;
                cmd.subtype = AlterTableType::AddColumn;
                cmd.def = Some(self.parse_table_func_element_until(&[
                    TokenKind::Cascade,
                    TokenKind::Restrict,
                    TokenKind::Char(','),
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])?);
                cmd.behavior = self.parse_drop_behavior();
            }
            TokenKind::Drop => {
                self.advance();
                self.expect(TokenKind::Attribute)?;
                cmd.subtype = AlterTableType::DropColumn;
                cmd.missing_ok = self.consume_if_exists()?;
                cmd.name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("DROP ATTRIBUTE requires a name"))?,
                );
                cmd.behavior = self.parse_drop_behavior();
            }
            TokenKind::Alter => {
                self.advance();
                self.expect(TokenKind::Attribute)?;
                cmd.subtype = AlterTableType::AlterColumnType;
                let attribute_location = self.location();
                cmd.name = Some(
                    self.consume_col_id()
                        .ok_or_else(|| self.error_here("ALTER ATTRIBUTE requires a name"))?,
                );
                if self.consume(TokenKind::Set) {
                    self.expect(TokenKind::DataP)?;
                }
                self.expect(TokenKind::TypeP)?;
                let type_name = Some(Box::new(
                    self.parse_type_name_until(&[
                        TokenKind::Collate,
                        TokenKind::Cascade,
                        TokenKind::Restrict,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])
                    .ok_or_else(|| self.error_here("ALTER ATTRIBUTE TYPE requires a data type"))?,
                ));
                let coll_clause = if self.consume(TokenKind::Collate) {
                    let location = self.previous_location();
                    let collname = self.parse_name_list_until_keywords(&[
                        TokenKind::Cascade,
                        TokenKind::Restrict,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                    if collname.is_empty() {
                        return Err(self.error_here("COLLATE requires a collation name"));
                    }
                    Some(Box::new(CollateClause {
                        node_tag: NodeTag::CollateClause,
                        collname,
                        location: location as ParseLoc,
                        ..CollateClause::default()
                    }))
                } else {
                    None
                };
                cmd.def = Some(Box::new(Node::ColumnDef(ColumnDef {
                    node_tag: NodeTag::ColumnDef,
                    type_name,
                    coll_clause,
                    location: attribute_location as ParseLoc,
                    ..ColumnDef::default()
                })));
                cmd.behavior = self.parse_drop_behavior();
            }
            _ => {
                return Err(self.error_here("expected ADD, DROP, or ALTER ATTRIBUTE"));
            }
        }
        Ok(cmd)
    }
}
