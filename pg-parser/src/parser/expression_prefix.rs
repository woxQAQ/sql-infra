use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_c_expr(&mut self) -> Option<Node> {
        if matches!(
            self.peek_kind(),
            TokenKind::Not
                | TokenKind::Char('+')
                | TokenKind::Char('-')
                | TokenKind::Char('*')
                | TokenKind::Op
                | TokenKind::Operator
                | TokenKind::Default
                | TokenKind::Unique
        ) || (self.peek_kind() == TokenKind::CurrentP && self.peek_kind_n(1) == TokenKind::Of)
        {
            return self.fail("token cannot start a common expression");
        }

        let mut lhs = self.parse_prefix(false)?;
        loop {
            lhs = match self.peek_kind() {
                TokenKind::Char('[') => {
                    let index = self.parse_indirection_index()?;
                    append_indirection(lhs, index)
                }
                TokenKind::Char('.') => {
                    self.advance();
                    let item = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let name = self
                            .consume_identifier_in_categories(&[
                                KeywordCategory::Unreserved,
                                KeywordCategory::ColName,
                                KeywordCategory::TypeFuncName,
                                KeywordCategory::Reserved,
                            ])
                            .or_else(|| self.fail("expected a field name after '.'"))?;
                        make_string_node(name)
                    };
                    append_indirection(lhs, item)
                }
                _ => break,
            };
        }
        Some(lhs)
    }

    pub(super) fn parse_prefix(&mut self, restricted: bool) -> Option<Node> {
        if let Some(constant) = self.try_parse_typed_constant() {
            return Some(constant);
        }
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Not => {
                if restricted {
                    return self.fail("NOT is not allowed in a restricted expression");
                }
                let location = self.advance().location();
                let arg = self.parse_expr_mode(60, restricted)?;
                Some(Node::BoolExpr(BoolExpr {
                    xpr: Expr::new(NodeTag::BoolExpr),
                    boolop: BoolExprType::NotExpr,
                    args: vec![arg],
                    location: location as ParseLoc,
                }))
            }
            TokenKind::Char('+') => {
                let token = self.advance().clone();
                let rhs = self.parse_expr_mode(70, restricted)?;
                Some(make_aexpr(
                    AExprKind::Op,
                    vec![token_text(&token)],
                    None,
                    Some(rhs),
                    token.location(),
                ))
            }
            TokenKind::Char('-') => {
                let location = self.advance().location();
                let rhs = self.parse_expr_mode(70, restricted)?;
                Some(negate_node(rhs, location))
            }
            TokenKind::Op => {
                let token = self.advance().clone();
                let rhs = self.parse_expr_mode(41, restricted)?;
                Some(make_aexpr(
                    AExprKind::Op,
                    vec![token_name(&token)?],
                    None,
                    Some(rhs),
                    token.location(),
                ))
            }
            TokenKind::Operator => {
                let location = self.location();
                let name = self.parse_explicit_operator_name()?;
                let rhs = self.parse_expr_mode(41, restricted)?;
                Some(make_aexpr_with_name(
                    AExprKind::Op,
                    name,
                    None,
                    Some(rhs),
                    location,
                ))
            }
            TokenKind::Exists => {
                let location = self.advance().location();
                let subselect = self.parse_parenthesized_statement()?;
                Some(Node::SubLink(SubLink {
                    xpr: Expr::new(NodeTag::SubLink),
                    sub_link_type: SubLinkType::ExistsSublink,
                    subselect: Some(Box::new(subselect)),
                    location: location as ParseLoc,
                    ..SubLink::default()
                }))
            }
            TokenKind::Unique => self.fail("UNIQUE predicate is not yet implemented"),
            TokenKind::Array => {
                let location = self.advance().location();
                if self.consume(TokenKind::Char('[')) {
                    let list_start = self.previous_location();
                    self.parse_array_expr_body(location, list_start)
                } else {
                    self.parse_parenthesized_statement().map(|subselect| {
                        Node::SubLink(SubLink {
                            xpr: Expr::new(NodeTag::SubLink),
                            sub_link_type: SubLinkType::ArraySublink,
                            subselect: Some(Box::new(subselect)),
                            location: location as ParseLoc,
                            ..SubLink::default()
                        })
                    })
                }
            }
            TokenKind::Case => self.parse_case_expr(),
            TokenKind::Default => {
                if restricted {
                    return self.fail("DEFAULT is not allowed in a restricted expression");
                }
                let location = self.advance().location();
                Some(Node::SetToDefault(SetToDefault {
                    xpr: Expr::new(NodeTag::SetToDefault),
                    location: location as ParseLoc,
                    ..SetToDefault::default()
                }))
            }
            TokenKind::Grouping => self.parse_grouping_func(),
            TokenKind::Collation if self.peek_kind_n(1) == TokenKind::For => {
                let location = self.advance().location();
                self.advance();
                self.expect(TokenKind::Char('('))?;
                let arg = self.parse_expr(0)?;
                self.expect(TokenKind::Char(')'))?;
                Some(self.make_sql_syntax_call("pg_collation_for", vec![arg], location))
            }
            TokenKind::Cast | TokenKind::Treat => self.parse_cast_or_treat(),
            TokenKind::Extract => self.parse_extract_func(),
            TokenKind::Normalize => self.parse_normalize_func(),
            TokenKind::Position => self.parse_position_func(),
            TokenKind::Overlay => self.parse_overlay_func(),
            TokenKind::Substring => self.parse_substring_func(),
            TokenKind::Trim => self.parse_trim_func(),
            TokenKind::Xmlexists => self.parse_xmlexists_func(),
            TokenKind::SystemUser => {
                let location = self.advance().location();
                Some(Node::FuncCall(FuncCall {
                    node_tag: NodeTag::FuncCall,
                    funcname: system_type_names("system_user"),
                    funcformat: CoercionForm::SqlSyntax,
                    location: location as ParseLoc,
                    ..FuncCall::default()
                }))
            }
            TokenKind::CurrentDate
            | TokenKind::CurrentTime
            | TokenKind::CurrentTimestamp
            | TokenKind::Localtime
            | TokenKind::Localtimestamp
            | TokenKind::CurrentRole
            | TokenKind::CurrentUser
            | TokenKind::User
            | TokenKind::SessionUser
            | TokenKind::CurrentCatalog
            | TokenKind::CurrentSchema => self.parse_sql_value_function(),
            TokenKind::Xmlconcat
            | TokenKind::Xmlelement
            | TokenKind::Xmlforest
            | TokenKind::Xmlparse
            | TokenKind::Xmlpi
            | TokenKind::Xmlroot => self.parse_xml_expr(),
            TokenKind::Xmlserialize => self.parse_xml_serialize(),
            TokenKind::Json
            | TokenKind::JsonObject
            | TokenKind::JsonArray
            | TokenKind::JsonScalar
            | TokenKind::JsonSerialize
            | TokenKind::JsonQuery
            | TokenKind::JsonExists
            | TokenKind::JsonValue
            | TokenKind::JsonObjectagg
            | TokenKind::JsonArrayagg => self.parse_json_expression(),
            TokenKind::MergeAction => self.parse_merge_support_func(),
            TokenKind::Row => {
                let location = self.advance().location();
                self.expect(TokenKind::Char('('))?;
                let args = self.parse_expr_list_until(TokenKind::Char(')'))?;
                self.expect(TokenKind::Char(')'))?;
                Some(Node::RowExpr(RowExpr {
                    xpr: Expr::new(NodeTag::RowExpr),
                    args,
                    row_format: CoercionForm::ExplicitCall,
                    location: location as ParseLoc,
                    ..RowExpr::default()
                }))
            }
            TokenKind::Char('(') => self.parse_parenthesized_expr(),
            TokenKind::Char('*') => {
                self.advance();
                Some(Node::AStar(AStar {
                    node_tag: NodeTag::AStar,
                }))
            }
            TokenKind::Coalesce => self.parse_keyword_call_as_coalesce(),
            TokenKind::Greatest | TokenKind::Least => self.parse_keyword_call_as_minmax(),
            TokenKind::Nullif => self.parse_keyword_call_as_aexpr(AExprKind::Nullif),
            _ => {
                if let Some(leaf) = token_to_leaf(&token) {
                    if matches!(
                        token.kind,
                        TokenKind::IConst
                            | TokenKind::FConst
                            | TokenKind::SConst
                            | TokenKind::BConst
                            | TokenKind::XConst
                            | TokenKind::Param
                            | TokenKind::NullP
                            | TokenKind::TrueP
                            | TokenKind::FalseP
                    ) {
                        self.advance();
                        Some(leaf)
                    } else {
                        self.parse_name_or_func()
                    }
                } else {
                    self.parse_name_or_func()
                }
            }
        }
    }
}
