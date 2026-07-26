use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createrule.html
    // CREATE [ OR REPLACE ] RULE name AS ON event
    //     TO table_name [ WHERE condition ]
    //     DO [ ALSO | INSTEAD ] { NOTHING | command | ( command ; command ... ) }
    //
    // where event can be one of:
    //
    //     SELECT | INSERT | UPDATE | DELETE
    pub(super) fn parse_rule(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::Rule)?;
        self.record_completion_slot(completion::GrammarSlot::Rule);
        let rulename = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE RULE requires a name"))?,
        );
        self.expect(TokenKind::As)?;
        self.expect(TokenKind::On)?;
        self.record_completion_tokens(&[
            TokenKind::Select,
            TokenKind::Update,
            TokenKind::Insert,
            TokenKind::DeleteP,
        ]);
        let event = match self.advance().kind {
            TokenKind::Select => CmdType::Select,
            TokenKind::Update => CmdType::Update,
            TokenKind::Insert => CmdType::Insert,
            TokenKind::DeleteP => CmdType::Delete,
            _ => {
                return Err(self.error_here("rule event must be SELECT, UPDATE, INSERT, or DELETE"));
            }
        };
        self.expect(TokenKind::To)?;
        let relation = Some(Box::new(
            self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Table)
                .ok_or_else(|| self.error_here("CREATE RULE requires a target relation"))?,
        ));
        let where_clause = if self.consume(TokenKind::Where) {
            Some(self.parse_expr_box_strict_until(&[
                TokenKind::Do,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])?)
        } else {
            None
        };
        self.expect(TokenKind::Do)?;
        let instead = self.consume(TokenKind::Instead);
        if !instead {
            self.consume(TokenKind::Also);
        }
        let actions = if self.consume(TokenKind::Nothing) {
            Vec::new()
        } else if self.consume(TokenKind::Char('(')) {
            let mut tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            if self.at_completion() && tokens.is_empty() {
                self.record_completion_tokens(&[
                    TokenKind::With,
                    TokenKind::Select,
                    TokenKind::Values,
                    TokenKind::Table,
                    TokenKind::Char('('),
                    TokenKind::Insert,
                    TokenKind::Update,
                    TokenKind::DeleteP,
                    TokenKind::Notify,
                ]);
                return Err(self.error_here("completion point in rule action list"));
            }
            if self.at_completion() {
                self.append_completion_marker(&mut tokens);
            }
            let actions =
                parse_statement_list_tokens_with_completion(tokens, self.completion.clone())?;
            self.expect(TokenKind::Char(')'))?;
            if actions.iter().any(|action| !is_rule_action(action)) {
                return Err(self.error_here("invalid statement in rule action list"));
            }
            actions
        } else if !self.at_statement_end() {
            let action = self.parse_statement(None)?;
            if !is_rule_action(&action) {
                return Err(self.error_here("invalid rule action statement"));
            }
            vec![action]
        } else {
            return Err(self.error_here("CREATE RULE requires an action or NOTHING"));
        };
        Ok(Node::RuleStmt(RuleStmt {
            node_tag: NodeTag::RuleStmt,
            relation,
            rulename,
            where_clause,
            event,
            instead,
            actions,
            replace,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createview.html
    // CREATE [ OR REPLACE ] [ TEMP | TEMPORARY ] [ RECURSIVE ] VIEW name [ ( column_name [, ...] ) ]
    //     [ WITH ( view_option_name [= view_option_value] [, ... ] ) ]
    //     AS query
    //     [ WITH [ CASCADED | LOCAL ] CHECK OPTION ]
    pub(super) fn parse_view(
        &mut self,
        replace: bool,
        relpersistence: u8,
        recursive: bool,
    ) -> PResult<Node> {
        self.expect(TokenKind::View)?;
        let mut view_node = self
            .try_parse_qualified_range_var_with_slot(completion::GrammarSlot::View)
            .ok_or_else(|| self.error_here("CREATE VIEW requires a view name"))?;
        view_node.relpersistence = relpersistence;
        let aliases = if self.consume(TokenKind::Char('(')) {
            let mut aliases = Vec::new();
            loop {
                let alias = self
                    .consume_col_id()
                    .ok_or_else(|| self.error_here("expected a CREATE VIEW column name"))?;
                aliases.push(make_string_node(alias));
                if !self.consume(TokenKind::Char(',')) {
                    break;
                }
            }
            if aliases.is_empty() {
                return Err(self.error_here("CREATE VIEW column list cannot be empty"));
            }
            self.expect(TokenKind::Char(')'))?;
            aliases
        } else {
            Vec::new()
        };
        if recursive && aliases.is_empty() {
            return Err(self.error_here("CREATE RECURSIVE VIEW requires a column list"));
        }
        let options = if self.consume(TokenKind::With) {
            let options = self.parse_parenthesized_reloptions()?;
            if options.is_empty() {
                return Err(self.error_here("CREATE VIEW WITH requires an option list"));
            }
            options
        } else {
            Vec::new()
        };
        self.expect(TokenKind::As)?;
        let tokens = self.take_until_top_level(&[TokenKind::Char(';'), TokenKind::Eof]);
        let (query_tokens, with_check_option) = split_view_check_option(tokens);
        if recursive && with_check_option != ViewCheckOption::NoCheckOption {
            return Err(self.error_here("WITH CHECK OPTION is not supported on recursive views"));
        }
        let query = self.parse_select_fragment_tokens(query_tokens)?;
        let query = if recursive {
            make_recursive_view_select(&view_node, &aliases, query)?
        } else {
            query
        };
        Ok(Node::ViewStmt(ViewStmt {
            node_tag: NodeTag::ViewStmt,
            view: Some(Box::new(view_node)),
            aliases,
            query: Some(Box::new(query)),
            replace,
            options,
            with_check_option,
        }))
    }
}
pub(super) fn is_rule_action(node: &Node) -> bool {
    matches!(
        node,
        Node::SelectStmt(_)
            | Node::InsertStmt(_)
            | Node::UpdateStmt(_)
            | Node::DeleteStmt(_)
            | Node::NotifyStmt(_)
    )
}

pub(super) fn split_view_check_option(mut tokens: Vec<Token>) -> (Vec<Token>, ViewCheckOption) {
    let len = tokens.len();
    if len >= 4
        && tokens[len - 4].kind == TokenKind::With
        && matches!(tokens[len - 3].kind, TokenKind::Cascaded | TokenKind::Local)
        && tokens[len - 2].kind == TokenKind::Check
        && tokens[len - 1].kind == TokenKind::Option
    {
        let option = if tokens[len - 3].kind == TokenKind::Local {
            ViewCheckOption::LocalCheckOption
        } else {
            ViewCheckOption::CascadedCheckOption
        };
        tokens.truncate(len - 4);
        return (tokens, option);
    }
    if len >= 3
        && tokens[len - 3].kind == TokenKind::With
        && tokens[len - 2].kind == TokenKind::Check
        && tokens[len - 1].kind == TokenKind::Option
    {
        tokens.truncate(len - 3);
        return (tokens, ViewCheckOption::CascadedCheckOption);
    }
    (tokens, ViewCheckOption::NoCheckOption)
}

pub(super) fn make_recursive_view_select(
    view: &RangeVar,
    aliases: &[Node],
    query: Node,
) -> PResult<Node> {
    let relname = view.relname.clone().ok_or_else(|| {
        ParseError::syntax_exit(0, "recursive view requires an unqualified relation name")
    })?;
    let cte = Node::CommonTableExpr(CommonTableExpr {
        node_tag: NodeTag::CommonTableExpr,
        ctename: Some(relname.clone()),
        aliascolnames: aliases.to_vec(),
        ctequery: Some(Box::new(query)),
        location: -1,
        ..CommonTableExpr::default()
    });
    let target_list = aliases
        .iter()
        .map(|alias| -> PResult<Node> {
            let Node::String(alias) = alias else {
                unreachable!("recursive view aliases are String nodes");
            };
            let alias = alias.sval.clone().ok_or_else(|| {
                ParseError::syntax_exit(0, "recursive view alias cannot be empty")
            })?;
            Ok(Node::ResTarget(ResTarget {
                node_tag: NodeTag::ResTarget,
                val: Some(Box::new(Node::ColumnRef(ColumnRef {
                    node_tag: NodeTag::ColumnRef,
                    fields: vec![make_string_node(alias)],
                    location: -1,
                }))),
                location: -1,
                ..ResTarget::default()
            }))
        })
        .collect::<PResult<NodeList>>()?;
    let mut relation = range_var_from_parts(vec![relname], 0);
    relation.location = -1;
    Ok(Node::SelectStmt(SelectStmt {
        node_tag: NodeTag::SelectStmt,
        target_list,
        from_clause: vec![Node::RangeVar(relation)],
        with_clause: Some(Box::new(WithClause {
            node_tag: NodeTag::WithClause,
            ctes: vec![cte],
            recursive: true,
            location: -1,
        })),
        ..SelectStmt::default()
    }))
}
pub(super) fn parse_statement_list_tokens_with_completion(
    mut tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<NodeList> {
    let location = tokens.last().map_or(0, Token::end_location);
    tokens.push(Token::synthetic(TokenKind::Eof, location));
    let mut parser = Parser {
        tokens,
        pos: 0,
        completion,
    };
    Ok(parser
        .parse_controlled()?
        .into_iter()
        .filter_map(|stmt| stmt.stmt.map(|node| *node))
        .collect())
}
