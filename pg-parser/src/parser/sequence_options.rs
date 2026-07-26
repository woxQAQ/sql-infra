use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createsequence.html
    // CREATE [ { TEMPORARY | TEMP } | UNLOGGED ] SEQUENCE [ IF NOT EXISTS ] name
    //     [ AS data_type ]
    //     [ INCREMENT [ BY ] increment ]
    //     [ MINVALUE minvalue | NO MINVALUE ] [ MAXVALUE maxvalue | NO MAXVALUE ]
    //     [ [ NO ] CYCLE ]
    //     [ START [ WITH ] start ]
    //     [ CACHE cache ]
    //     [ OWNED BY { table_name.column_name | NONE } ]
    pub(super) fn parse_create_sequence(&mut self, relpersistence: u8) -> PResult<Node> {
        self.expect(TokenKind::Sequence)?;
        let if_not_exists = self.consume_if_not_exists()?;
        let mut sequence_node = self
            .try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Sequence)
            .ok_or_else(|| self.error_here("CREATE SEQUENCE requires a name"))?;
        sequence_node.relpersistence = relpersistence;
        let sequence = Some(Box::new(sequence_node));
        let options = self.parse_sequence_options()?;
        Ok(Node::CreateSeqStmt(CreateSeqStmt {
            node_tag: NodeTag::CreateSeqStmt,
            sequence,
            options,
            if_not_exists,
            ..CreateSeqStmt::default()
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersequence.html
    // ALTER SEQUENCE [ IF EXISTS ] name
    //     [ AS data_type ]
    //     [ INCREMENT [ BY ] increment ]
    //     [ MINVALUE minvalue | NO MINVALUE ] [ MAXVALUE maxvalue | NO MAXVALUE ]
    //     [ [ NO ] CYCLE ]
    //     [ START [ WITH ] start ]
    //     [ RESTART [ [ WITH ] restart ] ]
    //     [ CACHE cache ]
    //     [ OWNED BY { table_name.column_name | NONE } ]
    // ALTER SEQUENCE [ IF EXISTS ] name SET { LOGGED | UNLOGGED }
    // ALTER SEQUENCE [ IF EXISTS ] name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SEQUENCE [ IF EXISTS ] name RENAME TO new_name
    // ALTER SEQUENCE [ IF EXISTS ] name SET SCHEMA new_schema
    pub(super) fn parse_alter_sequence(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Sequence)?;
        let missing_ok = self.consume_if_exists()?;
        let sequence = Some(Box::new(
            self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Sequence)
                .ok_or_else(|| self.error_here("ALTER SEQUENCE requires a sequence name"))?,
        ));
        let options = self.parse_sequence_options()?;
        if options.is_empty() {
            return Err(self.error_here("ALTER SEQUENCE requires at least one option"));
        }
        Ok(Node::AlterSeqStmt(AlterSeqStmt {
            node_tag: NodeTag::AlterSeqStmt,
            sequence,
            options,
            missing_ok,
            ..AlterSeqStmt::default()
        }))
    }

    pub(super) fn parse_sequence_options(&mut self) -> PResult<NodeList> {
        let mut options = Vec::new();
        while !self.at_statement_end()
            && !self.at(TokenKind::Char(')'))
            && !self.at(TokenKind::Char(','))
        {
            let location = self.location();
            let (name, arg) = match self.peek_kind() {
                TokenKind::As => {
                    self.advance();
                    self.record_completion_slot(completion::GrammarSlot::Type);
                    let type_tokens = self.take_sequence_type_tokens();
                    if type_tokens.is_empty() {
                        return Err(self.error_here("AS requires a sequence data type"));
                    }
                    let type_name = parse_simple_type_name_tokens(type_tokens)?;
                    ("as", Some(Node::TypeName(type_name)))
                }
                TokenKind::Cache => {
                    self.advance();
                    ("cache", Some(self.parse_numeric_only()?))
                }
                TokenKind::Cycle => {
                    self.advance();
                    ("cycle", Some(Node::Boolean(Boolean::new(true))))
                }
                TokenKind::No => {
                    self.advance();
                    let option = match self.peek_kind() {
                        TokenKind::Cycle => ("cycle", Some(Node::Boolean(Boolean::new(false)))),
                        TokenKind::Maxvalue => ("maxvalue", None),
                        TokenKind::Minvalue => ("minvalue", None),
                        _ => {
                            return Err(
                                self.error_here("expected CYCLE, MAXVALUE, or MINVALUE after NO")
                            );
                        }
                    };
                    self.advance();
                    option
                }
                TokenKind::Increment => {
                    self.advance();
                    self.consume(TokenKind::By);
                    ("increment", Some(self.parse_numeric_only()?))
                }
                TokenKind::Logged => {
                    self.advance();
                    ("logged", None)
                }
                TokenKind::Unlogged => {
                    self.advance();
                    ("unlogged", None)
                }
                TokenKind::Maxvalue => {
                    self.advance();
                    ("maxvalue", Some(self.parse_numeric_only()?))
                }
                TokenKind::Minvalue => {
                    self.advance();
                    ("minvalue", Some(self.parse_numeric_only()?))
                }
                TokenKind::Owned => {
                    self.advance();
                    self.expect(TokenKind::By)?;
                    self.record_completion_tokens(&[TokenKind::None]);
                    self.record_completion_slot(completion::GrammarSlot::Column);
                    let names = self.parse_name_list();
                    if names.is_empty() {
                        return Err(self.error_here("OWNED BY requires a name"));
                    }
                    (
                        "owned_by",
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: names,
                            ..AArrayExpr::default()
                        })),
                    )
                }
                TokenKind::Sequence => {
                    self.advance();
                    self.expect(TokenKind::NameP)?;
                    self.record_completion_slot(completion::GrammarSlot::Sequence);
                    let names = self.parse_name_list();
                    if names.is_empty() {
                        return Err(self.error_here("SEQUENCE NAME requires a name"));
                    }
                    (
                        "sequence_name",
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: names,
                            ..AArrayExpr::default()
                        })),
                    )
                }
                TokenKind::Start => {
                    self.advance();
                    self.consume(TokenKind::With);
                    ("start", Some(self.parse_numeric_only()?))
                }
                TokenKind::Restart => {
                    self.advance();
                    self.consume(TokenKind::With);
                    let arg = if matches!(
                        self.peek_kind(),
                        TokenKind::IConst
                            | TokenKind::FConst
                            | TokenKind::Char('+')
                            | TokenKind::Char('-')
                    ) {
                        Some(self.parse_numeric_only()?)
                    } else {
                        None
                    };
                    ("restart", arg)
                }
                _ => return Err(self.error_here("invalid sequence option")),
            };
            options.push(make_def_elem(name, arg, location));
        }
        Ok(options)
    }

    fn take_sequence_type_tokens(&mut self) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut depth = 0usize;
        loop {
            let kind = self.peek_kind();
            if kind == TokenKind::Completion {
                break;
            }
            if depth == 0
                && matches!(
                    kind,
                    TokenKind::Char(')')
                        | TokenKind::Char(',')
                        | TokenKind::Char(';')
                        | TokenKind::Eof
                )
            {
                break;
            }
            let starts_option = matches!(
                kind,
                TokenKind::As
                    | TokenKind::Cache
                    | TokenKind::Cycle
                    | TokenKind::No
                    | TokenKind::Increment
                    | TokenKind::Logged
                    | TokenKind::Maxvalue
                    | TokenKind::Minvalue
                    | TokenKind::Owned
                    | TokenKind::Sequence
                    | TokenKind::Start
                    | TokenKind::Restart
                    | TokenKind::Unlogged
            );
            if depth == 0
                && !tokens.is_empty()
                && starts_option
                && tokens.last().map(|token| token.kind) != Some(TokenKind::Char('.'))
            {
                break;
            }
            let token = self.advance().clone();
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            tokens.push(token);
        }
        tokens
    }

    pub(super) fn parse_parenthesized_sequence_options_body(&mut self) -> PResult<NodeList> {
        let options = self.parse_sequence_options()?;
        if options.is_empty() {
            return Err(self.error_here("identity sequence option list cannot be empty"));
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }

    pub(super) fn parse_numeric_only(&mut self) -> PResult<Node> {
        let negative = self.consume(TokenKind::Char('-'));
        if !negative {
            self.consume(TokenKind::Char('+'));
        }
        let token = self.advance().clone();
        let location = token.location();
        match (token.kind, token.value) {
            (TokenKind::IConst, Some(TokenValue::Integer(value))) => {
                Ok(Node::Integer(Integer::new(if negative {
                    -value
                } else {
                    value
                })))
            }
            (TokenKind::FConst, Some(TokenValue::String(value))) => {
                Ok(Node::Float(Float::new(if negative {
                    format!("-{value}")
                } else {
                    value
                })))
            }
            _ => Err(ParseError::syntax_exit(
                location,
                "expected a numeric value",
            )),
        }
    }

    pub(super) fn parse_signed_integer(&mut self) -> PResult<Node> {
        let negative = self.consume(TokenKind::Char('-'));
        if !negative {
            self.consume(TokenKind::Char('+'));
        }
        let token = self.expect(TokenKind::IConst)?;
        let Some(TokenValue::Integer(value)) = token.value else {
            return Err(ParseError::ranged(token.range, "expected an integer"));
        };
        Ok(Node::Integer(Integer::new(if negative {
            -value
        } else {
            value
        })))
    }
}
