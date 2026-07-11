use super::*;

impl Parser {
    pub(super) fn parse_create_op_class(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Class)?;
        let opclassname = self.parse_name_list_until_keywords(&[
            TokenKind::Default,
            TokenKind::For,
            TokenKind::Using,
            TokenKind::As,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if opclassname.is_empty() {
            return Err(self.error_here("CREATE OPERATOR CLASS requires a name"));
        }
        let is_default = self.consume(TokenKind::Default);
        self.expect(TokenKind::For)?;
        self.expect(TokenKind::TypeP)?;
        let datatype = self
            .parse_type_name_until(&[TokenKind::Using, TokenKind::As, TokenKind::Eof])
            .map(Box::new)
            .ok_or_else(|| self.error_here("operator class requires a data type"))?;
        self.expect(TokenKind::Using)?;
        let amname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?,
        );
        let opfamilyname = if self.consume(TokenKind::Family) {
            let family = self.parse_name_list_until_keywords(&[
                TokenKind::As,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            if family.is_empty() {
                return Err(self.error_here("FAMILY requires a name"));
            }
            family
        } else {
            Vec::new()
        };
        self.expect(TokenKind::As)?;
        let items = self.parse_opclass_item_list(&[TokenKind::Char(';'), TokenKind::Eof])?;
        Ok(Node::CreateOpClassStmt(CreateOpClassStmt {
            node_tag: NodeTag::CreateOpClassStmt,
            opclassname,
            opfamilyname,
            amname,
            datatype: Some(datatype),
            items,
            is_default,
        }))
    }

    pub(super) fn parse_create_op_family(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Family)?;
        let opfamilyname = self.parse_name_list_until_keywords(&[
            TokenKind::Using,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if opfamilyname.is_empty() {
            return Err(self.error_here("CREATE OPERATOR FAMILY requires a name"));
        }
        self.expect(TokenKind::Using)?;
        let amname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?,
        );
        Ok(Node::CreateOpFamilyStmt(CreateOpFamilyStmt {
            node_tag: NodeTag::CreateOpFamilyStmt,
            opfamilyname,
            amname,
        }))
    }

    pub(super) fn parse_alter_op_family(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Family)?;
        let opfamilyname = self.parse_name_list_until_keywords(&[
            TokenKind::Using,
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if opfamilyname.is_empty() {
            return Err(self.error_here("ALTER OPERATOR FAMILY requires a name"));
        }
        self.expect(TokenKind::Using)?;
        let amname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?,
        );
        let is_drop = if self.consume(TokenKind::AddP) {
            false
        } else if self.consume(TokenKind::Drop) {
            true
        } else {
            return Err(self.error_here("ALTER OPERATOR FAMILY requires ADD or DROP"));
        };
        let items = if is_drop {
            self.parse_opclass_drop_list()?
        } else {
            self.parse_opclass_item_list(&[TokenKind::Char(';'), TokenKind::Eof])?
        };
        self.expect_statement_end()?;
        Ok(Node::AlterOpFamilyStmt(AlterOpFamilyStmt {
            node_tag: NodeTag::AlterOpFamilyStmt,
            opfamilyname,
            amname,
            is_drop,
            items,
        }))
    }

    pub(super) fn parse_table_func_element_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<Box<Node>> {
        let location = self.location();
        let colname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("expected an attribute name"))?,
        );
        let type_name = Some(Box::new(
            self.parse_type_name_until(&extend_stops(stops, TokenKind::Collate))
                .ok_or_else(|| self.error_here("attribute requires a data type"))?,
        ));
        let coll_clause = if self.consume(TokenKind::Collate) {
            let coll_location = self.previous_location();
            let collname = self.parse_name_list_until_keywords(stops);
            if collname.is_empty() {
                return Err(self.error_here("COLLATE requires a collation name"));
            }
            Some(Box::new(CollateClause {
                node_tag: NodeTag::CollateClause,
                collname,
                location: coll_location as ParseLoc,
                ..CollateClause::default()
            }))
        } else {
            None
        };
        Ok(Box::new(Node::ColumnDef(ColumnDef {
            node_tag: NodeTag::ColumnDef,
            colname,
            type_name,
            is_local: true,
            coll_clause,
            location: location as ParseLoc,
            ..ColumnDef::default()
        })))
    }

    pub(super) fn parse_opclass_item_list(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let itemtype = match self.peek_kind() {
                TokenKind::Operator => {
                    self.advance();
                    1
                }
                TokenKind::Function => {
                    self.advance();
                    2
                }
                TokenKind::Storage => {
                    self.advance();
                    3
                }
                _ => return Err(self.error_here("expected OPERATOR, FUNCTION, or STORAGE")),
            };
            let number = if itemtype != 3 {
                let token = self.expect(TokenKind::IConst)?;
                match token.value {
                    Some(TokenValue::Integer(value)) => value,
                    _ => return Err(ParseError::new(token.location, "expected item number")),
                }
            } else {
                0
            };
            let mut item = CreateOpClassItem {
                node_tag: NodeTag::CreateOpClassItem,
                itemtype,
                number,
                ..CreateOpClassItem::default()
            };
            if itemtype == 3 {
                item.storedtype = Some(Box::new(
                    self.parse_type_name_until(&[
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])
                    .ok_or_else(|| self.error_here("STORAGE requires a type"))?,
                ));
            } else {
                if itemtype == 2 && self.consume(TokenKind::Char('(')) {
                    let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
                    self.expect(TokenKind::Char(')'))?;
                    item.class_args = parse_type_node_list(tokens)?;
                }
                let name = if itemtype == 1 {
                    self.parse_opclass_operator_until(&[
                        TokenKind::For,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])?
                } else {
                    self.parse_object_with_args_until(&[
                        TokenKind::For,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])?
                };
                item.name = Some(Box::new(name));
                if self.consume(TokenKind::For) {
                    if itemtype != 1 {
                        return Err(self.error_here(
                            "FOR SEARCH / FOR ORDER BY is only valid for operator class operators",
                        ));
                    }
                    if self.consume(TokenKind::Order) {
                        self.expect(TokenKind::By)?;
                        item.order_family = self.parse_name_list_until_keywords(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ]);
                        if item.order_family.is_empty() {
                            return Err(self.error_here("ORDER BY requires an operator family"));
                        }
                    } else {
                        self.expect(TokenKind::Search)?;
                    }
                }
            }
            items.push(Node::CreateOpClassItem(item));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an operator class item after ','"));
            }
        }
        if items.is_empty() {
            return Err(self.error_here("operator class requires at least one item"));
        }
        Ok(items)
    }

    pub(super) fn parse_opclass_drop_list(&mut self) -> PResult<NodeList> {
        let mut items = Vec::new();
        loop {
            let itemtype = if self.consume(TokenKind::Operator) {
                1
            } else if self.consume(TokenKind::Function) {
                2
            } else {
                return Err(self.error_here("expected OPERATOR or FUNCTION"));
            };
            let number = match self.expect(TokenKind::IConst)?.value {
                Some(TokenValue::Integer(number)) => number,
                _ => return Err(self.error_here("expected an operator family item number")),
            };
            self.expect(TokenKind::Char('('))?;
            let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            let class_args = parse_type_node_list(tokens)?;
            if class_args.is_empty() {
                return Err(self.error_here("operator family item requires argument types"));
            }
            self.expect(TokenKind::Char(')'))?;
            items.push(Node::CreateOpClassItem(CreateOpClassItem {
                node_tag: NodeTag::CreateOpClassItem,
                itemtype,
                number,
                class_args,
                ..CreateOpClassItem::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_statement_end() {
                return Err(self.error_here("expected an operator family item after ','"));
            }
        }
        Ok(items)
    }
}
