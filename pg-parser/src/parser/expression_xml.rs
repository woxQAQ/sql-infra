use super::expression::ExprParser;
use super::*;

impl ExprParser {
    pub(super) fn parse_xml_expr(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let mut expr = XmlExpr {
            xpr: Expr::new(NodeTag::XmlExpr),
            op: match token.kind {
                TokenKind::Xmlconcat => XmlExprOp::Xmlconcat,
                TokenKind::Xmlelement => XmlExprOp::Xmlelement,
                TokenKind::Xmlforest => XmlExprOp::Xmlforest,
                TokenKind::Xmlparse => XmlExprOp::Xmlparse,
                TokenKind::Xmlpi => XmlExprOp::Xmlpi,
                TokenKind::Xmlroot => XmlExprOp::Xmlroot,
                _ => return None,
            },
            location: token.location() as ParseLoc,
            ..XmlExpr::default()
        };
        match token.kind {
            TokenKind::Xmlconcat => {
                expr.args = self.parse_expr_list_until(TokenKind::Char(')'))?;
                if expr.args.is_empty() {
                    return self.fail("XMLCONCAT requires at least one argument");
                }
            }
            TokenKind::Xmlelement => {
                self.parse_xmlelement_arguments(&mut expr)?;
            }
            TokenKind::Xmlforest => {
                expr.named_args = self.parse_xml_labeled_expr_list(TokenKind::Char(')'))?;
                if expr.named_args.is_empty() {
                    return self.fail("XMLFOREST requires at least one expression");
                }
            }
            TokenKind::Xmlparse => {
                self.parse_xmlparse_arguments(&mut expr)?;
            }
            TokenKind::Xmlpi => {
                self.expect(TokenKind::NameP)?;
                expr.name = Some(
                    self.consume_column_label()
                        .or_else(|| self.fail("XMLPI NAME requires a column label"))?,
                );
                if self.consume(TokenKind::Char(',')) {
                    expr.args.push(self.parse_expr(0)?);
                }
            }
            TokenKind::Xmlroot => {
                self.parse_xmlroot_arguments(&mut expr)?;
            }
            _ => return None,
        }
        self.expect(TokenKind::Char(')'))?;
        Some(Node::XmlExpr(expr))
    }

    fn parse_xmlelement_arguments(&mut self, expr: &mut XmlExpr) -> Option<()> {
        self.expect(TokenKind::NameP)?;
        expr.name = Some(
            self.consume_column_label()
                .or_else(|| self.fail("XMLELEMENT NAME requires a column label"))?,
        );
        if !self.consume(TokenKind::Char(',')) {
            return Some(());
        }
        if !self.consume(TokenKind::Xmlattributes) {
            expr.args = self.parse_expr_list_until(TokenKind::Char(')'))?;
            return Some(());
        }

        self.expect(TokenKind::Char('('))?;
        expr.named_args = self.parse_xml_labeled_expr_list(TokenKind::Char(')'))?;
        if expr.named_args.is_empty() {
            return self.fail("XMLATTRIBUTES requires at least one expression");
        }
        self.expect(TokenKind::Char(')'))?;
        if self.consume(TokenKind::Char(',')) {
            expr.args = self.parse_expr_list_until(TokenKind::Char(')'))?;
        }
        Some(())
    }

    fn parse_xmlparse_arguments(&mut self, expr: &mut XmlExpr) -> Option<()> {
        expr.xmloption = if self.consume(TokenKind::DocumentP) {
            XmlOptionType::Document
        } else {
            self.expect(TokenKind::ContentP)?;
            XmlOptionType::Content
        };
        let value = self.parse_expr(0)?;
        let preserve_whitespace = if self.consume(TokenKind::Preserve) {
            self.expect(TokenKind::WhitespaceP)?;
            true
        } else if self.consume(TokenKind::StripP) {
            self.expect(TokenKind::WhitespaceP)?;
            false
        } else {
            false
        };
        expr.args = vec![
            value,
            Node::AConst(AConst {
                node_tag: NodeTag::AConst,
                val: ValUnion::Boolean(Boolean::new(preserve_whitespace)),
                location: -1,
                ..AConst::default()
            }),
        ];
        Some(())
    }

    fn parse_xmlroot_arguments(&mut self, expr: &mut XmlExpr) -> Option<()> {
        expr.args.push(self.parse_expr(0)?);
        self.expect(TokenKind::Char(','))?;
        self.expect(TokenKind::VersionP)?;
        if self.consume(TokenKind::No) {
            self.expect(TokenKind::ValueP)?;
            expr.args.push(Node::AConst(AConst::null(-1)));
        } else {
            expr.args.push(self.parse_expr(0)?);
        }

        let standalone = if self.consume(TokenKind::Char(',')) {
            self.expect(TokenKind::StandaloneP)?;
            if self.consume(TokenKind::YesP) {
                0
            } else {
                self.expect(TokenKind::No)?;
                if self.consume(TokenKind::ValueP) {
                    2
                } else {
                    1
                }
            }
        } else {
            3
        };
        expr.args
            .push(Node::AConst(AConst::integer(standalone, -1)));
        Some(())
    }

    pub(super) fn parse_xml_labeled_expr_list(&mut self, stop: TokenKind) -> Option<NodeList> {
        let mut targets = Vec::new();
        while !self.at(stop) && !self.at(TokenKind::Eof) {
            let location = self.location();
            let value = self.parse_expr(0)?;
            let name = if self.consume(TokenKind::As) {
                Some(
                    self.consume_column_label()
                        .or_else(|| self.fail("XML alias requires a column label"))?,
                )
            } else {
                None
            };
            targets.push(Node::ResTarget(ResTarget {
                node_tag: NodeTag::ResTarget,
                name,
                val: Some(Box::new(value)),
                location: location as ParseLoc,
                ..ResTarget::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(stop) {
                return self.fail("expected an XML expression after ','");
            }
        }
        Some(targets)
    }

    pub(super) fn parse_xml_serialize(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Xmlserialize)?.location();
        self.expect(TokenKind::Char('('))?;
        let xmloption = if self.consume(TokenKind::DocumentP) {
            XmlOptionType::Document
        } else {
            self.expect(TokenKind::ContentP)?;
            XmlOptionType::Content
        };
        let expr = self.parse_expr(0)?;
        self.expect(TokenKind::As)?;
        let mut type_tokens = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Indent
                | TokenKind::No
                | TokenKind::Char(')')
                | TokenKind::Completion
                | TokenKind::Eof
        ) {
            type_tokens.push(self.advance().clone());
        }
        let completing_type = self.at_completion();
        if completing_type {
            let mut completion_tokens = type_tokens.clone();
            completion_tokens.push(self.peek().clone());
            record_simple_type_name_completion(&completion_tokens, self.completion.as_ref());
        }
        let type_name = match parse_simple_type_name_tokens(type_tokens) {
            Ok(type_name) => Box::new(type_name),
            Err(_) if completing_type => {
                return self.fail("completion point in XMLSERIALIZE type");
            }
            Err(error) => return self.fail_with(error),
        };
        if self.at_completion() {
            self.record_completion_lookahead_tokens(&[TokenKind::Indent, TokenKind::No]);
            self.record_completion_tokens(&[TokenKind::Char(')')]);
            return self.fail("completion point after XMLSERIALIZE type");
        }
        let indent = if self.consume(TokenKind::Indent) {
            true
        } else if self.consume(TokenKind::No) {
            self.expect(TokenKind::Indent)?;
            false
        } else {
            false
        };
        self.expect(TokenKind::Char(')'))?;
        Some(Node::XmlSerialize(XmlSerialize {
            node_tag: NodeTag::XmlSerialize,
            xmloption,
            expr: Some(Box::new(expr)),
            type_name: Some(type_name),
            indent,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_merge_support_func(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::MergeAction)?.location();
        self.expect(TokenKind::Char('('))?;
        self.expect(TokenKind::Char(')'))?;
        Some(Node::MergeSupportFunc(MergeSupportFunc {
            xpr: Expr::new(NodeTag::MergeSupportFunc),
            msftype: 25,
            location: location as ParseLoc,
            ..MergeSupportFunc::default()
        }))
    }
}
