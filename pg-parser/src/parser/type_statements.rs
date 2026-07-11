use super::*;

impl Parser {
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

        if self.consume(TokenKind::EnumP) {
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
        } else if self.consume(TokenKind::Range) {
            let params = self.parse_parenthesized_definition()?;
            Ok(Node::CreateRangeStmt(CreateRangeStmt {
                node_tag: NodeTag::CreateRangeStmt,
                type_name,
                params,
            }))
        } else if self.consume(TokenKind::Char('(')) {
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
        } else {
            Err(self.error_here("expected ENUM, RANGE, or a composite attribute list"))
        }
    }
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

        if self.consume(TokenKind::AddP) {
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
        } else if self.consume(TokenKind::Rename) {
            self.expect(TokenKind::ValueP)?;
            stmt.old_val = Some(self.consume_required_string("RENAME VALUE requires a string")?);
            self.expect(TokenKind::To)?;
            stmt.new_val = Some(self.consume_required_string("TO requires a string")?);
        } else if self.consume(TokenKind::Drop) {
            self.expect(TokenKind::ValueP)?;
            self.consume_required_string("DROP VALUE requires a string")?;
            return Err(ParseError::new(
                self.previous_location(),
                "dropping an enum value is not implemented",
            ));
        } else {
            return Err(self.error_here("ALTER TYPE enum requires ADD, RENAME, or DROP VALUE"));
        }
        self.expect_statement_end()?;
        Ok(Node::AlterEnumStmt(stmt))
    }

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
        if self.consume(TokenKind::AddP) {
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
        } else if self.consume(TokenKind::Drop) {
            self.expect(TokenKind::Attribute)?;
            cmd.subtype = AlterTableType::DropColumn;
            cmd.missing_ok = self.consume_if_exists()?;
            cmd.name = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("DROP ATTRIBUTE requires a name"))?,
            );
            cmd.behavior = self.parse_drop_behavior();
        } else if self.consume(TokenKind::Alter) {
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
        } else {
            return Err(self.error_here("expected ADD, DROP, or ALTER ATTRIBUTE"));
        }
        Ok(cmd)
    }
}
