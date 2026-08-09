//! Prepared-statement and execution utility grammar.
//!
//! `PREPARE`, `EXECUTE`, `DEALLOCATE`, `EXPLAIN`, and `CALL` share preparable
//! statement and argument fragments while producing distinct raw nodes.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-prepare.html
    // PREPARE name [ ( data_type [, ...] ) ] AS statement
    pub(super) fn parse_prepare(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Prepare)?;
        self.record_completion_slot(GrammarSlot::AnyName);
        let name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("PREPARE requires a statement name"))?,
        );
        let argtypes = if self.consume(TokenKind::Char('(')) {
            self.record_completion_slot_within_fragment(GrammarSlot::Type, &[TokenKind::Char(')')]);
            let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            let types = parse_type_node_list(tokens)?;
            self.expect(TokenKind::Char(')'))?;
            types
        } else {
            Vec::new()
        };
        self.expect(TokenKind::As)?;
        self.record_completion_tokens(&[
            TokenKind::Select,
            TokenKind::Values,
            TokenKind::Table,
            TokenKind::Char('('),
            TokenKind::With,
            TokenKind::Insert,
            TokenKind::Update,
            TokenKind::DeleteP,
            TokenKind::Merge,
        ]);
        if !matches!(
            self.peek_kind(),
            TokenKind::Select
                | TokenKind::Values
                | TokenKind::Table
                | TokenKind::Char('(')
                | TokenKind::With
                | TokenKind::Insert
                | TokenKind::Update
                | TokenKind::DeleteP
                | TokenKind::Merge
        ) {
            return Err(self.error_here("PREPARE requires a preparable DML statement"));
        }
        let query = Some(Box::new(self.parse_statement(None)?));
        Ok(node!(PrepareStmt {
            name,
            argtypes,
            query,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-execute.html
    // EXECUTE name [ ( parameter [, ...] ) ]
    pub(super) fn parse_execute(&mut self) -> PResult<Node> {
        let stmt = self.parse_execute_core()?;
        self.expect_statement_end()?;
        Ok(Node::ExecuteStmt(stmt))
    }

    pub(super) fn parse_execute_core(&mut self) -> PResult<ExecuteStmt> {
        self.expect(TokenKind::Execute)?;
        self.record_completion_slot(GrammarSlot::AnyName);
        let name = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("EXECUTE requires a statement name"))?,
        );
        let params = if self.consume(TokenKind::Char('(')) {
            let params = self.parse_expr_list_strict_until(&[TokenKind::Char(')')])?;
            if params.is_empty() {
                return Err(self.error_here("EXECUTE parameter list cannot be empty"));
            }
            self.expect(TokenKind::Char(')'))?;
            params
        } else {
            Vec::new()
        };
        Ok(ExecuteStmt { name, params })
    }

    pub(super) fn parse_optional_with_data(&mut self) -> PResult<bool> {
        if !self.consume(TokenKind::With) {
            return Ok(false);
        }
        self.record_completion_tokens(&[TokenKind::No, TokenKind::DataP]);
        let skip_data = self.consume(TokenKind::No);
        self.expect(TokenKind::DataP)?;
        Ok(skip_data)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-deallocate.html
    // DEALLOCATE [ PREPARE ] { name | ALL }
    pub(super) fn parse_deallocate(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Deallocate)?;
        self.consume(TokenKind::Prepare);
        let isall = self.consume(TokenKind::All);
        let (name, location) = if isall {
            (None, -1)
        } else {
            self.record_completion_slot(GrammarSlot::AnyName);
            let location = self.location() as ParseLoc;
            (
                Some(self.consume_col_id().ok_or_else(|| {
                    self.error_here("DEALLOCATE requires a statement name or ALL")
                })?),
                location,
            )
        };
        self.expect_statement_end()?;
        Ok(node!(DeallocateStmt {
            name,
            isall,
            location,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-explain.html
    // EXPLAIN [ ( option [, ...] ) ] statement
    //
    // where option can be one of:
    //
    //     ANALYZE [ boolean ]
    //     VERBOSE [ boolean ]
    //     COSTS [ boolean ]
    //     SETTINGS [ boolean ]
    //     GENERIC_PLAN [ boolean ]
    //     BUFFERS [ boolean ]
    //     SERIALIZE [ { NONE | TEXT | BINARY } ]
    //     WAL [ boolean ]
    //     TIMING [ boolean ]
    //     SUMMARY [ boolean ]
    //     MEMORY [ boolean ]
    //     FORMAT { TEXT | XML | JSON | YAML }
    pub(super) fn parse_explain(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Explain)?;
        self.record_completion_tokens(&[
            TokenKind::Char('('),
            TokenKind::Analyze,
            TokenKind::Analyse,
            TokenKind::Verbose,
            TokenKind::Select,
            TokenKind::Values,
            TokenKind::Table,
            TokenKind::Char('('),
            TokenKind::With,
            TokenKind::Insert,
            TokenKind::Update,
            TokenKind::DeleteP,
            TokenKind::Merge,
            TokenKind::Declare,
            TokenKind::Create,
            TokenKind::Refresh,
            TokenKind::Execute,
        ]);
        if self.at_completion() {
            return Err(self.error_here("completion point after EXPLAIN"));
        }
        let parenthesized = self.at(TokenKind::Char('('));
        let mut options = if parenthesized {
            self.parse_parenthesized_utility_option_list()?
        } else {
            Vec::new()
        };
        if !parenthesized && matches!(self.peek_kind(), TokenKind::Analyze | TokenKind::Analyse) {
            let token = self.advance().clone();
            options.push(make_def_elem("analyze", None, token.location()));
            if self.at(TokenKind::Verbose) {
                let token = self.advance().clone();
                options.push(make_def_elem("verbose", None, token.location()));
            }
        } else if !parenthesized && self.at(TokenKind::Verbose) {
            let token = self.advance().clone();
            options.push(make_def_elem("verbose", None, token.location()));
        }
        if self.at_statement_end() {
            return Err(self.error_here("EXPLAIN requires a statement"));
        }
        let query = self.parse_statement(None)?;
        if !matches!(
            query,
            Node::SelectStmt(_)
                | Node::InsertStmt(_)
                | Node::UpdateStmt(_)
                | Node::DeleteStmt(_)
                | Node::MergeStmt(_)
                | Node::DeclareCursorStmt(_)
                | Node::CreateTableAsStmt(_)
                | Node::RefreshMatViewStmt(_)
                | Node::ExecuteStmt(_)
        ) {
            return Err(self.error_here("statement is not explainable"));
        }
        Ok(node!(ExplainStmt {
            query: Some(Box::new(query)),
            options,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-call.html
    // CALL name ( [ argument ] [, ...] )
    pub(super) fn parse_call(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Call)?;
        self.record_completion_slot(GrammarSlot::Procedure);
        if self.at_completion() {
            return Err(self.error_here("completion point at CALL routine name"));
        }
        let mut tokens = self.take_until_top_level(STATEMENT_END_TOKENS);
        if self.at_completion()
            && matches!(
                parse_expression_tokens(tokens.clone()),
                Ok(node!(FuncCall {
                    agg_filter: None,
                    over: None,
                    agg_within_group: false,
                    ignore_nulls: 0,
                    ..
                }))
            )
        {
            self.record_completion_tokens(&[TokenKind::Char(';')]);
            return Err(self.error_here("completion point after CALL"));
        }
        self.append_completion_marker(&mut tokens);
        let funccall =
            match parse_expression_tokens_with_completion(tokens, self.completion.clone())? {
                Node::FuncCall(call)
                    if call.agg_filter.is_none()
                        && call.over.is_none()
                        && !call.agg_within_group
                        && call.ignore_nulls == 0 =>
                {
                    Some(Box::new(call))
                }
                _ => return Err(self.error_here("CALL requires a function application")),
            };
        Ok(node!(CallStmt {
            funccall,
            ..CallStmt::default()
        }))
    }
}
