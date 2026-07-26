use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createschema.html
    // CREATE SCHEMA schema_name [ AUTHORIZATION role_specification ] [ schema_element [ ... ] ]
    // CREATE SCHEMA AUTHORIZATION role_specification [ schema_element [ ... ] ]
    // CREATE SCHEMA IF NOT EXISTS schema_name [ AUTHORIZATION role_specification ]
    // CREATE SCHEMA IF NOT EXISTS AUTHORIZATION role_specification
    //
    // where role_specification can be:
    //
    //     user_name
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    pub(super) fn parse_create_schema(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Schema)?;
        let if_not_exists = self.consume_if_not_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Schema);
        let schemaname = if self.at(TokenKind::Authorization) {
            None
        } else {
            self.consume_col_id()
        };
        let authrole = if self.consume(TokenKind::Authorization) {
            Some(Box::new(self.consume_role_spec().ok_or_else(|| {
                self.error_here("CREATE SCHEMA requires an authorization role")
            })?))
        } else {
            None
        };
        if schemaname.is_none() && authrole.is_none() {
            return Err(self.error_here("CREATE SCHEMA requires a name or AUTHORIZATION role"));
        }
        let mut schema_elts = Vec::new();
        self.record_completion_tokens(&[TokenKind::Create, TokenKind::Grant]);
        while self.at_any(&[TokenKind::Create, TokenKind::Grant]) {
            if if_not_exists {
                return Err(
                    self.error_here("CREATE SCHEMA IF NOT EXISTS cannot include schema elements")
                );
            }
            schema_elts.push(self.parse_schema_statement()?);
        }
        Ok(Node::CreateSchemaStmt(CreateSchemaStmt {
            node_tag: NodeTag::CreateSchemaStmt,
            schemaname,
            authrole,
            schema_elts,
            if_not_exists,
        }))
    }

    fn parse_schema_statement(&mut self) -> PResult<Node> {
        let location = self.location();
        if !self.at_any(&[TokenKind::Create, TokenKind::Grant]) {
            return Err(self.error_here("expected a CREATE SCHEMA element"));
        }
        let start = self.pos;
        let mut depth = 0usize;
        let mut atomic_depth = 0usize;
        let mut case_depth = 0usize;
        let mut end = start + 1;
        while end < self.tokens.len() {
            let kind = self.tokens[end].kind;
            match kind {
                TokenKind::Completion => break,
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::BeginP
                    if depth == 0
                        && self.tokens.get(end + 1).map(|token| token.kind)
                            == Some(TokenKind::Atomic) =>
                {
                    atomic_depth += 1;
                }
                TokenKind::Case if depth == 0 && atomic_depth > 0 => case_depth += 1,
                TokenKind::EndP if depth == 0 && atomic_depth > 0 => {
                    if case_depth > 0 {
                        case_depth -= 1;
                    } else {
                        atomic_depth -= 1;
                    }
                }
                TokenKind::Create | TokenKind::Grant if depth == 0 && atomic_depth == 0 => {
                    break;
                }
                TokenKind::Char(';') | TokenKind::Eof if depth == 0 && atomic_depth == 0 => {
                    break;
                }
                _ => {}
            }
            end += 1;
        }
        self.pos = end;
        let mut tokens = self.tokens[start..end].to_vec();
        self.append_completion_marker(&mut tokens);
        let node = parse_statement_node_tokens_with_completion(tokens, self.completion.clone())?;
        if matches!(
            node,
            Node::CreateStmt(_)
                | Node::IndexStmt(_)
                | Node::CreateDomainStmt(_)
                | Node::CreateFunctionStmt(_)
                | Node::CreateSeqStmt(_)
                | Node::CreateTrigStmt(_)
                | Node::DefineStmt(_)
                | Node::GrantStmt(_)
                | Node::ViewStmt(_)
        ) {
            Ok(node)
        } else {
            Err(ParseError::syntax_exit(
                location,
                "statement type is not allowed in CREATE SCHEMA",
            ))
        }
    }
}
