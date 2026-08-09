//! Prefix and primary-expression recognition for [`ExprParser`].
//!
//! Literals, unary operators, names, parenthesized forms, and constructor starts
//! enter the precedence parser through this module.

use super::expression::{AND_BINDING_POWER, ExprParser, UMINUS_BINDING_POWER};
use super::*;

const COMMON_EXPRESSION_START_TOKENS: &[TokenKind] = &[
    TokenKind::Exists,
    TokenKind::Array,
    TokenKind::Case,
    TokenKind::Grouping,
    TokenKind::Collation,
    TokenKind::Cast,
    TokenKind::Treat,
    TokenKind::Extract,
    TokenKind::Normalize,
    TokenKind::Position,
    TokenKind::Overlay,
    TokenKind::Substring,
    TokenKind::Trim,
    TokenKind::Xmlexists,
    TokenKind::SystemUser,
    TokenKind::CurrentDate,
    TokenKind::CurrentTime,
    TokenKind::CurrentTimestamp,
    TokenKind::Localtime,
    TokenKind::Localtimestamp,
    TokenKind::CurrentRole,
    TokenKind::CurrentUser,
    TokenKind::User,
    TokenKind::SessionUser,
    TokenKind::CurrentCatalog,
    TokenKind::CurrentSchema,
    TokenKind::Xmlconcat,
    TokenKind::Xmlelement,
    TokenKind::Xmlforest,
    TokenKind::Xmlparse,
    TokenKind::Xmlpi,
    TokenKind::Xmlroot,
    TokenKind::Xmlserialize,
    TokenKind::Json,
    TokenKind::JsonObject,
    TokenKind::JsonArray,
    TokenKind::JsonScalar,
    TokenKind::JsonSerialize,
    TokenKind::JsonQuery,
    TokenKind::JsonExists,
    TokenKind::JsonValue,
    TokenKind::JsonObjectagg,
    TokenKind::JsonArrayagg,
    TokenKind::MergeAction,
    TokenKind::Row,
    TokenKind::Char('('),
    TokenKind::Coalesce,
    TokenKind::Greatest,
    TokenKind::Least,
    TokenKind::Nullif,
    TokenKind::NullP,
    TokenKind::TrueP,
    TokenKind::FalseP,
];

pub(super) struct PrefixExpression {
    pub(super) node: Node,
    pub(super) is_row_syntax: bool,
}

impl ExprParser {
    pub(super) fn parse_c_expr(&mut self) -> Option<Node> {
        if self.at_completion() {
            self.record_completion_expression_start_tokens(COMMON_EXPRESSION_START_TOKENS);
            self.record_completion_slot(completion::GrammarSlot::Column);
            self.record_completion_slot(completion::GrammarSlot::Function);
            if let Some(hole) = self.recover_completion_hole() {
                return token_to_leaf(&hole);
            }
            return self.fail("completion point at common expression start");
        }
        if matches!(
            self.peek_kind(),
            TokenKind::Not
                | TokenKind::Char('+')
                | TokenKind::Char('-')
                | TokenKind::Char('*')
                | TokenKind::Char('|')
                | TokenKind::RightArrow
                | TokenKind::Op
                | TokenKind::Operator
                | TokenKind::Default
                | TokenKind::Unique
        ) || (self.peek_kind() == TokenKind::CurrentP && self.peek_kind_n(1) == TokenKind::Of)
        {
            return self.fail("token cannot start a common expression");
        }

        let prefix_kind = self.peek_kind();
        let mut lhs = self.parse_prefix(false)?.node;
        let indirection_allowed = prefix_kind == TokenKind::Char('(')
            || matches!(lhs, Node::ColumnRef(_) | Node::ParamRef(_));
        let mut indirection_ends_in_star =
            prefix_kind != TokenKind::Char('(') && node_ends_in_star_indirection(&lhs);
        loop {
            lhs = match self.peek_kind() {
                TokenKind::Char('[') if indirection_allowed && !indirection_ends_in_star => {
                    let index = self.parse_indirection_index()?;
                    append_indirection(lhs, index)
                }
                TokenKind::Char('.') if indirection_allowed && !indirection_ends_in_star => {
                    self.advance();
                    let item = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let name = self
                            .consume_column_label()
                            .or_else(|| self.fail("expected a field name after '.'"))?;
                        make_string_node(name)
                    };
                    indirection_ends_in_star = matches!(item, Node::AStar(_));
                    append_indirection(lhs, item)
                }
                _ => break,
            };
        }
        Some(lhs)
    }

    pub(super) fn parse_prefix(&mut self, restricted: bool) -> Option<PrefixExpression> {
        if self.at_completion() {
            self.record_completion_expression_start_tokens(COMMON_EXPRESSION_START_TOKENS);
            self.record_completion_expression_start_tokens(&[
                TokenKind::Char('+'),
                TokenKind::Char('-'),
                TokenKind::RightArrow,
                TokenKind::Char('|'),
                TokenKind::Op,
                TokenKind::Operator,
            ]);
            if !restricted {
                self.record_completion_expression_start_tokens(&[
                    TokenKind::Not,
                    TokenKind::Default,
                ]);
            }
            self.record_completion_slot(completion::GrammarSlot::Column);
            self.record_completion_slot(completion::GrammarSlot::Function);
            if let Some(hole) = self.recover_completion_hole() {
                return token_to_leaf(&hole).map(|node| PrefixExpression {
                    node,
                    is_row_syntax: false,
                });
            }
            return self.fail("completion point at expression start");
        }
        if let Some(constant) = self.try_parse_typed_constant() {
            return Some(PrefixExpression {
                node: constant,
                is_row_syntax: false,
            });
        }
        let token = self.peek().clone();
        if token.kind == TokenKind::Collation && self.peek_kind_n(1) == TokenKind::Completion {
            self.advance();
            self.record_completion_tokens(&[TokenKind::For]);
            return self.fail("COLLATION requires FOR");
        }
        let mut is_row_syntax = false;
        let node = match token.kind {
            TokenKind::Not => {
                if restricted {
                    return self.fail("NOT is not allowed in a restricted expression");
                }
                let location = self.advance().location();
                let arg = self.parse_expr_mode(AND_BINDING_POWER + 1, restricted)?;
                Some(Node::BoolExpr(BoolExpr {
                    xpr: Expr::new(NodeTag::BoolExpr),
                    boolop: BoolExprType::NotExpr,
                    args: vec![arg],
                    location: location as ParseLoc,
                }))
            }
            TokenKind::Char('+') => {
                let token = self.advance().clone();
                let rhs = self.parse_expr_mode(UMINUS_BINDING_POWER, restricted)?;
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
                let rhs = self.parse_expr_mode(UMINUS_BINDING_POWER, restricted)?;
                Some(negate_node(rhs, location))
            }
            TokenKind::RightArrow | TokenKind::Char('|') | TokenKind::Op => {
                let token = self.advance().clone();
                let rhs = self.parse_expr_mode(41, restricted)?;
                Some(make_aexpr(
                    AExprKind::Op,
                    vec![token_name(&token).unwrap_or_else(|| token_text(&token))],
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
                is_row_syntax = true;
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
            TokenKind::Char('(') => {
                let (node, row_syntax) = self.parse_parenthesized_expr()?;
                is_row_syntax = row_syntax;
                Some(node)
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
        }?;
        Some(PrefixExpression {
            node,
            is_row_syntax,
        })
    }
}
