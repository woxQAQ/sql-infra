//! Operator class and operator family statement parsing.
//!
//! Create, alter, add, and drop item grammars preserve operator/function
//! signatures, storage types, strategy numbers, and family identities.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createopclass.html
    // CREATE OPERATOR CLASS name [ DEFAULT ] FOR TYPE data_type
    //   USING index_method [ FAMILY family_name ] AS
    //   {  OPERATOR strategy_number operator_name [ ( op_type, op_type ) ] [ FOR SEARCH | FOR ORDER
    // BY sort_family_name ]    | FUNCTION support_number [ ( op_type [ , op_type ] ) ]
    // function_name ( argument_type [, ...] )    | STORAGE storage_type
    //   } [, ... ]
    pub(super) fn parse_create_op_class(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Class)?;
        let name_stops = [
            TokenKind::Default,
            TokenKind::For,
            TokenKind::Using,
            TokenKind::As,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        self.record_completion_slot(GrammarSlot::OperatorClass);
        self.record_completion_qualified_name_slot(GrammarSlot::OperatorClass, &name_stops);
        let opclassname = self.parse_name_list_until_keywords(&name_stops);
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
        let amname = Some(self.parse_access_method_name()?);
        let opfamilyname = if self.consume(TokenKind::Family) {
            let family_stops = [TokenKind::As, TokenKind::Char(';'), TokenKind::Eof];
            self.record_completion_slot(GrammarSlot::OperatorFamily);
            self.record_completion_qualified_name_slot(GrammarSlot::OperatorFamily, &family_stops);
            let family = self.parse_name_list_until_keywords(&family_stops);
            if family.is_empty() {
                return Err(self.error_here("FAMILY requires a name"));
            }
            family
        } else {
            Vec::new()
        };
        self.expect(TokenKind::As)?;
        let items = self.parse_opclass_item_list(STATEMENT_END_TOKENS)?;
        Ok(node!(CreateOpClassStmt {
            opclassname,
            opfamilyname,
            amname,
            datatype: Some(datatype),
            items,
            is_default,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createopfamily.html
    // CREATE OPERATOR FAMILY name USING index_method
    pub(super) fn parse_create_op_family(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Family)?;
        let name_stops = [TokenKind::Using, TokenKind::Char(';'), TokenKind::Eof];
        self.record_completion_slot(GrammarSlot::OperatorFamily);
        self.record_completion_qualified_name_slot(GrammarSlot::OperatorFamily, &name_stops);
        let opfamilyname = self.parse_name_list_until_keywords(&name_stops);
        if opfamilyname.is_empty() {
            return Err(self.error_here("CREATE OPERATOR FAMILY requires a name"));
        }
        self.expect(TokenKind::Using)?;
        let amname = Some(self.parse_access_method_name()?);
        Ok(node!(CreateOpFamilyStmt {
            opfamilyname,
            amname,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropfamily.html
    // ALTER OPERATOR FAMILY name USING index_method ADD
    //   {  OPERATOR strategy_number operator_name ( op_type, op_type )
    //               [ FOR SEARCH | FOR ORDER BY sort_family_name ]
    //    | FUNCTION support_number [ ( op_type [ , op_type ] ) ]
    //               function_name [ ( argument_type [, ...] ) ]
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method DROP
    //   {  OPERATOR strategy_number ( op_type [ , op_type ] )
    //    | FUNCTION support_number ( op_type [ , op_type ] )
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     SET SCHEMA new_schema
    pub(super) fn parse_alter_op_family(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Family)?;
        let name_stops = [
            TokenKind::Using,
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ];
        self.record_completion_slot(GrammarSlot::OperatorFamily);
        self.record_completion_qualified_name_slot(GrammarSlot::OperatorFamily, &name_stops);
        let opfamilyname = self.parse_name_list_until_keywords(&name_stops);
        if opfamilyname.is_empty() {
            return Err(self.error_here("ALTER OPERATOR FAMILY requires a name"));
        }
        self.expect(TokenKind::Using)?;
        let amname = Some(self.parse_access_method_name()?);
        self.record_completion_tokens(&[
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Rename,
            TokenKind::Set,
            TokenKind::Owner,
        ]);
        if self.consume(TokenKind::Set) {
            self.record_completion_tokens(&[TokenKind::Schema]);
            return Err(self.error_here("expected SET SCHEMA"));
        }
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
            self.parse_opclass_item_list(STATEMENT_END_TOKENS)?
        };
        self.expect_statement_end()?;
        Ok(node!(AlterOpFamilyStmt {
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
        self.record_completion_slot(GrammarSlot::AnyName);
        let colname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("expected an attribute name"))?,
        );
        let type_name = Some(Box::new(
            self.parse_type_name_until(&extend_stops(stops, TokenKind::Collate))
                .ok_or_else(|| self.error_here("attribute requires a data type"))?,
        ));
        let coll_clause = if self.consume(TokenKind::Collate) {
            self.record_completion_slot(GrammarSlot::Collation);
            let coll_location = self.previous_location();
            let collname = self.parse_name_list_until_keywords(stops);
            if collname.is_empty() {
                return Err(self.error_here("COLLATE requires a collation name"));
            }
            Some(Box::new(CollateClause {
                collname,
                location: coll_location as ParseLoc,
                ..CollateClause::default()
            }))
        } else {
            None
        };
        Ok(Box::new(node!(ColumnDef {
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
            self.record_completion_tokens(&[
                TokenKind::Operator,
                TokenKind::Function,
                TokenKind::Storage,
            ]);
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
                    _ => return Err(ParseError::ranged(token.range, "expected item number")),
                }
            } else {
                0
            };
            let mut item = CreateOpClassItem {
                itemtype,
                number,
                ..CreateOpClassItem::default()
            };
            if itemtype == 3 {
                item.storedtype = Some(Box::new(
                    self.parse_type_name_until(COMMA_OR_STATEMENT_END_TOKENS)
                        .ok_or_else(|| self.error_here("STORAGE requires a type"))?,
                ));
            } else {
                if itemtype == 2 && self.consume(TokenKind::Char('(')) {
                    self.record_completion_slot_within_fragment(
                        GrammarSlot::Type,
                        &[TokenKind::Char(')')],
                    );
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
                    if self.consume_phrase(&[TokenKind::Order, TokenKind::By])? {
                        item.order_family =
                            self.parse_name_list_until_keywords(COMMA_OR_STATEMENT_END_TOKENS);
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
            self.record_completion_tokens(&[TokenKind::Operator, TokenKind::Function]);
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
            self.record_completion_slot_within_fragment(GrammarSlot::Type, &[TokenKind::Char(')')]);
            let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            let class_args = parse_type_node_list(tokens)?;
            if class_args.is_empty() {
                return Err(self.error_here("operator family item requires argument types"));
            }
            self.expect(TokenKind::Char(')'))?;
            items.push(node!(CreateOpClassItem {
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
