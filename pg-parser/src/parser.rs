use crate::TokenKind;
use crate::ast::*;
use crate::lexer::{LexError, Token, TokenValue, lex};

type PResult<T> = Result<T, ParseError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub message: std::string::String,
    pub location: usize,
}

impl ParseError {
    fn new(location: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

impl From<LexError> for ParseError {
    fn from(value: LexError) -> Self {
        Self {
            message: value.message,
            location: value.location,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.location)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(sql: &str) -> PResult<Vec<RawStmt>> {
    Parser::new(sql)?.parse()
}

pub fn parse_one(sql: &str) -> PResult<RawStmt> {
    let mut stmts = parse(sql)?;
    if stmts.len() != 1 {
        return Err(ParseError::new(
            stmts.get(1).map_or(0, |stmt| stmt.stmt_location as usize),
            format!("expected one statement, found {}", stmts.len()),
        ));
    }
    Ok(stmts.remove(0))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WithTarget {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(sql: &str) -> PResult<Self> {
        Ok(Self {
            tokens: lex(sql)?,
            pos: 0,
        })
    }

    pub fn parse(&mut self) -> PResult<Vec<RawStmt>> {
        let mut stmts = Vec::new();
        while !self.at(TokenKind::Eof) {
            while self.consume(TokenKind::Char(';')) {}
            if self.at(TokenKind::Eof) {
                break;
            }

            let start = self.location();
            let stmt = self.parse_statement(None)?;
            let end = self.location();
            self.consume(TokenKind::Char(';'));
            stmts.push(RawStmt {
                node_tag: NodeTag::RawStmt,
                stmt: Some(Box::new(stmt)),
                stmt_location: start as ParseLoc,
                stmt_len: end.saturating_sub(start) as ParseLoc,
            });
        }
        Ok(stmts)
    }

    fn parse_statement(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        match self.peek_kind() {
            TokenKind::With => self.parse_with_statement(),
            TokenKind::Select | TokenKind::Values | TokenKind::Table => {
                Ok(Node::SelectStmt(self.parse_select(with_clause)?))
            }
            TokenKind::Insert => self.parse_insert(with_clause),
            TokenKind::Update => self.parse_update(with_clause),
            TokenKind::DeleteP => self.parse_delete(with_clause),
            TokenKind::Merge => self.parse_merge(with_clause),
            TokenKind::Create => self.parse_create(),
            TokenKind::Alter => self.parse_alter(),
            TokenKind::Drop => self.parse_drop(),
            TokenKind::Set => self.parse_set_or_constraints(),
            TokenKind::Reset => self.parse_variable_reset(),
            TokenKind::Show => self.parse_variable_show(),
            TokenKind::BeginP
            | TokenKind::Start
            | TokenKind::Commit
            | TokenKind::EndP
            | TokenKind::Rollback
            | TokenKind::AbortP
            | TokenKind::Savepoint
            | TokenKind::Release => self.parse_transaction(),
            TokenKind::Prepare if self.peek_kind_n(1) == TokenKind::Transaction => {
                self.parse_transaction()
            }
            TokenKind::Prepare => self.parse_prepare(),
            TokenKind::Execute => self.parse_execute(),
            TokenKind::Deallocate => self.parse_deallocate(),
            TokenKind::Declare => self.parse_declare_cursor(),
            TokenKind::Close => self.parse_close(),
            TokenKind::Fetch | TokenKind::Move => self.parse_fetch_or_move(),
            TokenKind::Copy => self.parse_copy(),
            TokenKind::Vacuum | TokenKind::Analyze | TokenKind::Analyse => self.parse_vacuum(),
            TokenKind::Explain => self.parse_explain(),
            TokenKind::Call => self.parse_call(),
            TokenKind::Checkpoint => self.parse_checkpoint(),
            TokenKind::Discard => self.parse_discard(),
            TokenKind::LockP => self.parse_lock(),
            TokenKind::Listen => self.parse_listen(),
            TokenKind::Unlisten => self.parse_unlisten(),
            TokenKind::Notify => self.parse_notify(),
            TokenKind::Load => self.parse_load(),
            TokenKind::Refresh => self.parse_refresh(),
            TokenKind::Reindex => self.parse_reindex(),
            TokenKind::Cluster | TokenKind::Repack => self.parse_repack(),
            TokenKind::Reassign => self.parse_reassign_owned(),
            TokenKind::Truncate => self.parse_truncate(),
            TokenKind::Comment => self.parse_comment(),
            TokenKind::Security => self.parse_security_label(),
            TokenKind::Grant => self.parse_grant(true),
            TokenKind::Revoke => self.parse_grant(false),
            TokenKind::ImportP => self.parse_import_foreign_schema(),
            TokenKind::Do => self.parse_do(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Wait => self.parse_wait(),
            other => Err(self.error_here(format!("unexpected token {:?}", other))),
        }
    }

    fn parse_with_statement(&mut self) -> PResult<Node> {
        let with = self.parse_with_clause()?;
        let target = match self.peek_kind() {
            TokenKind::Select | TokenKind::Values | TokenKind::Table => WithTarget::Select,
            TokenKind::Insert => WithTarget::Insert,
            TokenKind::Update => WithTarget::Update,
            TokenKind::DeleteP => WithTarget::Delete,
            TokenKind::Merge => WithTarget::Merge,
            _ => return self.parse_statement(Some(with)),
        };

        match target {
            WithTarget::Select => Ok(Node::SelectStmt(self.parse_select(Some(with))?)),
            WithTarget::Insert => self.parse_insert(Some(with)),
            WithTarget::Update => self.parse_update(Some(with)),
            WithTarget::Delete => self.parse_delete(Some(with)),
            WithTarget::Merge => self.parse_merge(Some(with)),
        }
    }

    fn parse_with_clause(&mut self) -> PResult<WithClause> {
        let location = self.expect(TokenKind::With)?.location;
        let recursive = self.consume(TokenKind::Recursive);
        let mut ctes = Vec::new();

        loop {
            let cte_location = self.location();
            let Some(name) = self.consume_name() else {
                break;
            };
            if self.consume(TokenKind::Char('(')) {
                self.skip_until_top_level(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
            }
            self.consume(TokenKind::As);
            if matches!(self.peek_kind(), TokenKind::Materialized | TokenKind::Not) {
                self.advance();
                if self.previous_kind() == TokenKind::Not {
                    self.consume(TokenKind::Materialized);
                }
            }
            let ctequery = if self.consume(TokenKind::Char('(')) {
                let inner = self.take_until_top_level(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                tokens_to_statement_node(inner.clone())
                    .or_else(|| tokens_to_node(inner))
                    .map(Box::new)
            } else {
                None
            };
            ctes.push(Node::CommonTableExpr(CommonTableExpr {
                node_tag: NodeTag::CommonTableExpr,
                ctename: Some(name),
                ctequery,
                location: cte_location as ParseLoc,
                ..CommonTableExpr::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }

        Ok(WithClause {
            node_tag: NodeTag::WithClause,
            ctes,
            recursive,
            location: location as ParseLoc,
        })
    }

    fn parse_select(&mut self, with_clause: Option<WithClause>) -> PResult<SelectStmt> {
        let location = self.location();
        let mut stmt = SelectStmt {
            node_tag: NodeTag::SelectStmt,
            with_clause: with_clause.map(Box::new),
            ..SelectStmt::default()
        };

        match self.peek_kind() {
            TokenKind::Values => {
                self.advance();
                stmt.values_lists = self.parse_values_lists();
            }
            TokenKind::Table => {
                self.advance();
                if let Some(range) = self.try_parse_range_var() {
                    stmt.from_clause.push(Node::RangeVar(range));
                }
            }
            _ => {
                self.expect(TokenKind::Select)?;
                if self.consume(TokenKind::All) {
                    stmt.distinct_clause.clear();
                } else if self.consume(TokenKind::Distinct) {
                    stmt.distinct_clause
                        .push(Node::String(String::new("distinct")));
                    if self.consume(TokenKind::On) && self.consume(TokenKind::Char('(')) {
                        stmt.distinct_clause
                            .extend(self.parse_expr_list_until(&[TokenKind::Char(')')]));
                        self.consume(TokenKind::Char(')'));
                    }
                }
                stmt.target_list = self.parse_res_target_list_until(&[
                    TokenKind::From,
                    TokenKind::Where,
                    TokenKind::GroupP,
                    TokenKind::Having,
                    TokenKind::Window,
                    TokenKind::Order,
                    TokenKind::Limit,
                    TokenKind::Offset,
                    TokenKind::Fetch,
                    TokenKind::For,
                    TokenKind::Union,
                    TokenKind::Intersect,
                    TokenKind::Except,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
            }
        }

        if self.consume(TokenKind::From) {
            stmt.from_clause = self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::GroupP,
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Where) {
            stmt.where_clause = self.parse_expr_box_until(&[
                TokenKind::GroupP,
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::GroupP) {
            self.consume(TokenKind::By);
            if self.consume(TokenKind::All) {
                stmt.group_by_all = true;
            } else if self.consume(TokenKind::Distinct) {
                stmt.group_distinct = true;
            }
            stmt.group_clause = self.parse_expr_list_until(&[
                TokenKind::Having,
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Having) {
            stmt.having_clause = self.parse_expr_box_until(&[
                TokenKind::Window,
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Window) {
            stmt.window_clause = self.parse_window_clause_until(&[
                TokenKind::Order,
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Order) {
            self.consume(TokenKind::By);
            stmt.sort_clause = self.parse_sort_list_until(&[
                TokenKind::Limit,
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Limit) {
            stmt.limit_count = self.parse_expr_box_until(&[
                TokenKind::Offset,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if self.consume(TokenKind::Offset) {
            stmt.limit_offset = self.parse_expr_box_until(&[
                TokenKind::Limit,
                TokenKind::Fetch,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            self.consume(TokenKind::Row);
            self.consume(TokenKind::Rows);
        }
        if self.consume(TokenKind::Fetch) {
            let _ = self.consume(TokenKind::FirstP) || self.consume(TokenKind::Next);
            stmt.limit_count = self.parse_expr_box_until(&[
                TokenKind::Row,
                TokenKind::Rows,
                TokenKind::Only,
                TokenKind::With,
                TokenKind::For,
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            let _ = self.consume(TokenKind::Row) || self.consume(TokenKind::Rows);
            if self.consume(TokenKind::With) {
                self.consume(TokenKind::Ties);
                stmt.limit_option = LimitOption::WithTies;
            } else {
                self.consume(TokenKind::Only);
            }
        }
        if self.at(TokenKind::For) {
            stmt.locking_clause = self.parse_locking_clause_until(&[
                TokenKind::Union,
                TokenKind::Intersect,
                TokenKind::Except,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        if matches!(
            self.peek_kind(),
            TokenKind::Union | TokenKind::Intersect | TokenKind::Except
        ) {
            let op_token = self.advance().kind;
            stmt.op = match op_token {
                TokenKind::Union => SetOperation::Union,
                TokenKind::Intersect => SetOperation::Intersect,
                TokenKind::Except => SetOperation::Except,
                _ => SetOperation::None,
            };
            stmt.all = self.consume(TokenKind::All);
            let larg = stmt.clone();
            let rarg = self.parse_select(None)?;
            stmt.larg = Some(Box::new(larg));
            stmt.rarg = Some(Box::new(rarg));
        }

        if stmt.target_list.is_empty()
            && stmt.from_clause.is_empty()
            && stmt.values_lists.is_empty()
            && stmt.op == SetOperation::None
        {
            stmt.target_list.push(Node::ResTarget(ResTarget {
                node_tag: NodeTag::ResTarget,
                val: Some(Box::new(make_string_node("select"))),
                location: location as ParseLoc,
                ..ResTarget::default()
            }));
        }

        Ok(stmt)
    }

    fn parse_insert(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Insert)?;
        self.consume(TokenKind::Into);
        let relation = self.try_parse_range_var().map(Box::new);
        let mut cols = Vec::new();
        if self.consume(TokenKind::Char('(')) {
            cols = self.parse_insert_column_list();
            self.consume(TokenKind::Char(')'));
        }
        if self.consume(TokenKind::Overriding) {
            self.skip_until_top_level(&[
                TokenKind::Select,
                TokenKind::Values,
                TokenKind::With,
                TokenKind::Table,
                TokenKind::Default,
                TokenKind::On,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        let select_stmt = if self.consume(TokenKind::Default) {
            self.consume(TokenKind::Values);
            None
        } else if matches!(
            self.peek_kind(),
            TokenKind::Select | TokenKind::Values | TokenKind::With | TokenKind::Table
        ) {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            None
        };
        let mut on_conflict_clause = None;
        if self.consume(TokenKind::On) && self.consume(TokenKind::Conflict) {
            let location = self.previous_location();
            let action = if self.consume(TokenKind::Do) {
                if self.consume(TokenKind::Nothing) {
                    OnConflictAction::Nothing
                } else if self.consume(TokenKind::Update) {
                    self.skip_until_top_level(&[
                        TokenKind::Returning,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                    OnConflictAction::Update
                } else {
                    OnConflictAction::None
                }
            } else {
                OnConflictAction::None
            };
            on_conflict_clause = Some(Box::new(OnConflictClause {
                node_tag: NodeTag::OnConflictClause,
                action,
                location: location as ParseLoc,
                ..OnConflictClause::default()
            }));
        }
        let returning_clause = self.parse_returning_clause();
        Ok(Node::InsertStmt(InsertStmt {
            node_tag: NodeTag::InsertStmt,
            relation,
            cols,
            select_stmt,
            on_conflict_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            ..InsertStmt::default()
        }))
    }

    fn parse_update(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Update)?;
        let relation = self.try_parse_range_var().map(Box::new);
        self.consume(TokenKind::Set);
        let target_list = self.parse_res_target_list_until(&[
            TokenKind::From,
            TokenKind::Where,
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let from_clause = if self.consume(TokenKind::From) {
            self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            Vec::new()
        };
        let where_clause = if self.consume(TokenKind::Where) {
            self.parse_expr_box_until(&[TokenKind::Returning, TokenKind::Char(';'), TokenKind::Eof])
        } else {
            None
        };
        let returning_clause = self.parse_returning_clause();
        Ok(Node::UpdateStmt(UpdateStmt {
            node_tag: NodeTag::UpdateStmt,
            relation,
            target_list,
            from_clause,
            where_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            ..UpdateStmt::default()
        }))
    }

    fn parse_delete(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::DeleteP)?;
        self.consume(TokenKind::From);
        let relation = self.try_parse_range_var().map(Box::new);
        let using_clause = if self.consume(TokenKind::Using) {
            self.parse_from_clause_until(&[
                TokenKind::Where,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            Vec::new()
        };
        let where_clause = if self.consume(TokenKind::Where) {
            self.parse_expr_box_until(&[TokenKind::Returning, TokenKind::Char(';'), TokenKind::Eof])
        } else {
            None
        };
        let returning_clause = self.parse_returning_clause();
        Ok(Node::DeleteStmt(DeleteStmt {
            node_tag: NodeTag::DeleteStmt,
            relation,
            using_clause,
            where_clause,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            ..DeleteStmt::default()
        }))
    }

    fn parse_merge(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Merge)?;
        self.consume(TokenKind::Into);
        let relation = self.try_parse_range_var().map(Box::new);
        let source_relation = if self.consume(TokenKind::Using) {
            if let Some(range) = self.try_parse_range_var() {
                Some(Box::new(Node::RangeVar(range)))
            } else {
                self.parse_expr_box_until(&[
                    TokenKind::On,
                    TokenKind::When,
                    TokenKind::Returning,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])
            }
        } else {
            None
        };
        let join_condition = if self.consume(TokenKind::On) {
            self.parse_expr_box_until(&[
                TokenKind::When,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            None
        };
        while self.consume(TokenKind::When) {
            self.skip_until_top_level(&[
                TokenKind::When,
                TokenKind::Returning,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        let returning_clause = self.parse_returning_clause();
        Ok(Node::MergeStmt(MergeStmt {
            node_tag: NodeTag::MergeStmt,
            relation,
            source_relation,
            join_condition,
            returning_clause,
            with_clause: with_clause.map(Box::new),
            ..MergeStmt::default()
        }))
    }

    fn parse_create(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Create)?;
        let replace = self.consume(TokenKind::Or) && self.consume(TokenKind::Replace);
        while matches!(
            self.peek_kind(),
            TokenKind::Temp
                | TokenKind::Temporary
                | TokenKind::Unlogged
                | TokenKind::Global
                | TokenKind::Local
        ) {
            self.advance();
        }
        let node = match self.peek_kind() {
            TokenKind::Table => self.parse_create_table(false)?,
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Table => {
                self.advance();
                self.parse_create_table(true)?
            }
            TokenKind::Unique | TokenKind::Index => self.parse_index(false)?,
            TokenKind::Schema => self.parse_create_schema()?,
            TokenKind::Database => self.parse_createdb()?,
            TokenKind::Recursive if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.parse_view(replace)?
            }
            TokenKind::View => self.parse_view(replace)?,
            TokenKind::Materialized if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.parse_create_table_as(ObjectType::Matview)?
            }
            TokenKind::Extension => self.parse_create_extension(),
            TokenKind::Function | TokenKind::Procedure => self.parse_create_function(replace)?,
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_create_user_mapping()
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_create_role()?,
            TokenKind::Sequence => self.parse_create_sequence()?,
            TokenKind::DomainP => self.parse_create_domain()?,
            TokenKind::TypeP => self.parse_create_type()?,
            TokenKind::Publication => self.parse_create_publication(),
            TokenKind::Subscription => self.parse_create_subscription(),
            TokenKind::Policy => self.parse_create_policy(),
            TokenKind::Trigger => self.parse_create_trigger(replace),
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.parse_create_event_trigger()
            }
            TokenKind::Language => self.parse_create_language(replace),
            TokenKind::Procedural if self.peek_kind_n(1) == TokenKind::Language => {
                self.advance();
                self.parse_create_language(replace)
            }
            TokenKind::Server => self.parse_create_server(),
            TokenKind::Tablespace => self.parse_create_tablespace(),
            TokenKind::Access if self.peek_kind_n(1) == TokenKind::Method => {
                self.advance();
                self.parse_create_am()
            }
            TokenKind::Cast => self.parse_create_cast(),
            TokenKind::ConversionP => self.parse_create_conversion(),
            TokenKind::Transform => self.parse_create_transform(replace),
            TokenKind::Statistics => self.parse_create_stats(),
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                self.advance();
                self.parse_create_op_class()
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.parse_create_op_family()
            }
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.parse_create_prop_graph()
            }
            TokenKind::Graph => self.parse_create_prop_graph(),
            TokenKind::Rule => self.parse_rule(replace)?,
            TokenKind::Assertion => self.parse_create_assertion(),
            TokenKind::Aggregate => self.parse_define(ObjectType::Aggregate, replace),
            TokenKind::Operator => self.parse_define(ObjectType::Operator, replace),
            TokenKind::Collation => self.parse_define(ObjectType::Collation, replace),
            TokenKind::TextP if self.peek_kind_n(1) == TokenKind::Search => {
                self.parse_define_text_search()
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.consume(TokenKind::DataP);
                self.consume(TokenKind::Wrapper);
                self.parse_create_fdw()
            }
            other => return Err(self.error_here(format!("unsupported CREATE form {:?}", other))),
        };
        Ok(node)
    }

    fn parse_create_assertion(&mut self) -> Node {
        self.expect(TokenKind::Assertion).ok();
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Check,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_rest();
        Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind: ObjectType::Default,
            defnames,
            ..DefineStmt::default()
        })
    }

    fn parse_define(&mut self, kind: ObjectType, replace: bool) -> Node {
        self.advance();
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::As,
            TokenKind::With,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_rest();
        Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind,
            defnames,
            replace,
            ..DefineStmt::default()
        })
    }

    fn parse_define_text_search(&mut self) -> Node {
        self.expect(TokenKind::TextP).ok();
        self.expect(TokenKind::Search).ok();
        let kind = match self.advance().kind {
            TokenKind::Parser => ObjectType::Tsparser,
            TokenKind::Dictionary => ObjectType::Tsdictionary,
            TokenKind::Template => ObjectType::Tstemplate,
            TokenKind::Configuration => ObjectType::Tsconfiguration,
            _ => ObjectType::Default,
        };
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_rest();
        Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind,
            defnames,
            ..DefineStmt::default()
        })
    }

    fn parse_create_fdw(&mut self) -> Node {
        let fdwname = self.consume_name();
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::CreateFdwStmt(CreateFdwStmt {
            node_tag: NodeTag::CreateFdwStmt,
            fdwname,
            options,
            ..CreateFdwStmt::default()
        })
    }

    fn parse_create_cast(&mut self) -> Node {
        self.expect(TokenKind::Cast).ok();
        self.consume(TokenKind::Char('('));
        let sourcetype = self
            .parse_type_name_until(&[TokenKind::As, TokenKind::Char(')'), TokenKind::Eof])
            .map(Box::new);
        self.consume(TokenKind::As);
        let targettype = self
            .parse_type_name_until(&[TokenKind::Char(')'), TokenKind::Eof])
            .map(Box::new);
        self.consume(TokenKind::Char(')'));

        let mut func = None;
        let mut inout = false;
        if self.consume(TokenKind::With) {
            if self.consume(TokenKind::Function) {
                func = self
                    .parse_object_with_args_until(&[
                        TokenKind::As,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])
                    .map(Box::new);
            } else if self.consume(TokenKind::Inout) {
                inout = true;
            }
        } else {
            self.consume(TokenKind::Without);
            self.consume(TokenKind::Function);
        }
        let context = if self.consume(TokenKind::As) {
            if self.consume(TokenKind::ImplicitP) {
                CoercionContext::Implicit
            } else if self.consume(TokenKind::Assignment) {
                CoercionContext::Assignment
            } else {
                CoercionContext::Explicit
            }
        } else {
            CoercionContext::Explicit
        };
        self.skip_rest();
        Node::CreateCastStmt(CreateCastStmt {
            node_tag: NodeTag::CreateCastStmt,
            sourcetype,
            targettype,
            func,
            context,
            inout,
        })
    }

    fn parse_create_conversion(&mut self) -> Node {
        self.expect(TokenKind::ConversionP).ok();
        let def = self.consume(TokenKind::Default);
        let conversion_name = self.parse_name_list_until_keywords(&[
            TokenKind::For,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.consume(TokenKind::For);
        let for_encoding_name = self.consume_string_like();
        self.consume(TokenKind::To);
        let to_encoding_name = self.consume_string_like();
        self.consume(TokenKind::From);
        let func_name =
            self.parse_name_list_until_keywords(&[TokenKind::Char(';'), TokenKind::Eof]);
        self.skip_rest();
        Node::CreateConversionStmt(CreateConversionStmt {
            node_tag: NodeTag::CreateConversionStmt,
            conversion_name,
            for_encoding_name,
            to_encoding_name,
            func_name,
            def,
        })
    }

    fn parse_create_transform(&mut self, replace: bool) -> Node {
        self.expect(TokenKind::Transform).ok();
        self.consume(TokenKind::For);
        let type_name = self
            .parse_type_name_until(&[TokenKind::Language, TokenKind::Char(';'), TokenKind::Eof])
            .map(Box::new);
        self.consume(TokenKind::Language);
        let lang = self.consume_name();
        let mut fromsql = None;
        let mut tosql = None;
        if self.consume(TokenKind::Char('(')) {
            while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
                let is_from = self.consume(TokenKind::From);
                let is_to = if !is_from {
                    self.consume(TokenKind::To)
                } else {
                    false
                };
                self.consume(TokenKind::SqlP);
                self.consume(TokenKind::With);
                self.consume(TokenKind::Function);
                let func = self
                    .parse_object_with_args_until(&[
                        TokenKind::Char(','),
                        TokenKind::Char(')'),
                        TokenKind::Eof,
                    ])
                    .map(Box::new);
                if is_from {
                    fromsql = func;
                } else if is_to {
                    tosql = func;
                }
                self.consume(TokenKind::Char(','));
            }
            self.consume(TokenKind::Char(')'));
        }
        self.skip_rest();
        Node::CreateTransformStmt(CreateTransformStmt {
            node_tag: NodeTag::CreateTransformStmt,
            replace,
            type_name,
            lang,
            fromsql,
            tosql,
        })
    }

    fn parse_create_stats(&mut self) -> Node {
        self.expect(TokenKind::Statistics).ok();
        let if_not_exists = self.consume_if_not_exists();
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::On,
            TokenKind::From,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let stat_types = if self.consume(TokenKind::Char('(')) {
            let names = self.parse_expr_list_until(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            names
        } else {
            Vec::new()
        };
        self.consume(TokenKind::On);
        let exprs =
            self.parse_expr_list_until(&[TokenKind::From, TokenKind::Char(';'), TokenKind::Eof]);
        let relations = if self.consume(TokenKind::From) {
            self.parse_from_clause_until(&[TokenKind::Char(';'), TokenKind::Eof])
        } else {
            Vec::new()
        };
        self.skip_rest();
        Node::CreateStatsStmt(CreateStatsStmt {
            node_tag: NodeTag::CreateStatsStmt,
            defnames,
            stat_types,
            exprs,
            relations,
            if_not_exists,
            ..CreateStatsStmt::default()
        })
    }

    fn parse_create_op_class(&mut self) -> Node {
        self.expect(TokenKind::Class).ok();
        let opclassname = self.parse_name_list_until_keywords(&[
            TokenKind::Default,
            TokenKind::For,
            TokenKind::Using,
            TokenKind::As,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let is_default = self.consume(TokenKind::Default);
        self.consume(TokenKind::For);
        self.consume(TokenKind::TypeP);
        let datatype = self
            .parse_type_name_until(&[TokenKind::Using, TokenKind::As, TokenKind::Eof])
            .map(Box::new);
        self.consume(TokenKind::Using);
        let amname = self.consume_name();
        let opfamilyname = if self.consume(TokenKind::Family) {
            self.parse_name_list_until_keywords(&[
                TokenKind::As,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            Vec::new()
        };
        let items = if self.consume(TokenKind::As) {
            self.parse_opclass_item_list(&[TokenKind::Char(';'), TokenKind::Eof])
        } else {
            Vec::new()
        };
        self.skip_rest();
        Node::CreateOpClassStmt(CreateOpClassStmt {
            node_tag: NodeTag::CreateOpClassStmt,
            opclassname,
            opfamilyname,
            amname,
            datatype,
            items,
            is_default,
        })
    }

    fn parse_create_op_family(&mut self) -> Node {
        self.expect(TokenKind::Family).ok();
        let opfamilyname = self.parse_name_list_until_keywords(&[
            TokenKind::Using,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.consume(TokenKind::Using);
        let amname = self.consume_name();
        self.skip_rest();
        Node::CreateOpFamilyStmt(CreateOpFamilyStmt {
            node_tag: NodeTag::CreateOpFamilyStmt,
            opfamilyname,
            amname,
        })
    }

    fn parse_create_prop_graph(&mut self) -> Node {
        self.expect(TokenKind::Graph).ok();
        let pgname = self.try_parse_qualified_range_var().map(Box::new);
        let mut vertex_tables = Vec::new();
        let mut edge_tables = Vec::new();
        while !self.at_statement_end() {
            if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
                self.advance();
                self.consume(TokenKind::Tables);
                vertex_tables.extend(self.parse_prop_graph_vertex_list());
            } else if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
                self.advance();
                self.consume(TokenKind::Tables);
                edge_tables.extend(self.parse_prop_graph_edge_list());
            } else {
                self.advance();
            }
        }
        Node::CreatePropGraphStmt(CreatePropGraphStmt {
            node_tag: NodeTag::CreatePropGraphStmt,
            pgname,
            vertex_tables,
            edge_tables,
        })
    }

    fn parse_rule(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::Rule)?;
        let rulename = self.consume_name();
        self.skip_until_top_level(&[TokenKind::On, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::On);
        let event = match self.advance().kind {
            TokenKind::Select => CmdType::Select,
            TokenKind::Update => CmdType::Update,
            TokenKind::Insert => CmdType::Insert,
            TokenKind::DeleteP => CmdType::Delete,
            _ => CmdType::Unknown,
        };
        self.skip_until_top_level(&[TokenKind::To, TokenKind::Char(';'), TokenKind::Eof]);
        let relation = if self.consume(TokenKind::To) {
            self.try_parse_range_var().map(Box::new)
        } else {
            None
        };
        let where_clause = if self.consume(TokenKind::Where) {
            self.parse_expr_box_until(&[TokenKind::Do, TokenKind::Char(';'), TokenKind::Eof])
        } else {
            None
        };
        self.skip_until_top_level(&[TokenKind::Do, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Do);
        let instead = self.consume(TokenKind::Instead);
        self.consume(TokenKind::Also);
        let actions = if self.consume(TokenKind::Nothing) {
            Vec::new()
        } else if self.consume(TokenKind::Char('(')) {
            let tokens = self.take_until_top_level(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            tokens_to_statement_list(tokens)
        } else if !self.at_statement_end() {
            vec![self.parse_statement(None)?]
        } else {
            Vec::new()
        };
        self.skip_rest();
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

    fn parse_create_table(&mut self, foreign: bool) -> PResult<Node> {
        self.expect(TokenKind::Table)?;
        let if_not_exists = self.consume_if_not_exists();
        let relation = self.try_parse_qualified_range_var().map(Box::new);
        if !foreign && self.consume(TokenKind::As) {
            let query = Some(Box::new(self.parse_statement(None)?));
            return Ok(Node::CreateTableAsStmt(CreateTableAsStmt {
                node_tag: NodeTag::CreateTableAsStmt,
                query,
                into: Some(Box::new(IntoClause {
                    node_tag: NodeTag::IntoClause,
                    rel: relation,
                    ..IntoClause::default()
                })),
                objtype: ObjectType::Table,
                if_not_exists,
                ..CreateTableAsStmt::default()
            }));
        }
        let table_elts = if self.consume(TokenKind::Char('(')) {
            self.parse_column_defs()
        } else {
            Vec::new()
        };
        self.skip_rest();
        let create = CreateStmt {
            node_tag: NodeTag::CreateStmt,
            relation,
            table_elts,
            if_not_exists,
            ..CreateStmt::default()
        };
        if foreign {
            Ok(Node::CreateForeignTableStmt(CreateForeignTableStmt {
                base: create,
                ..CreateForeignTableStmt::default()
            }))
        } else {
            Ok(Node::CreateStmt(create))
        }
    }

    fn parse_create_schema(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Schema)?;
        let if_not_exists = self.consume_if_not_exists();
        let schemaname = self.consume_name();
        self.skip_rest();
        Ok(Node::CreateSchemaStmt(CreateSchemaStmt {
            node_tag: NodeTag::CreateSchemaStmt,
            schemaname,
            if_not_exists,
            ..CreateSchemaStmt::default()
        }))
    }

    fn parse_createdb(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        let dbname = self.consume_name();
        self.skip_rest();
        Ok(Node::CreatedbStmt(CreatedbStmt {
            node_tag: NodeTag::CreatedbStmt,
            dbname,
            ..CreatedbStmt::default()
        }))
    }

    fn parse_view(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::View)?;
        let view = self.try_parse_qualified_range_var().map(Box::new);
        let query = if self.consume(TokenKind::As) {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            self.skip_rest();
            None
        };
        Ok(Node::ViewStmt(ViewStmt {
            node_tag: NodeTag::ViewStmt,
            view,
            query,
            replace,
            ..ViewStmt::default()
        }))
    }

    fn parse_create_table_as(&mut self, objtype: ObjectType) -> PResult<Node> {
        self.expect(TokenKind::View)?;
        let rel = self.try_parse_qualified_range_var().map(Box::new);
        let query = if self.consume(TokenKind::As) {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            self.skip_rest();
            None
        };
        Ok(Node::CreateTableAsStmt(CreateTableAsStmt {
            node_tag: NodeTag::CreateTableAsStmt,
            query,
            into: Some(Box::new(IntoClause {
                node_tag: NodeTag::IntoClause,
                rel,
                ..IntoClause::default()
            })),
            objtype,
            ..CreateTableAsStmt::default()
        }))
    }

    fn parse_index(&mut self, unique_seen: bool) -> PResult<Node> {
        let unique = unique_seen || self.consume(TokenKind::Unique);
        self.expect(TokenKind::Index)?;
        self.consume(TokenKind::Concurrently);
        let if_not_exists = self.consume_if_not_exists();
        let idxname = if self.peek_kind() != TokenKind::On {
            self.consume_name()
        } else {
            None
        };
        self.consume(TokenKind::On);
        let relation = self.try_parse_qualified_range_var().map(Box::new);
        self.skip_rest();
        Ok(Node::IndexStmt(IndexStmt {
            node_tag: NodeTag::IndexStmt,
            idxname,
            relation,
            unique,
            if_not_exists,
            ..IndexStmt::default()
        }))
    }

    fn parse_create_function(&mut self, replace: bool) -> PResult<Node> {
        let is_procedure = self.consume(TokenKind::Procedure);
        if !is_procedure {
            self.expect(TokenKind::Function)?;
        }
        let funcname = self.parse_name_list();
        self.skip_rest();
        Ok(Node::CreateFunctionStmt(CreateFunctionStmt {
            node_tag: NodeTag::CreateFunctionStmt,
            is_procedure,
            replace,
            funcname,
            ..CreateFunctionStmt::default()
        }))
    }

    fn parse_create_role(&mut self) -> PResult<Node> {
        let stmt_type = match self.advance().kind {
            TokenKind::User => RoleStmtType::User,
            TokenKind::GroupP => RoleStmtType::Group,
            _ => RoleStmtType::Role,
        };
        let role = self.consume_name();
        self.skip_rest();
        Ok(Node::CreateRoleStmt(CreateRoleStmt {
            node_tag: NodeTag::CreateRoleStmt,
            stmt_type,
            role,
            ..CreateRoleStmt::default()
        }))
    }

    fn parse_create_sequence(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Sequence)?;
        let if_not_exists = self.consume_if_not_exists();
        let sequence = self.try_parse_qualified_range_var().map(Box::new);
        self.skip_rest();
        Ok(Node::CreateSeqStmt(CreateSeqStmt {
            node_tag: NodeTag::CreateSeqStmt,
            sequence,
            if_not_exists,
            ..CreateSeqStmt::default()
        }))
    }

    fn parse_create_domain(&mut self) -> PResult<Node> {
        self.expect(TokenKind::DomainP)?;
        let domainname = self.parse_name_list();
        if self.consume(TokenKind::As) {
            let type_name = self.parse_type_name();
            self.skip_rest();
            Ok(Node::CreateDomainStmt(CreateDomainStmt {
                node_tag: NodeTag::CreateDomainStmt,
                domainname,
                type_name: Some(Box::new(type_name)),
                ..CreateDomainStmt::default()
            }))
        } else {
            self.skip_rest();
            Ok(Node::CreateDomainStmt(CreateDomainStmt {
                node_tag: NodeTag::CreateDomainStmt,
                domainname,
                ..CreateDomainStmt::default()
            }))
        }
    }

    fn parse_create_type(&mut self) -> PResult<Node> {
        self.expect(TokenKind::TypeP)?;
        let type_name = self.parse_name_list();
        if !self.consume(TokenKind::As) {
            self.skip_rest();
            return Ok(Node::DefineStmt(DefineStmt {
                node_tag: NodeTag::DefineStmt,
                kind: ObjectType::Type,
                defnames: type_name,
                ..DefineStmt::default()
            }));
        }

        if self.consume(TokenKind::EnumP) {
            let vals = if self.consume(TokenKind::Char('(')) {
                let vals = self.parse_expr_list_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                vals
            } else {
                Vec::new()
            };
            self.skip_rest();
            Ok(Node::CreateEnumStmt(CreateEnumStmt {
                node_tag: NodeTag::CreateEnumStmt,
                type_name,
                vals,
            }))
        } else if self.consume(TokenKind::Range) {
            self.skip_rest();
            Ok(Node::CreateRangeStmt(CreateRangeStmt {
                node_tag: NodeTag::CreateRangeStmt,
                type_name,
                ..CreateRangeStmt::default()
            }))
        } else {
            let coldeflist = if self.consume(TokenKind::Char('(')) {
                self.parse_column_defs()
            } else {
                Vec::new()
            };
            self.skip_rest();
            Ok(Node::CompositeTypeStmt(CompositeTypeStmt {
                node_tag: NodeTag::CompositeTypeStmt,
                typevar: Some(Box::new(range_var_from_parts(list_to_names(&type_name), 0))),
                coldeflist,
            }))
        }
    }

    fn parse_alter(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Alter)?;
        if self.peek_kind() == TokenKind::Default && self.peek_kind_n(1) == TokenKind::Privileges {
            return self.parse_alter_default_privileges();
        }
        if self.peek_kind() == TokenKind::TypeP && self.looks_like_alter_enum() {
            return self.parse_alter_enum();
        }
        if self.looks_like_rename_stmt() {
            return self.parse_rename();
        }
        if self.looks_like_alter_object_depends_stmt() {
            return self.parse_alter_object_depends();
        }
        if self.looks_like_alter_object_schema_stmt() {
            return self.parse_alter_object_schema();
        }
        if self.looks_like_alter_owner_stmt() {
            return self.parse_alter_owner();
        }
        let node = match self.peek_kind() {
            TokenKind::Table => self.parse_alter_table(ObjectType::Table)?,
            TokenKind::Index => self.parse_alter_table(ObjectType::Index)?,
            TokenKind::Sequence => self.parse_alter_sequence()?,
            TokenKind::View => self.parse_alter_table(ObjectType::View)?,
            TokenKind::Materialized if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.parse_alter_table(ObjectType::Matview)?
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Table => {
                self.advance();
                self.parse_alter_table(ObjectType::ForeignTable)?
            }
            TokenKind::Database => self.parse_alter_database()?,
            TokenKind::SystemP => self.parse_alter_system()?,
            TokenKind::Tablespace => self.parse_alter_tablespace()?,
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_alter_user_mapping()
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_alter_role()?,
            TokenKind::DomainP => self.parse_alter_domain(),
            TokenKind::TypeP if self.looks_like_alter_composite_type() => {
                self.parse_alter_composite_type()
            }
            TokenKind::TypeP => self.parse_alter_type(),
            TokenKind::Extension => self.parse_alter_extension(),
            TokenKind::Collation => self.parse_alter_collation(),
            TokenKind::Policy => self.parse_alter_policy(),
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.parse_alter_prop_graph()
            }
            TokenKind::Publication => self.parse_alter_publication(),
            TokenKind::Subscription => self.parse_alter_subscription(),
            TokenKind::Statistics => self.parse_alter_stats(),
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.parse_alter_event_trigger()
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.consume(TokenKind::DataP);
                self.consume(TokenKind::Wrapper);
                self.parse_alter_fdw()
            }
            TokenKind::Server => self.parse_alter_foreign_server(),
            TokenKind::Function
            | TokenKind::Procedure
            | TokenKind::Routine
            | TokenKind::Aggregate => self.parse_alter_function(),
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.parse_alter_op_family()
            }
            TokenKind::Operator => self.parse_alter_operator(),
            TokenKind::TextP if self.peek_kind_n(1) == TokenKind::Search => {
                self.advance();
                self.consume(TokenKind::Search);
                if self.consume(TokenKind::Dictionary) {
                    self.parse_alter_ts_dictionary()
                } else {
                    self.consume(TokenKind::Configuration);
                    self.parse_alter_ts_configuration()
                }
            }
            other => return Err(self.error_here(format!("unsupported ALTER form {:?}", other))),
        };
        Ok(node)
    }

    fn parse_alter_default_privileges(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Default)?;
        self.expect(TokenKind::Privileges)?;
        let mut options = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Grant | TokenKind::Revoke | TokenKind::Eof
        ) {
            let location = self.location();
            match self.peek_kind() {
                TokenKind::For => {
                    self.advance();
                    self.consume(TokenKind::Role);
                    self.consume(TokenKind::User);
                    let roles = self.parse_name_list_list_until(&[
                        TokenKind::InP,
                        TokenKind::Grant,
                        TokenKind::Revoke,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                    options.push(Node::DefElem(DefElem {
                        node_tag: NodeTag::DefElem,
                        defname: Some("roles".to_owned()),
                        arg: Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: roles,
                            ..AArrayExpr::default()
                        }))),
                        location: location as ParseLoc,
                        ..DefElem::default()
                    }));
                }
                TokenKind::InP => {
                    self.advance();
                    self.consume(TokenKind::Schema);
                    let schemas = self.parse_name_list_list_until(&[
                        TokenKind::Grant,
                        TokenKind::Revoke,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                    options.push(Node::DefElem(DefElem {
                        node_tag: NodeTag::DefElem,
                        defname: Some("schemas".to_owned()),
                        arg: Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: schemas,
                            ..AArrayExpr::default()
                        }))),
                        location: location as ParseLoc,
                        ..DefElem::default()
                    }));
                }
                _ => {
                    self.advance();
                }
            }
        }
        let action = if self.at(TokenKind::Grant) {
            match self.parse_grant(true)? {
                Node::GrantStmt(stmt) => Some(Box::new(stmt)),
                _ => None,
            }
        } else if self.at(TokenKind::Revoke) {
            match self.parse_grant(false)? {
                Node::GrantStmt(stmt) => Some(Box::new(stmt)),
                _ => None,
            }
        } else {
            None
        };
        Ok(Node::AlterDefaultPrivilegesStmt(
            AlterDefaultPrivilegesStmt {
                node_tag: NodeTag::AlterDefaultPrivilegesStmt,
                options,
                action,
            },
        ))
    }

    fn parse_alter_table(&mut self, objtype: ObjectType) -> PResult<Node> {
        self.advance();
        let missing_ok = self.consume_if_exists();
        let relation = self.try_parse_qualified_range_var().map(Box::new);
        let cmds = self.parse_alter_table_cmds();
        self.skip_rest();
        Ok(Node::AlterTableStmt(AlterTableStmt {
            node_tag: NodeTag::AlterTableStmt,
            relation,
            cmds,
            objtype,
            missing_ok,
        }))
    }

    fn parse_alter_user_mapping(&mut self) -> Node {
        self.expect(TokenKind::User).ok();
        self.expect(TokenKind::Mapping).ok();
        self.consume(TokenKind::For);
        let user = self.consume_role_spec().map(Box::new);
        self.skip_until_top_level(&[
            TokenKind::Server,
            TokenKind::Options,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let servername = if self.consume(TokenKind::Server) {
            self.consume_name()
        } else {
            None
        };
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::AlterUserMappingStmt(AlterUserMappingStmt {
            node_tag: NodeTag::AlterUserMappingStmt,
            user,
            servername,
            options,
        })
    }

    fn parse_alter_domain(&mut self) -> Node {
        self.expect(TokenKind::DomainP).ok();
        let type_name = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Drop,
            TokenKind::AddP,
            TokenKind::Validate,
            TokenKind::Rename,
            TokenKind::Owner,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let mut stmt = AlterDomainStmt {
            node_tag: NodeTag::AlterDomainStmt,
            type_name,
            ..AlterDomainStmt::default()
        };
        match self.peek_kind() {
            TokenKind::Set => {
                self.advance();
                if self.consume(TokenKind::Default) {
                    stmt.subtype = AlterDomainType::AlterDefault;
                    stmt.def = self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof]);
                } else if self.consume(TokenKind::Not) {
                    self.consume(TokenKind::NullP);
                    stmt.subtype = AlterDomainType::SetNotNull;
                }
            }
            TokenKind::Drop => {
                self.advance();
                if self.consume(TokenKind::Default) {
                    stmt.subtype = AlterDomainType::AlterDefault;
                } else if self.consume(TokenKind::Not) {
                    self.consume(TokenKind::NullP);
                    stmt.subtype = AlterDomainType::DropNotNull;
                } else if self.consume(TokenKind::Constraint) {
                    stmt.subtype = AlterDomainType::DropConstraint;
                    stmt.missing_ok = self.consume_if_exists();
                    stmt.name = self.consume_name();
                    stmt.behavior = self.parse_drop_behavior();
                }
            }
            TokenKind::AddP => {
                self.advance();
                stmt.subtype = AlterDomainType::AddConstraint;
                stmt.def = self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof]);
            }
            TokenKind::Validate => {
                self.advance();
                self.consume(TokenKind::Constraint);
                stmt.subtype = AlterDomainType::ValidateConstraint;
                stmt.name = self.consume_name();
            }
            _ => {}
        }
        self.skip_rest();
        Node::AlterDomainStmt(stmt)
    }

    fn parse_alter_type(&mut self) -> Node {
        self.expect(TokenKind::TypeP).ok();
        let type_name = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.consume(TokenKind::Set);
        let options = self.parse_def_elem_list();
        self.skip_rest();
        Node::AlterTypeStmt(AlterTypeStmt {
            node_tag: NodeTag::AlterTypeStmt,
            type_name,
            options,
        })
    }

    fn parse_alter_collation(&mut self) -> Node {
        self.expect(TokenKind::Collation).ok();
        let collname = self.parse_name_list_until_keywords(&[
            TokenKind::Refresh,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_rest();
        Node::AlterCollationStmt(AlterCollationStmt {
            node_tag: NodeTag::AlterCollationStmt,
            collname,
        })
    }

    fn parse_alter_policy(&mut self) -> Node {
        self.expect(TokenKind::Policy).ok();
        let missing = self.consume_if_exists();
        let policy_name = self.consume_name();
        self.skip_until_top_level(&[
            TokenKind::On,
            TokenKind::To,
            TokenKind::Using,
            TokenKind::With,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let table = if self.consume(TokenKind::On) {
            self.try_parse_qualified_range_var().map(Box::new)
        } else {
            None
        };
        let roles = if self.consume(TokenKind::To) {
            self.parse_name_list_list_until(&[
                TokenKind::Using,
                TokenKind::With,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            Vec::new()
        };
        let qual = if self.consume(TokenKind::Using) {
            if self.consume(TokenKind::Char('(')) {
                let expr = self.parse_expr_box_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                expr
            } else {
                self.parse_expr_box_until(&[TokenKind::With, TokenKind::Char(';'), TokenKind::Eof])
            }
        } else {
            None
        };
        let with_check = if self.consume(TokenKind::With) {
            self.consume(TokenKind::Check);
            if self.consume(TokenKind::Char('(')) {
                let expr = self.parse_expr_box_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                expr
            } else {
                self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof])
            }
        } else {
            None
        };
        self.skip_rest();
        let _ = missing;
        Node::AlterPolicyStmt(AlterPolicyStmt {
            node_tag: NodeTag::AlterPolicyStmt,
            policy_name,
            table,
            roles,
            qual,
            with_check,
        })
    }

    fn parse_alter_prop_graph(&mut self) -> Node {
        self.expect(TokenKind::Graph).ok();
        let pgname = self.try_parse_qualified_range_var().map(Box::new);
        let mut stmt = AlterPropGraphStmt {
            node_tag: NodeTag::AlterPropGraphStmt,
            pgname,
            ..AlterPropGraphStmt::default()
        };
        if self.consume(TokenKind::AddP) {
            if matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node) {
                self.advance();
                self.consume(TokenKind::Tables);
                stmt.add_vertex_tables = self.parse_prop_graph_vertex_list();
            }
            if matches!(self.peek_kind(), TokenKind::Edge | TokenKind::Relationship) {
                self.advance();
                self.consume(TokenKind::Tables);
                stmt.add_edge_tables = self.parse_prop_graph_edge_list();
            }
        } else if self.consume(TokenKind::Drop) {
            let vertex = matches!(self.peek_kind(), TokenKind::Vertex | TokenKind::Node);
            self.advance();
            self.consume(TokenKind::Tables);
            if self.consume(TokenKind::Char('(')) {
                let names = self.parse_expr_list_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                if vertex {
                    stmt.drop_vertex_tables = names;
                } else {
                    stmt.drop_edge_tables = names;
                }
            }
            stmt.drop_behavior = self.parse_drop_behavior();
        }
        self.skip_rest();
        Node::AlterPropGraphStmt(stmt)
    }

    fn parse_alter_sequence(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Sequence)?;
        let missing_ok = self.consume_if_exists();
        let sequence = self.try_parse_qualified_range_var().map(Box::new);
        self.skip_rest();
        Ok(Node::AlterSeqStmt(AlterSeqStmt {
            node_tag: NodeTag::AlterSeqStmt,
            sequence,
            missing_ok,
            ..AlterSeqStmt::default()
        }))
    }

    fn parse_alter_database(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        let dbname = self.consume_name();
        if self.consume(TokenKind::Refresh) {
            self.consume(TokenKind::Collation);
            self.consume(TokenKind::VersionP);
            self.skip_rest();
            Ok(Node::AlterDatabaseRefreshCollStmt(
                AlterDatabaseRefreshCollStmt {
                    node_tag: NodeTag::AlterDatabaseRefreshCollStmt,
                    dbname,
                },
            ))
        } else if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            let setstmt = Some(Box::new(self.parse_variable_set_like()?));
            self.skip_rest();
            Ok(Node::AlterDatabaseSetStmt(AlterDatabaseSetStmt {
                node_tag: NodeTag::AlterDatabaseSetStmt,
                dbname,
                setstmt,
            }))
        } else {
            self.skip_rest();
            Ok(Node::AlterDatabaseStmt(AlterDatabaseStmt {
                node_tag: NodeTag::AlterDatabaseStmt,
                dbname,
                ..AlterDatabaseStmt::default()
            }))
        }
    }

    fn parse_alter_system(&mut self) -> PResult<Node> {
        self.expect(TokenKind::SystemP)?;
        let setstmt = if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            Some(Box::new(self.parse_variable_set_like()?))
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::AlterSystemStmt(AlterSystemStmt {
            node_tag: NodeTag::AlterSystemStmt,
            setstmt,
        }))
    }

    fn parse_alter_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let tablespacename = self.consume_name();
        if self.consume(TokenKind::Rename) {
            self.skip_until_top_level(&[TokenKind::To, TokenKind::Char(';'), TokenKind::Eof]);
            let newname = if self.consume(TokenKind::To) {
                self.consume_name()
            } else {
                None
            };
            self.skip_rest();
            return Ok(Node::RenameStmt(RenameStmt {
                node_tag: NodeTag::RenameStmt,
                rename_type: ObjectType::Tablespace,
                subname: tablespacename,
                newname,
                ..RenameStmt::default()
            }));
        }
        let is_reset = if self.consume(TokenKind::Set) {
            false
        } else {
            self.consume(TokenKind::Reset);
            true
        };
        self.skip_rest();
        Ok(Node::AlterTableSpaceOptionsStmt(
            AlterTableSpaceOptionsStmt {
                node_tag: NodeTag::AlterTableSpaceOptionsStmt,
                tablespacename,
                is_reset,
                ..AlterTableSpaceOptionsStmt::default()
            },
        ))
    }

    fn parse_alter_role(&mut self) -> PResult<Node> {
        self.advance();
        let role = self.consume_name().map(|rolename| {
            Box::new(RoleSpec {
                node_tag: NodeTag::RoleSpec,
                rolename: Some(rolename),
                ..RoleSpec::default()
            })
        });
        let database = if self.consume(TokenKind::InP) {
            self.consume(TokenKind::Database);
            self.consume_name()
        } else {
            None
        };
        if matches!(self.peek_kind(), TokenKind::Set | TokenKind::Reset) {
            let setstmt = Some(Box::new(self.parse_variable_set_like()?));
            self.skip_rest();
            return Ok(Node::AlterRoleSetStmt(AlterRoleSetStmt {
                node_tag: NodeTag::AlterRoleSetStmt,
                role,
                database,
                setstmt,
            }));
        }
        self.skip_rest();
        Ok(Node::AlterRoleStmt(AlterRoleStmt {
            node_tag: NodeTag::AlterRoleStmt,
            role,
            ..AlterRoleStmt::default()
        }))
    }

    fn parse_alter_enum(&mut self) -> PResult<Node> {
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

        if self.consume(TokenKind::AddP) {
            self.consume(TokenKind::ValueP);
            stmt.skip_if_new_val_exists = self.consume_if_not_exists();
            stmt.new_val = self.consume_string_like();
            if self.consume(TokenKind::Before) {
                stmt.new_val_neighbor = self.consume_string_like();
                stmt.new_val_is_after = false;
            } else if self.consume(TokenKind::After) {
                stmt.new_val_neighbor = self.consume_string_like();
                stmt.new_val_is_after = true;
            } else {
                stmt.new_val_is_after = true;
            }
        } else if self.consume(TokenKind::Rename) {
            self.consume(TokenKind::ValueP);
            stmt.old_val = self.consume_string_like();
            self.consume(TokenKind::To);
            stmt.new_val = self.consume_string_like();
        } else if self.consume(TokenKind::Drop) {
            self.consume(TokenKind::ValueP);
            stmt.old_val = self.consume_string_like();
        }

        self.skip_rest();
        Ok(Node::AlterEnumStmt(stmt))
    }

    fn parse_alter_composite_type(&mut self) -> Node {
        self.expect(TokenKind::TypeP).ok();
        let names = self.parse_name_list_until_keywords(&[
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Alter,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let relation = Some(Box::new(range_var_from_parts(list_to_names(&names), 0)));
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                self.consume(TokenKind::Attribute);
                cmd.subtype = AlterTableType::AddColumn;
                cmd.name = self.consume_name();
            }
            TokenKind::Drop => {
                self.advance();
                self.consume(TokenKind::Attribute);
                cmd.subtype = AlterTableType::DropColumn;
                cmd.missing_ok = self.consume_if_exists();
                cmd.name = self.consume_name();
            }
            TokenKind::Alter => {
                self.advance();
                self.consume(TokenKind::Attribute);
                cmd.subtype = AlterTableType::AlterColumnType;
                cmd.name = self.consume_name();
            }
            _ => {}
        }
        self.skip_rest();
        Node::AlterTableStmt(AlterTableStmt {
            node_tag: NodeTag::AlterTableStmt,
            relation,
            cmds: vec![Node::AlterTableCmd(cmd)],
            objtype: ObjectType::Type,
            ..AlterTableStmt::default()
        })
    }

    fn parse_alter_extension(&mut self) -> Node {
        self.expect(TokenKind::Extension).ok();
        let extname = self.consume_name();
        if matches!(self.peek_kind(), TokenKind::AddP | TokenKind::Drop) {
            let action = if self.consume(TokenKind::AddP) { 1 } else { -1 };
            let objtype = self.consume_object_type().unwrap_or(ObjectType::Extension);
            self.skip_rest();
            Node::AlterExtensionContentsStmt(AlterExtensionContentsStmt {
                node_tag: NodeTag::AlterExtensionContentsStmt,
                extname,
                action,
                objtype,
                ..AlterExtensionContentsStmt::default()
            })
        } else {
            self.skip_rest();
            Node::AlterExtensionStmt(AlterExtensionStmt {
                node_tag: NodeTag::AlterExtensionStmt,
                extname,
                ..AlterExtensionStmt::default()
            })
        }
    }

    fn parse_alter_op_family(&mut self) -> Node {
        self.expect(TokenKind::Family).ok();
        let opfamilyname = self.parse_name_list_until_keywords(&[
            TokenKind::Using,
            TokenKind::AddP,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let amname = if self.consume(TokenKind::Using) {
            self.consume_name()
        } else {
            None
        };
        let is_drop = if self.consume(TokenKind::AddP) {
            false
        } else {
            self.consume(TokenKind::Drop)
        };
        self.skip_rest();
        Node::AlterOpFamilyStmt(AlterOpFamilyStmt {
            node_tag: NodeTag::AlterOpFamilyStmt,
            opfamilyname,
            amname,
            is_drop,
            ..AlterOpFamilyStmt::default()
        })
    }

    fn parse_alter_publication(&mut self) -> Node {
        self.expect(TokenKind::Publication).ok();
        let pubname = self.consume_name();
        let action = if self.consume(TokenKind::AddP) {
            AlterPublicationAction::AddObjects
        } else if self.consume(TokenKind::Drop) {
            AlterPublicationAction::DropObjects
        } else {
            self.consume(TokenKind::Set);
            AlterPublicationAction::SetObjects
        };
        let mut for_all_tables = false;
        let mut for_all_sequences = false;
        let mut pubobjects = Vec::new();
        if self.consume(TokenKind::All) {
            if self.consume(TokenKind::Tables) {
                for_all_tables = true;
            } else if self.consume(TokenKind::Sequences) {
                for_all_sequences = true;
            }
        } else if self.consume(TokenKind::Table) || self.consume(TokenKind::Tables) {
            pubobjects = self.parse_from_clause_until(&[
                TokenKind::With,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
        }
        let options = if self.consume(TokenKind::With) {
            self.parse_def_elem_list()
        } else {
            Vec::new()
        };
        self.skip_rest();
        Node::AlterPublicationStmt(AlterPublicationStmt {
            node_tag: NodeTag::AlterPublicationStmt,
            pubname,
            options,
            pubobjects,
            action,
            for_all_tables,
            for_all_sequences,
        })
    }

    fn parse_alter_subscription(&mut self) -> Node {
        self.expect(TokenKind::Subscription).ok();
        let subname = self.consume_name();
        let mut stmt = AlterSubscriptionStmt {
            node_tag: NodeTag::AlterSubscriptionStmt,
            subname,
            ..AlterSubscriptionStmt::default()
        };
        stmt.kind = match self.peek_kind() {
            TokenKind::Connection => {
                self.advance();
                stmt.conninfo = self.consume_string_like();
                AlterSubscriptionType::Connection
            }
            TokenKind::Set if self.peek_kind_n(1) == TokenKind::Publication => {
                self.advance();
                self.advance();
                stmt.publication = self.parse_expr_list_until(&[
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
                AlterSubscriptionType::SetPublication
            }
            TokenKind::AddP => {
                self.advance();
                self.consume(TokenKind::Publication);
                stmt.publication = self.parse_expr_list_until(&[
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
                AlterSubscriptionType::AddPublication
            }
            TokenKind::Drop => {
                self.advance();
                self.consume(TokenKind::Publication);
                stmt.publication = self.parse_expr_list_until(&[
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
                AlterSubscriptionType::DropPublication
            }
            TokenKind::Refresh => {
                self.advance();
                if self.consume(TokenKind::Publication) {
                    AlterSubscriptionType::RefreshPublication
                } else {
                    self.consume(TokenKind::Sequences);
                    AlterSubscriptionType::RefreshSequences
                }
            }
            TokenKind::EnableP | TokenKind::DisableP => {
                self.advance();
                AlterSubscriptionType::Enabled
            }
            TokenKind::Skip => {
                self.advance();
                AlterSubscriptionType::Skip
            }
            _ => AlterSubscriptionType::Options,
        };
        if self.consume(TokenKind::With) {
            stmt.options = self.parse_def_elem_list();
        } else {
            stmt.options = self.parse_options_clause();
        }
        self.skip_rest();
        Node::AlterSubscriptionStmt(stmt)
    }

    fn parse_alter_stats(&mut self) -> Node {
        self.expect(TokenKind::Statistics).ok();
        let missing_ok = self.consume_if_exists();
        let defnames = self.parse_name_list_until_keywords(&[
            TokenKind::Set,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let stxstattarget = if self.consume(TokenKind::Set) {
            self.consume(TokenKind::Statistics);
            self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof])
        } else {
            None
        };
        self.skip_rest();
        Node::AlterStatsStmt(AlterStatsStmt {
            node_tag: NodeTag::AlterStatsStmt,
            defnames,
            stxstattarget,
            missing_ok,
        })
    }

    fn parse_alter_event_trigger(&mut self) -> Node {
        self.expect(TokenKind::Trigger).ok();
        let trigname = self.consume_name();
        let tgenabled = match self.peek_kind() {
            TokenKind::EnableP => {
                self.advance();
                if self.consume(TokenKind::Replica) {
                    b'R'
                } else if self.consume(TokenKind::Always) {
                    b'A'
                } else {
                    b'O'
                }
            }
            TokenKind::DisableP => {
                self.advance();
                b'D'
            }
            _ => 0,
        };
        self.skip_rest();
        Node::AlterEventTrigStmt(AlterEventTrigStmt {
            node_tag: NodeTag::AlterEventTrigStmt,
            trigname,
            tgenabled,
        })
    }

    fn parse_alter_fdw(&mut self) -> Node {
        let fdwname = self.consume_name();
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::AlterFdwStmt(AlterFdwStmt {
            node_tag: NodeTag::AlterFdwStmt,
            fdwname,
            options,
            ..AlterFdwStmt::default()
        })
    }

    fn parse_alter_foreign_server(&mut self) -> Node {
        self.expect(TokenKind::Server).ok();
        let servername = self.consume_name();
        let mut version = None;
        let mut has_version = false;
        if self.consume(TokenKind::VersionP) {
            has_version = true;
            version = if self.consume(TokenKind::NullP) {
                None
            } else {
                self.consume_string_like().or_else(|| self.consume_name())
            };
        }
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::AlterForeignServerStmt(AlterForeignServerStmt {
            node_tag: NodeTag::AlterForeignServerStmt,
            servername,
            version,
            options,
            has_version,
        })
    }

    fn parse_alter_function(&mut self) -> Node {
        let objtype = match self.advance().kind {
            TokenKind::Procedure => ObjectType::Procedure,
            TokenKind::Routine => ObjectType::Routine,
            TokenKind::Aggregate => ObjectType::Aggregate,
            _ => ObjectType::Function,
        };
        let func = self
            .parse_object_with_args_until(&[
                TokenKind::Set,
                TokenKind::Reset,
                TokenKind::Stable,
                TokenKind::Immutable,
                TokenKind::Volatile,
                TokenKind::Security,
                TokenKind::Owner,
                TokenKind::Rename,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
            .map(Box::new);
        let actions = self.parse_alter_actions_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        self.skip_rest();
        Node::AlterFunctionStmt(AlterFunctionStmt {
            node_tag: NodeTag::AlterFunctionStmt,
            objtype,
            func,
            actions,
        })
    }

    fn parse_alter_operator(&mut self) -> Node {
        self.expect(TokenKind::Operator).ok();
        let opername = self
            .parse_object_with_args_until(&[TokenKind::Set, TokenKind::Char(';'), TokenKind::Eof])
            .map(Box::new);
        self.consume(TokenKind::Set);
        let options = self.parse_def_elem_list();
        self.skip_rest();
        Node::AlterOperatorStmt(AlterOperatorStmt {
            node_tag: NodeTag::AlterOperatorStmt,
            opername,
            options,
        })
    }

    fn parse_alter_ts_dictionary(&mut self) -> Node {
        let dictname = self.parse_name_list_until_keywords(&[
            TokenKind::Char('('),
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let options = self.parse_def_elem_list();
        self.skip_rest();
        Node::AlterTsDictionaryStmt(AlterTsDictionaryStmt {
            node_tag: NodeTag::AlterTsDictionaryStmt,
            dictname,
            options,
        })
    }

    fn parse_alter_ts_configuration(&mut self) -> Node {
        let cfgname = self.parse_name_list_until_keywords(&[
            TokenKind::AddP,
            TokenKind::Alter,
            TokenKind::Drop,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let kind = if self.consume(TokenKind::AddP) {
            AlterTsConfigType::AddMapping
        } else if self.consume(TokenKind::Alter) {
            if self.top_level_contains(TokenKind::Replace) {
                AlterTsConfigType::ReplaceDict
            } else {
                AlterTsConfigType::AlterMappingForToken
            }
        } else {
            self.consume(TokenKind::Drop);
            AlterTsConfigType::DropMapping
        };
        self.skip_rest();
        Node::AlterTsConfigurationStmt(AlterTsConfigurationStmt {
            node_tag: NodeTag::AlterTsConfigurationStmt,
            kind,
            cfgname,
            ..AlterTsConfigurationStmt::default()
        })
    }

    fn parse_rename(&mut self) -> PResult<Node> {
        let rename_type = self.consume_alter_object_type();
        let missing_ok = self.consume_if_exists();
        let relation = if relation_object_type(rename_type) {
            self.try_parse_range_var().map(Box::new)
        } else {
            None
        };
        let mut subname = if relation.is_none() {
            self.consume_name()
        } else {
            None
        };

        self.skip_until_top_level(&[TokenKind::Rename, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Rename);
        if matches!(
            self.peek_kind(),
            TokenKind::Column | TokenKind::Constraint | TokenKind::Attribute
        ) {
            self.advance();
            subname = self.consume_name();
        }
        self.skip_until_top_level(&[TokenKind::To, TokenKind::Char(';'), TokenKind::Eof]);
        let newname = if self.consume(TokenKind::To) {
            self.consume_name()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::RenameStmt(RenameStmt {
            node_tag: NodeTag::RenameStmt,
            rename_type,
            relation_type: rename_type,
            relation,
            subname,
            newname,
            missing_ok,
            ..RenameStmt::default()
        }))
    }

    fn parse_alter_object_depends(&mut self) -> PResult<Node> {
        let object_type = self.consume_alter_object_type();
        let relation = if relation_object_type(object_type) {
            self.try_parse_range_var().map(Box::new)
        } else {
            None
        };
        let remove = self.consume(TokenKind::No);
        self.skip_until_top_level(&[TokenKind::Depends, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Depends);
        self.consume(TokenKind::On);
        self.consume(TokenKind::Extension);
        let extname = self
            .consume_name()
            .map(|value| Box::new(String::new(value)));
        self.skip_rest();
        Ok(Node::AlterObjectDependsStmt(AlterObjectDependsStmt {
            node_tag: NodeTag::AlterObjectDependsStmt,
            object_type,
            relation,
            extname,
            remove,
            ..AlterObjectDependsStmt::default()
        }))
    }

    fn parse_alter_object_schema(&mut self) -> PResult<Node> {
        let object_type = self.consume_alter_object_type();
        let missing_ok = self.consume_if_exists();
        let relation = if relation_object_type(object_type) {
            self.try_parse_range_var().map(Box::new)
        } else {
            None
        };
        self.skip_until_top_level(&[TokenKind::Set, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Set);
        self.consume(TokenKind::Schema);
        let newschema = self.consume_name();
        self.skip_rest();
        Ok(Node::AlterObjectSchemaStmt(AlterObjectSchemaStmt {
            node_tag: NodeTag::AlterObjectSchemaStmt,
            object_type,
            relation,
            newschema,
            missing_ok,
            ..AlterObjectSchemaStmt::default()
        }))
    }

    fn parse_alter_owner(&mut self) -> PResult<Node> {
        let object_type = self.consume_alter_object_type();
        let relation = if relation_object_type(object_type) {
            self.try_parse_range_var().map(Box::new)
        } else {
            None
        };
        self.skip_until_top_level(&[TokenKind::Owner, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Owner);
        self.consume(TokenKind::To);
        let newowner = self.consume_role_spec().map(Box::new);
        self.skip_rest();
        Ok(Node::AlterOwnerStmt(AlterOwnerStmt {
            node_tag: NodeTag::AlterOwnerStmt,
            object_type,
            relation,
            newowner,
            ..AlterOwnerStmt::default()
        }))
    }

    fn parse_drop(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Drop)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        match self.peek_kind() {
            TokenKind::Database => self.parse_drop_database(),
            TokenKind::Cast => Ok(self.parse_drop_special(ObjectType::Cast, concurrent)),
            TokenKind::Transform => Ok(self.parse_drop_special(ObjectType::Transform, concurrent)),
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                Ok(self.parse_drop_operator_family(ObjectType::Opclass, concurrent))
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                Ok(self.parse_drop_operator_family(ObjectType::Opfamily, concurrent))
            }
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_drop_user_mapping()
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_drop_role(),
            TokenKind::Owned => self.parse_drop_owned(),
            TokenKind::Tablespace => self.parse_drop_tablespace(),
            TokenKind::Subscription => self.parse_drop_subscription(),
            _ => Ok(self.parse_drop_stmt(concurrent)),
        }
    }

    fn parse_drop_stmt(&mut self, concurrent: bool) -> Node {
        let remove_type = self.consume_object_type().unwrap_or(ObjectType::Table);
        let missing_ok = self.consume_if_exists();
        let objects = self.parse_name_list_list_until(&[
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let behavior = self.parse_drop_behavior();
        self.skip_rest();
        Node::DropStmt(DropStmt {
            node_tag: NodeTag::DropStmt,
            objects,
            remove_type,
            behavior,
            missing_ok,
            concurrent,
        })
    }

    fn parse_drop_special(&mut self, remove_type: ObjectType, concurrent: bool) -> Node {
        self.advance();
        let missing_ok = self.consume_if_exists();
        self.skip_until_top_level(&[
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let behavior = self.parse_drop_behavior();
        self.skip_rest();
        Node::DropStmt(DropStmt {
            node_tag: NodeTag::DropStmt,
            remove_type,
            behavior,
            missing_ok,
            concurrent,
            ..DropStmt::default()
        })
    }

    fn parse_drop_operator_family(&mut self, remove_type: ObjectType, concurrent: bool) -> Node {
        self.expect(TokenKind::Operator).ok();
        self.advance();
        let missing_ok = self.consume_if_exists();
        let objects = self.parse_name_list_list_until(&[
            TokenKind::Using,
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_until_top_level(&[
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let behavior = self.parse_drop_behavior();
        self.skip_rest();
        Node::DropStmt(DropStmt {
            node_tag: NodeTag::DropStmt,
            objects,
            remove_type,
            behavior,
            missing_ok,
            concurrent,
        })
    }

    fn parse_drop_database(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Database)?;
        let missing_ok = self.consume_if_exists();
        let dbname = self.consume_name();
        self.skip_rest();
        Ok(Node::DropdbStmt(DropdbStmt {
            node_tag: NodeTag::DropdbStmt,
            dbname,
            missing_ok,
            ..DropdbStmt::default()
        }))
    }

    fn parse_drop_role(&mut self) -> PResult<Node> {
        self.advance();
        let missing_ok = self.consume_if_exists();
        let roles = self.parse_name_list_list_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        Ok(Node::DropRoleStmt(DropRoleStmt {
            node_tag: NodeTag::DropRoleStmt,
            roles,
            missing_ok,
        }))
    }

    fn parse_drop_owned(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Owned)?;
        self.consume(TokenKind::By);
        let roles = self.parse_name_list_list_until(&[
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let behavior = self.parse_drop_behavior();
        Ok(Node::DropOwnedStmt(DropOwnedStmt {
            node_tag: NodeTag::DropOwnedStmt,
            roles,
            behavior,
        }))
    }

    fn parse_drop_tablespace(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Tablespace)?;
        let missing_ok = self.consume_if_exists();
        let tablespacename = self.consume_name();
        self.skip_rest();
        Ok(Node::DropTableSpaceStmt(DropTableSpaceStmt {
            node_tag: NodeTag::DropTableSpaceStmt,
            tablespacename,
            missing_ok,
        }))
    }

    fn parse_drop_subscription(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Subscription)?;
        let missing_ok = self.consume_if_exists();
        let subname = self.consume_name();
        let behavior = self.parse_drop_behavior();
        self.skip_rest();
        Ok(Node::DropSubscriptionStmt(DropSubscriptionStmt {
            node_tag: NodeTag::DropSubscriptionStmt,
            subname,
            missing_ok,
            behavior,
        }))
    }

    fn parse_drop_user_mapping(&mut self) -> PResult<Node> {
        self.expect(TokenKind::User)?;
        self.expect(TokenKind::Mapping)?;
        let missing_ok = self.consume_if_exists();
        self.consume(TokenKind::For);
        let user = self.consume_role_spec().map(Box::new);
        self.skip_until_top_level(&[TokenKind::Server, TokenKind::Char(';'), TokenKind::Eof]);
        let servername = if self.consume(TokenKind::Server) {
            self.consume_name()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::DropUserMappingStmt(DropUserMappingStmt {
            node_tag: NodeTag::DropUserMappingStmt,
            user,
            servername,
            missing_ok,
        }))
    }

    fn parse_set_or_constraints(&mut self) -> PResult<Node> {
        if self.peek_kind_n(1) == TokenKind::Constraints {
            self.expect(TokenKind::Set)?;
            self.expect(TokenKind::Constraints)?;
            let constraints = self.parse_name_list_list_until(&[
                TokenKind::Deferred,
                TokenKind::Immediate,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            let deferred = self.consume(TokenKind::Deferred);
            self.consume(TokenKind::Immediate);
            Ok(Node::ConstraintsSetStmt(ConstraintsSetStmt {
                node_tag: NodeTag::ConstraintsSetStmt,
                constraints,
                deferred,
            }))
        } else {
            Ok(Node::VariableSetStmt(self.parse_variable_set_like()?))
        }
    }

    fn parse_variable_set_like(&mut self) -> PResult<VariableSetStmt> {
        let location = self.location();
        let is_reset = self.consume(TokenKind::Reset);
        if !is_reset {
            self.expect(TokenKind::Set)?;
        }
        let is_local = self.consume(TokenKind::Local);
        self.consume(TokenKind::Session);
        let kind = if is_reset {
            if self.consume(TokenKind::All) {
                VariableSetKind::ResetAll
            } else {
                VariableSetKind::Reset
            }
        } else {
            VariableSetKind::SetValue
        };
        let name = if kind != VariableSetKind::ResetAll {
            self.consume_setting_name()
        } else {
            None
        };
        if self.consume(TokenKind::To) || self.consume(TokenKind::Char('=')) {
            // consumed assignment marker
        }
        let args = self.parse_expr_list_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        Ok(VariableSetStmt {
            node_tag: NodeTag::VariableSetStmt,
            kind,
            name,
            args,
            is_local,
            location: location as ParseLoc,
            ..VariableSetStmt::default()
        })
    }

    fn parse_variable_reset(&mut self) -> PResult<Node> {
        Ok(Node::VariableSetStmt(self.parse_variable_set_like()?))
    }

    fn parse_variable_show(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Show)?;
        let name = if self.consume(TokenKind::All) {
            Some("all".to_owned())
        } else {
            self.consume_setting_name()
        };
        self.skip_rest();
        Ok(Node::VariableShowStmt(VariableShowStmt {
            node_tag: NodeTag::VariableShowStmt,
            name,
        }))
    }

    fn parse_transaction(&mut self) -> PResult<Node> {
        let location = self.location();
        let kind = match self.advance().kind {
            TokenKind::BeginP => TransactionStmtKind::Begin,
            TokenKind::Start => TransactionStmtKind::Start,
            TokenKind::Commit | TokenKind::EndP => TransactionStmtKind::Commit,
            TokenKind::Rollback | TokenKind::AbortP => {
                if self.consume(TokenKind::To) {
                    TransactionStmtKind::RollbackTo
                } else if self.consume(TokenKind::Prepared) {
                    TransactionStmtKind::RollbackPrepared
                } else {
                    TransactionStmtKind::Rollback
                }
            }
            TokenKind::Savepoint => TransactionStmtKind::Savepoint,
            TokenKind::Release => TransactionStmtKind::Release,
            TokenKind::Prepare => TransactionStmtKind::Prepare,
            _ => TransactionStmtKind::Begin,
        };
        let savepoint_name = if matches!(
            kind,
            TransactionStmtKind::Savepoint
                | TransactionStmtKind::Release
                | TransactionStmtKind::RollbackTo
        ) {
            self.consume_name()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::TransactionStmt(TransactionStmt {
            node_tag: NodeTag::TransactionStmt,
            kind,
            savepoint_name,
            location: location as ParseLoc,
            ..TransactionStmt::default()
        }))
    }

    fn parse_prepare(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Prepare)?;
        let name = self.consume_name();
        self.skip_until_top_level(&[TokenKind::As, TokenKind::Char(';'), TokenKind::Eof]);
        let query = if self.consume(TokenKind::As) {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            None
        };
        Ok(Node::PrepareStmt(PrepareStmt {
            node_tag: NodeTag::PrepareStmt,
            name,
            query,
            ..PrepareStmt::default()
        }))
    }

    fn parse_execute(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Execute)?;
        let name = self.consume_name();
        let params = if self.consume(TokenKind::Char('(')) {
            let params = self.parse_expr_list_until(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            params
        } else {
            Vec::new()
        };
        self.skip_rest();
        Ok(Node::ExecuteStmt(ExecuteStmt {
            node_tag: NodeTag::ExecuteStmt,
            name,
            params,
        }))
    }

    fn parse_deallocate(&mut self) -> PResult<Node> {
        let location = self.expect(TokenKind::Deallocate)?.location;
        self.consume(TokenKind::Prepare);
        let isall = self.consume(TokenKind::All);
        let name = if isall { None } else { self.consume_name() };
        self.skip_rest();
        Ok(Node::DeallocateStmt(DeallocateStmt {
            node_tag: NodeTag::DeallocateStmt,
            name,
            isall,
            location: location as ParseLoc,
        }))
    }

    fn parse_explain(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Explain)?;
        if self.consume(TokenKind::Char('(')) {
            self.skip_until_top_level(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
        }
        let query = if !self.at_statement_end() {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            None
        };
        Ok(Node::ExplainStmt(ExplainStmt {
            node_tag: NodeTag::ExplainStmt,
            query,
            ..ExplainStmt::default()
        }))
    }

    fn parse_call(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Call)?;
        let funccall = self.parse_func_call().map(Box::new);
        self.skip_rest();
        Ok(Node::CallStmt(CallStmt {
            node_tag: NodeTag::CallStmt,
            funccall,
            ..CallStmt::default()
        }))
    }

    fn parse_copy(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Copy)?;
        let relation = self.try_parse_qualified_range_var().map(Box::new);
        let mut is_from = false;
        self.skip_until_top_level(&[
            TokenKind::From,
            TokenKind::To,
            TokenKind::Where,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if self.consume(TokenKind::From) {
            is_from = true;
        } else {
            self.consume(TokenKind::To);
        }
        let filename = self.consume_string_like();
        let where_clause = if self.consume(TokenKind::Where) {
            self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof])
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::CopyStmt(CopyStmt {
            node_tag: NodeTag::CopyStmt,
            relation,
            is_from,
            filename,
            where_clause,
            ..CopyStmt::default()
        }))
    }

    fn parse_vacuum(&mut self) -> PResult<Node> {
        let is_vacuumcmd = self.consume(TokenKind::Vacuum);
        if !is_vacuumcmd {
            self.advance();
        }
        let rels = self
            .parse_name_list_list_until(&[TokenKind::Char(';'), TokenKind::Eof])
            .into_iter()
            .map(|node| {
                Node::VacuumRelation(VacuumRelation {
                    node_tag: NodeTag::VacuumRelation,
                    relation: node_to_range_var(node).map(Box::new),
                    ..VacuumRelation::default()
                })
            })
            .collect();
        Ok(Node::VacuumStmt(VacuumStmt {
            node_tag: NodeTag::VacuumStmt,
            rels,
            is_vacuumcmd,
            ..VacuumStmt::default()
        }))
    }

    fn parse_checkpoint(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Checkpoint)?;
        self.skip_rest();
        Ok(Node::CheckPointStmt(CheckPointStmt {
            node_tag: NodeTag::CheckPointStmt,
            ..CheckPointStmt::default()
        }))
    }

    fn parse_discard(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Discard)?;
        let target = match self.advance().kind {
            TokenKind::Plans => DiscardMode::Plans,
            TokenKind::Sequences => DiscardMode::Sequences,
            TokenKind::Temp | TokenKind::Temporary => DiscardMode::Temp,
            _ => DiscardMode::All,
        };
        self.skip_rest();
        Ok(Node::DiscardStmt(DiscardStmt {
            node_tag: NodeTag::DiscardStmt,
            target,
        }))
    }

    fn parse_lock(&mut self) -> PResult<Node> {
        self.expect(TokenKind::LockP)?;
        self.consume(TokenKind::Table);
        let relations = self.parse_name_list_list_until(&[
            TokenKind::InP,
            TokenKind::Nowait,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        self.skip_rest();
        Ok(Node::LockStmt(LockStmt {
            node_tag: NodeTag::LockStmt,
            relations,
            ..LockStmt::default()
        }))
    }

    fn parse_listen(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Listen)?;
        let conditionname = self.consume_name();
        self.skip_rest();
        Ok(Node::ListenStmt(ListenStmt {
            node_tag: NodeTag::ListenStmt,
            conditionname,
        }))
    }

    fn parse_unlisten(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Unlisten)?;
        let conditionname = if self.consume(TokenKind::Char('*')) {
            None
        } else {
            self.consume_name()
        };
        self.skip_rest();
        Ok(Node::UnlistenStmt(UnlistenStmt {
            node_tag: NodeTag::UnlistenStmt,
            conditionname,
        }))
    }

    fn parse_notify(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Notify)?;
        let conditionname = self.consume_name();
        let payload = if self.consume(TokenKind::Char(',')) {
            self.consume_string_like()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::NotifyStmt(NotifyStmt {
            node_tag: NodeTag::NotifyStmt,
            conditionname,
            payload,
        }))
    }

    fn parse_load(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Load)?;
        let filename = self.consume_string_like();
        self.skip_rest();
        Ok(Node::LoadStmt(LoadStmt {
            node_tag: NodeTag::LoadStmt,
            filename,
        }))
    }

    fn parse_refresh(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Refresh)?;
        self.consume(TokenKind::Materialized);
        self.expect(TokenKind::View)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        let relation = self.try_parse_qualified_range_var().map(Box::new);
        self.skip_rest();
        Ok(Node::RefreshMatViewStmt(RefreshMatViewStmt {
            node_tag: NodeTag::RefreshMatViewStmt,
            concurrent,
            relation,
            ..RefreshMatViewStmt::default()
        }))
    }

    fn parse_reindex(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Reindex)?;
        let kind = match self.advance().kind {
            TokenKind::Table => ReindexObjectType::Table,
            TokenKind::Schema => ReindexObjectType::Schema,
            TokenKind::SystemP => ReindexObjectType::System,
            TokenKind::Database => ReindexObjectType::Database,
            _ => ReindexObjectType::Index,
        };
        let relation = self.try_parse_range_var().map(Box::new);
        self.skip_rest();
        Ok(Node::ReindexStmt(ReindexStmt {
            node_tag: NodeTag::ReindexStmt,
            kind,
            relation,
            ..ReindexStmt::default()
        }))
    }

    fn parse_repack(&mut self) -> PResult<Node> {
        let command = if self.consume(TokenKind::Cluster) {
            RepackCommand::Cluster
        } else {
            self.expect(TokenKind::Repack)?;
            RepackCommand::Repack
        };
        if self.consume(TokenKind::Char('(')) {
            self.skip_until_top_level(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
        }
        self.consume(TokenKind::Verbose);
        let relation = self.try_parse_range_var().map(|relation| {
            Box::new(VacuumRelation {
                node_tag: NodeTag::VacuumRelation,
                relation: Some(Box::new(relation)),
                ..VacuumRelation::default()
            })
        });
        let usingindex = self.consume(TokenKind::Using);
        self.consume(TokenKind::Index);
        let indexname = if usingindex || self.previous_kind() == TokenKind::Index {
            self.consume_name()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::RepackStmt(RepackStmt {
            node_tag: NodeTag::RepackStmt,
            command,
            relation,
            indexname,
            usingindex,
            ..RepackStmt::default()
        }))
    }

    fn parse_reassign_owned(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Reassign)?;
        self.expect(TokenKind::Owned)?;
        self.expect(TokenKind::By)?;
        let roles =
            self.parse_name_list_list_until(&[TokenKind::To, TokenKind::Char(';'), TokenKind::Eof]);
        let newrole = if self.consume(TokenKind::To) {
            self.consume_name().map(|rolename| {
                Box::new(RoleSpec {
                    node_tag: NodeTag::RoleSpec,
                    rolename: Some(rolename),
                    ..RoleSpec::default()
                })
            })
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::ReassignOwnedStmt(ReassignOwnedStmt {
            node_tag: NodeTag::ReassignOwnedStmt,
            roles,
            newrole,
        }))
    }

    fn parse_truncate(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Truncate)?;
        self.consume(TokenKind::Table);
        let relations = self.parse_name_list_list_until(&[
            TokenKind::Restart,
            TokenKind::ContinueP,
            TokenKind::Cascade,
            TokenKind::Restrict,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let behavior = self.parse_drop_behavior();
        Ok(Node::TruncateStmt(TruncateStmt {
            node_tag: NodeTag::TruncateStmt,
            relations,
            behavior,
            ..TruncateStmt::default()
        }))
    }

    fn parse_comment(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Comment)?;
        self.skip_until_top_level(&[TokenKind::Is, TokenKind::Char(';'), TokenKind::Eof]);
        let comment = if self.consume(TokenKind::Is) {
            self.consume_string_like()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::CommentStmt(CommentStmt {
            node_tag: NodeTag::CommentStmt,
            comment,
            ..CommentStmt::default()
        }))
    }

    fn parse_security_label(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Security)?;
        self.consume(TokenKind::Label);
        self.skip_until_top_level(&[TokenKind::Is, TokenKind::Char(';'), TokenKind::Eof]);
        let label = if self.consume(TokenKind::Is) {
            self.consume_string_like()
        } else {
            None
        };
        self.skip_rest();
        Ok(Node::SecLabelStmt(SecLabelStmt {
            node_tag: NodeTag::SecLabelStmt,
            label,
            ..SecLabelStmt::default()
        }))
    }

    fn parse_grant(&mut self, is_grant: bool) -> PResult<Node> {
        self.advance();
        let role_form = self.peek_kind() == TokenKind::Role;
        self.skip_rest();
        if role_form {
            Ok(Node::GrantRoleStmt(GrantRoleStmt {
                node_tag: NodeTag::GrantRoleStmt,
                is_grant,
                ..GrantRoleStmt::default()
            }))
        } else {
            Ok(Node::GrantStmt(GrantStmt {
                node_tag: NodeTag::GrantStmt,
                is_grant,
                ..GrantStmt::default()
            }))
        }
    }

    fn parse_import_foreign_schema(&mut self) -> PResult<Node> {
        self.expect(TokenKind::ImportP)?;
        self.consume(TokenKind::Foreign);
        self.expect(TokenKind::Schema)?;
        let remote_schema = self.consume_name();
        self.skip_rest();
        Ok(Node::ImportForeignSchemaStmt(ImportForeignSchemaStmt {
            node_tag: NodeTag::ImportForeignSchemaStmt,
            remote_schema,
            ..ImportForeignSchemaStmt::default()
        }))
    }

    fn parse_do(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Do)?;
        let args = self.parse_expr_list_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        Ok(Node::DoStmt(DoStmt {
            node_tag: NodeTag::DoStmt,
            args,
        }))
    }

    fn parse_return(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Return)?;
        let returnval = self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        Ok(Node::ReturnStmt(ReturnStmt {
            node_tag: NodeTag::ReturnStmt,
            returnval,
        }))
    }

    fn parse_wait(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Wait)?;
        self.consume(TokenKind::For);
        let lsn_literal = self.consume_string_like().or_else(|| self.consume_name());
        self.skip_rest();
        Ok(Node::WaitStmt(WaitStmt {
            node_tag: NodeTag::WaitStmt,
            lsn_literal,
            ..WaitStmt::default()
        }))
    }

    fn parse_declare_cursor(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Declare)?;
        let portalname = self.consume_name();
        self.skip_until_top_level(&[TokenKind::For, TokenKind::Char(';'), TokenKind::Eof]);
        let query = if self.consume(TokenKind::For) {
            Some(Box::new(self.parse_statement(None)?))
        } else {
            None
        };
        Ok(Node::DeclareCursorStmt(DeclareCursorStmt {
            node_tag: NodeTag::DeclareCursorStmt,
            portalname,
            query,
            ..DeclareCursorStmt::default()
        }))
    }

    fn parse_close(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Close)?;
        let portalname = self.consume_name();
        self.skip_rest();
        Ok(Node::ClosePortalStmt(ClosePortalStmt {
            node_tag: NodeTag::ClosePortalStmt,
            portalname,
        }))
    }

    fn parse_fetch_or_move(&mut self) -> PResult<Node> {
        let ismove = self.consume(TokenKind::Move);
        if !ismove {
            self.expect(TokenKind::Fetch)?;
        }
        let direction = if self.consume(TokenKind::Backward) {
            FetchDirection::Backward
        } else {
            self.consume(TokenKind::Forward);
            FetchDirection::Forward
        };
        self.skip_until_top_level(&[
            TokenKind::From,
            TokenKind::InP,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if self.consume(TokenKind::From) || self.consume(TokenKind::InP) {
            // portal name follows
        }
        let portalname = self.consume_name();
        self.skip_rest();
        Ok(Node::FetchStmt(FetchStmt {
            node_tag: NodeTag::FetchStmt,
            direction,
            portalname,
            ismove,
            ..FetchStmt::default()
        }))
    }

    fn parse_returning_clause(&mut self) -> Option<Box<ReturningClause>> {
        if !self.consume(TokenKind::Returning) {
            return None;
        }
        let exprs = self.parse_res_target_list_until(&[TokenKind::Char(';'), TokenKind::Eof]);
        Some(Box::new(ReturningClause {
            node_tag: NodeTag::ReturningClause,
            exprs,
            ..ReturningClause::default()
        }))
    }

    fn parse_values_lists(&mut self) -> NodeList {
        let mut values = Vec::new();
        while self.consume(TokenKind::Char('(')) {
            let elements = self.parse_expr_list_until(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            values.push(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements,
                ..AArrayExpr::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        values
    }

    fn parse_res_target_list_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            if tokens.is_empty() {
                break;
            }
            let (name, expr_tokens) = split_alias(tokens);
            items.push(Node::ResTarget(ResTarget {
                node_tag: NodeTag::ResTarget,
                name,
                val: tokens_to_node(expr_tokens).map(Box::new),
                location: location as ParseLoc,
                ..ResTarget::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn parse_expr_list_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            if let Some(node) = tokens_to_node(tokens) {
                items.push(node);
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn parse_expr_box_until(&mut self, stops: &[TokenKind]) -> Option<Box<Node>> {
        let tokens = self.take_until_top_level(stops);
        tokens_to_node(tokens).map(Box::new)
    }

    fn parse_sort_list_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            let mut sortby_dir = SortByDir::Default;
            let mut sortby_nulls = SortByNulls::Default;
            let mut expr_tokens = Vec::new();
            for token in tokens {
                match token.kind {
                    TokenKind::Asc => sortby_dir = SortByDir::Asc,
                    TokenKind::Desc => sortby_dir = SortByDir::Desc,
                    TokenKind::FirstP => sortby_nulls = SortByNulls::First,
                    TokenKind::LastP => sortby_nulls = SortByNulls::Last,
                    TokenKind::NullsP => {}
                    _ => expr_tokens.push(token),
                }
            }
            items.push(Node::SortBy(SortBy {
                node_tag: NodeTag::SortBy,
                node: tokens_to_node(expr_tokens).map(Box::new),
                sortby_dir,
                sortby_nulls,
                location: location as ParseLoc,
                ..SortBy::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn parse_from_clause_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            if let Some(item) = self.parse_from_item(stops) {
                items.push(item);
            } else {
                let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
                if let Some(node) = tokens_to_node(tokens) {
                    items.push(node);
                }
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn parse_from_item(&mut self, stops: &[TokenKind]) -> Option<Node> {
        let lateral = self.consume(TokenKind::LateralP);
        let mut base = if self.consume(TokenKind::Char('(')) {
            let inner = self.take_until_top_level(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            if let Some(subquery) = tokens_to_statement_node(inner.clone()) {
                Node::RangeSubselect(RangeSubselect {
                    node_tag: NodeTag::RangeSubselect,
                    lateral,
                    subquery: Some(Box::new(subquery)),
                    alias: self.parse_optional_alias(),
                })
            } else {
                tokens_to_node(inner)?
            }
        } else {
            let save = self.pos;
            let name_tokens = self.take_until_top_level(&[
                TokenKind::Char('('),
                TokenKind::As,
                TokenKind::Char(','),
                TokenKind::Join,
                TokenKind::InnerP,
                TokenKind::Left,
                TokenKind::Right,
                TokenKind::Full,
                TokenKind::Cross,
                TokenKind::Natural,
                TokenKind::On,
                TokenKind::Using,
                TokenKind::Tablesample,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            if self.at(TokenKind::Char('(')) && !name_tokens.is_empty() {
                self.pos = save;
                let func = self.parse_func_call()?;
                let ordinality =
                    self.consume(TokenKind::With) && self.consume(TokenKind::Ordinality);
                Node::RangeFunction(RangeFunction {
                    node_tag: NodeTag::RangeFunction,
                    lateral,
                    ordinality,
                    functions: vec![Node::FuncCall(func)],
                    alias: self.parse_optional_alias(),
                    ..RangeFunction::default()
                })
            } else {
                self.pos = save;
                self.try_parse_range_var().map(Node::RangeVar)?
            }
        };
        while !self.at_any(&extend_stops(stops, TokenKind::Char(','))) {
            if matches!(
                self.peek_kind(),
                TokenKind::Join
                    | TokenKind::InnerP
                    | TokenKind::Left
                    | TokenKind::Right
                    | TokenKind::Full
                    | TokenKind::Cross
                    | TokenKind::Natural
            ) {
                base = self.parse_join_tail(base, stops)?;
            } else {
                break;
            }
        }
        Some(base)
    }

    fn parse_join_tail(&mut self, larg: Node, stops: &[TokenKind]) -> Option<Node> {
        let jointype = match self.peek_kind() {
            TokenKind::Left => {
                self.advance();
                self.consume(TokenKind::OuterP);
                JoinType::Left
            }
            TokenKind::Right => {
                self.advance();
                self.consume(TokenKind::OuterP);
                JoinType::Right
            }
            TokenKind::Full => {
                self.advance();
                self.consume(TokenKind::OuterP);
                JoinType::Full
            }
            TokenKind::Cross | TokenKind::InnerP => {
                self.advance();
                JoinType::Inner
            }
            TokenKind::Natural => {
                self.advance();
                JoinType::Inner
            }
            TokenKind::Join => JoinType::Inner,
            _ => return Some(larg),
        };
        self.consume(TokenKind::Join);
        let rarg = self.parse_from_item(&[
            TokenKind::On,
            TokenKind::Using,
            TokenKind::Char(','),
            TokenKind::Join,
            TokenKind::InnerP,
            TokenKind::Left,
            TokenKind::Right,
            TokenKind::Full,
            TokenKind::Cross,
            TokenKind::Natural,
            TokenKind::Where,
            TokenKind::GroupP,
            TokenKind::Having,
            TokenKind::Window,
            TokenKind::Order,
            TokenKind::Limit,
            TokenKind::Offset,
            TokenKind::Fetch,
            TokenKind::For,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        let quals = if self.consume(TokenKind::On) {
            self.parse_expr_box_until(&extend_stops(stops, TokenKind::Char(',')))
        } else if self.consume(TokenKind::Using) {
            if self.consume(TokenKind::Char('(')) {
                let cols = self.parse_expr_list_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                return Some(Node::JoinExpr(JoinExpr {
                    node_tag: NodeTag::JoinExpr,
                    jointype,
                    larg: Some(Box::new(larg)),
                    rarg: Some(Box::new(rarg)),
                    using_clause: cols,
                    ..JoinExpr::default()
                }));
            }
            None
        } else {
            None
        };
        Some(Node::JoinExpr(JoinExpr {
            node_tag: NodeTag::JoinExpr,
            jointype,
            larg: Some(Box::new(larg)),
            rarg: Some(Box::new(rarg)),
            quals,
            ..JoinExpr::default()
        }))
    }

    fn parse_window_clause_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut windows = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let name = self.consume_name();
            self.consume(TokenKind::As);
            let mut window = WindowDef {
                node_tag: NodeTag::WindowDef,
                name,
                location: location as ParseLoc,
                ..WindowDef::default()
            };
            if self.consume(TokenKind::Char('(')) {
                while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
                    if self.consume(TokenKind::Partition) {
                        self.consume(TokenKind::By);
                        window.partition_clause = self.parse_expr_list_until(&[
                            TokenKind::Order,
                            TokenKind::Rows,
                            TokenKind::Range,
                            TokenKind::Groups,
                            TokenKind::Char(')'),
                        ]);
                    } else if self.consume(TokenKind::Order) {
                        self.consume(TokenKind::By);
                        window.order_clause = self.parse_sort_list_until(&[
                            TokenKind::Rows,
                            TokenKind::Range,
                            TokenKind::Groups,
                            TokenKind::Char(')'),
                        ]);
                    } else if matches!(
                        self.peek_kind(),
                        TokenKind::Rows | TokenKind::Range | TokenKind::Groups
                    ) {
                        self.skip_until_top_level(&[TokenKind::Char(')')]);
                    } else if window.refname.is_none() {
                        window.refname = self.consume_name();
                    } else {
                        self.advance();
                    }
                }
                self.consume(TokenKind::Char(')'));
            }
            windows.push(Node::WindowDef(window));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        windows
    }

    fn parse_locking_clause_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut clauses = Vec::new();
        while self.consume(TokenKind::For) {
            let strength = if self.consume(TokenKind::Update) {
                LockClauseStrength::Forupdate
            } else if self.consume(TokenKind::No) {
                self.consume(TokenKind::Key);
                self.consume(TokenKind::Update);
                LockClauseStrength::Fornokeyupdate
            } else if self.consume(TokenKind::Share) {
                LockClauseStrength::Forshare
            } else if self.consume(TokenKind::Key) {
                self.consume(TokenKind::Share);
                LockClauseStrength::Forkeyshare
            } else {
                LockClauseStrength::None
            };
            let locked_rels = if self.consume(TokenKind::Of) {
                self.parse_name_list_list_until(&[
                    TokenKind::Nowait,
                    TokenKind::Skip,
                    TokenKind::For,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])
            } else {
                Vec::new()
            };
            let wait_policy = if self.consume(TokenKind::Nowait) {
                LockWaitPolicy::Error
            } else if self.consume(TokenKind::Skip) {
                self.consume(TokenKind::Locked);
                LockWaitPolicy::Skip
            } else {
                LockWaitPolicy::Block
            };
            clauses.push(Node::LockingClause(LockingClause {
                node_tag: NodeTag::LockingClause,
                locked_rels,
                strength,
                wait_policy,
            }));
            if self.at_any(stops) || !self.at(TokenKind::For) {
                break;
            }
        }
        clauses
    }

    fn parse_insert_column_list(&mut self) -> NodeList {
        let mut cols = Vec::new();
        while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
            if let Some(name) = self.consume_name() {
                cols.push(Node::ResTarget(ResTarget {
                    node_tag: NodeTag::ResTarget,
                    name: Some(name),
                    ..ResTarget::default()
                }));
            } else {
                self.advance();
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        cols
    }

    fn parse_column_defs(&mut self) -> NodeList {
        let mut columns = Vec::new();
        while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
            let location = self.location();
            let chunk = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            if let Some(first) = chunk.first().and_then(token_name) {
                if is_table_constraint_name(&first) {
                    columns.push(Node::Constraint(Constraint {
                        node_tag: NodeTag::Constraint,
                        location: location as ParseLoc,
                        ..Constraint::default()
                    }));
                } else {
                    let type_name = chunk
                        .get(1..)
                        .and_then(|rest| tokens_to_type_name(rest.to_vec()));
                    columns.push(Node::ColumnDef(ColumnDef {
                        node_tag: NodeTag::ColumnDef,
                        colname: Some(first),
                        type_name: type_name.map(Box::new),
                        is_local: true,
                        location: location as ParseLoc,
                        ..ColumnDef::default()
                    }));
                }
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.consume(TokenKind::Char(')'));
        columns
    }

    fn parse_type_name(&mut self) -> TypeName {
        let location = self.location();
        let names = self.parse_name_list();
        TypeName {
            node_tag: NodeTag::TypeName,
            names,
            location: location as ParseLoc,
            ..TypeName::default()
        }
    }

    fn parse_type_name_until(&mut self, stops: &[TokenKind]) -> Option<TypeName> {
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        tokens_to_type_name(tokens).map(|mut type_name| {
            type_name.location = location as ParseLoc;
            type_name
        })
    }

    fn parse_object_with_args_until(&mut self, stops: &[TokenKind]) -> Option<ObjectWithArgs> {
        let tokens = self.take_until_top_level(stops);
        tokens_to_object_with_args(tokens)
    }

    fn parse_def_elem_list(&mut self) -> NodeList {
        if !self.consume(TokenKind::Char('(')) {
            return Vec::new();
        }
        let mut defs = Vec::new();
        while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
            let location = self.location();
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            if let Some(def) = tokens_to_def_elem(tokens, location) {
                defs.push(Node::DefElem(def));
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.consume(TokenKind::Char(')'));
        defs
    }

    fn parse_options_clause(&mut self) -> NodeList {
        if self.consume(TokenKind::Options) {
            self.parse_def_elem_list()
        } else {
            Vec::new()
        }
    }

    fn parse_alter_actions_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut actions = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Set => {
                    self.advance();
                    let defs = self.parse_def_elem_list();
                    if defs.is_empty() {
                        let name = self.consume_name().unwrap_or_else(|| "set".to_owned());
                        let arg = self.parse_expr_box_until(stops);
                        actions.push(Node::DefElem(DefElem {
                            node_tag: NodeTag::DefElem,
                            defname: Some(name),
                            arg,
                            defaction: DefElemAction::Set,
                            location: location as ParseLoc,
                            ..DefElem::default()
                        }));
                    } else {
                        actions.extend(defs);
                    }
                }
                TokenKind::Reset => {
                    self.advance();
                    let name = self.consume_name().unwrap_or_else(|| "reset".to_owned());
                    actions.push(Node::DefElem(DefElem {
                        node_tag: NodeTag::DefElem,
                        defname: Some(name),
                        defaction: DefElemAction::Drop,
                        location: location as ParseLoc,
                        ..DefElem::default()
                    }));
                }
                _ => {
                    let token = self.advance().clone();
                    if let Some(name) = token_name(&token) {
                        actions.push(Node::DefElem(DefElem {
                            node_tag: NodeTag::DefElem,
                            defname: Some(name),
                            location: location as ParseLoc,
                            ..DefElem::default()
                        }));
                    }
                }
            }
        }
        actions
    }

    fn parse_alter_table_cmds(&mut self) -> NodeList {
        let mut cmds = Vec::new();
        while !self.at_statement_end() {
            let cmd = self.parse_alter_table_cmd();
            if let Some(cmd) = cmd {
                cmds.push(Node::AlterTableCmd(cmd));
            } else {
                self.skip_until_top_level(&[
                    TokenKind::Char(','),
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        cmds
    }

    fn parse_alter_table_cmd(&mut self) -> Option<AlterTableCmd> {
        let mut cmd = AlterTableCmd {
            node_tag: NodeTag::AlterTableCmd,
            ..AlterTableCmd::default()
        };
        match self.peek_kind() {
            TokenKind::AddP => {
                self.advance();
                if self.consume(TokenKind::Column) {
                    cmd.subtype = AlterTableType::AddColumn;
                    cmd.def = self.parse_column_def_until(&[
                        TokenKind::Cascade,
                        TokenKind::Restrict,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                } else if self.consume(TokenKind::Constraint) {
                    cmd.subtype = AlterTableType::AddConstraint;
                    cmd.def = Some(Box::new(Node::Constraint(Constraint {
                        node_tag: NodeTag::Constraint,
                        location: self.location() as ParseLoc,
                        ..Constraint::default()
                    })));
                    self.skip_until_top_level(&[
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                } else {
                    cmd.subtype = AlterTableType::AddColumn;
                    cmd.def = self.parse_column_def_until(&[
                        TokenKind::Cascade,
                        TokenKind::Restrict,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                }
                cmd.behavior = self.parse_drop_behavior();
            }
            TokenKind::Drop => {
                self.advance();
                if self.consume(TokenKind::Column) {
                    cmd.subtype = AlterTableType::DropColumn;
                    cmd.missing_ok = self.consume_if_exists();
                    cmd.name = self.consume_name();
                } else if self.consume(TokenKind::Constraint) {
                    cmd.subtype = AlterTableType::DropConstraint;
                    cmd.missing_ok = self.consume_if_exists();
                    cmd.name = self.consume_name();
                } else {
                    cmd.subtype = AlterTableType::DropColumn;
                    cmd.missing_ok = self.consume_if_exists();
                    cmd.name = self.consume_name();
                }
                cmd.behavior = self.parse_drop_behavior();
            }
            TokenKind::Alter => {
                self.advance();
                self.consume(TokenKind::Column);
                cmd.name = self.consume_name();
                if self.consume(TokenKind::TypeP)
                    || (self.consume(TokenKind::Set)
                        && self.consume(TokenKind::DataP)
                        && self.consume(TokenKind::TypeP))
                {
                    cmd.subtype = AlterTableType::AlterColumnType;
                    let type_name = self
                        .parse_type_name_until(&[
                            TokenKind::Using,
                            TokenKind::Cascade,
                            TokenKind::Restrict,
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ])
                        .map(Box::new);
                    cmd.def = Some(Box::new(Node::ColumnDef(ColumnDef {
                        node_tag: NodeTag::ColumnDef,
                        colname: cmd.name.clone(),
                        type_name,
                        ..ColumnDef::default()
                    })));
                } else if self.consume(TokenKind::Set) {
                    if self.consume(TokenKind::Default) {
                        cmd.subtype = AlterTableType::ColumnDefault;
                        cmd.def = self.parse_expr_box_until(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ]);
                    } else if self.consume(TokenKind::Not) {
                        self.consume(TokenKind::NullP);
                        cmd.subtype = AlterTableType::SetNotNull;
                    } else if self.consume(TokenKind::Statistics) {
                        cmd.subtype = AlterTableType::SetStatistics;
                        cmd.def = self.parse_expr_box_until(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ]);
                    } else {
                        cmd.subtype = AlterTableType::SetOptions;
                        cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements: self.parse_def_elem_list(),
                            ..AArrayExpr::default()
                        })));
                    }
                } else if self.consume(TokenKind::Drop) {
                    if self.consume(TokenKind::Default) {
                        cmd.subtype = AlterTableType::ColumnDefault;
                    } else if self.consume(TokenKind::Not) {
                        self.consume(TokenKind::NullP);
                        cmd.subtype = AlterTableType::DropNotNull;
                    } else if self.consume(TokenKind::Expression) {
                        cmd.subtype = AlterTableType::DropExpression;
                    }
                }
            }
            TokenKind::Set => {
                self.advance();
                if self.consume(TokenKind::Tablespace) {
                    cmd.subtype = AlterTableType::SetTableSpace;
                    cmd.name = self.consume_name();
                } else if self.consume(TokenKind::Logged) {
                    cmd.subtype = AlterTableType::SetLogged;
                } else if self.consume(TokenKind::Unlogged) {
                    cmd.subtype = AlterTableType::SetUnLogged;
                } else if self.consume(TokenKind::Access) {
                    self.consume(TokenKind::Method);
                    cmd.subtype = AlterTableType::SetAccessMethod;
                    cmd.name = self.consume_name();
                } else {
                    cmd.subtype = AlterTableType::SetRelOptions;
                    cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements: self.parse_def_elem_list(),
                        ..AArrayExpr::default()
                    })));
                }
            }
            TokenKind::Reset => {
                self.advance();
                cmd.subtype = AlterTableType::ResetRelOptions;
                cmd.def = Some(Box::new(Node::AArrayExpr(AArrayExpr {
                    node_tag: NodeTag::AArrayExpr,
                    elements: self.parse_def_elem_list(),
                    ..AArrayExpr::default()
                })));
            }
            TokenKind::Validate => {
                self.advance();
                self.consume(TokenKind::Constraint);
                cmd.subtype = AlterTableType::ValidateConstraint;
                cmd.name = self.consume_name();
            }
            TokenKind::EnableP => {
                self.advance();
                cmd.subtype = if self.consume(TokenKind::Rule) {
                    AlterTableType::EnableRule
                } else {
                    self.consume(TokenKind::Trigger);
                    AlterTableType::EnableTrig
                };
                cmd.name = self.consume_name();
            }
            TokenKind::DisableP => {
                self.advance();
                cmd.subtype = if self.consume(TokenKind::Rule) {
                    AlterTableType::DisableRule
                } else {
                    self.consume(TokenKind::Trigger);
                    AlterTableType::DisableTrig
                };
                cmd.name = self.consume_name();
            }
            TokenKind::Cluster => {
                self.advance();
                self.consume(TokenKind::On);
                cmd.subtype = AlterTableType::ClusterOn;
                cmd.name = self.consume_name();
            }
            _ => return None,
        }
        self.skip_until_top_level(&[TokenKind::Char(','), TokenKind::Char(';'), TokenKind::Eof]);
        Some(cmd)
    }

    fn parse_column_def_until(&mut self, stops: &[TokenKind]) -> Option<Box<Node>> {
        let location = self.location();
        let colname = self.consume_name();
        let type_name = self.parse_type_name_until(stops).map(Box::new);
        colname.map(|colname| {
            Box::new(Node::ColumnDef(ColumnDef {
                node_tag: NodeTag::ColumnDef,
                colname: Some(colname),
                type_name,
                is_local: true,
                location: location as ParseLoc,
                ..ColumnDef::default()
            }))
        })
    }

    fn parse_opclass_item_list(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
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
                _ => {
                    self.advance();
                    continue;
                }
            };
            let number = if itemtype != 3 && self.peek_kind() == TokenKind::IConst {
                match self.advance().value {
                    Some(TokenValue::Integer(value)) => value,
                    _ => 0,
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
                item.storedtype = self
                    .parse_type_name_until(&[
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])
                    .map(Box::new);
            } else {
                item.name = self
                    .parse_object_with_args_until(&[
                        TokenKind::For,
                        TokenKind::Char(','),
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ])
                    .map(Box::new);
                if self.consume(TokenKind::For) {
                    if self.consume(TokenKind::Order) {
                        self.consume(TokenKind::By);
                        item.order_family = self.parse_name_list_until_keywords(&[
                            TokenKind::Char(','),
                            TokenKind::Char(';'),
                            TokenKind::Eof,
                        ]);
                    } else {
                        self.consume(TokenKind::Search);
                    }
                }
            }
            if item.name.is_some() || item.storedtype.is_some() || location == self.location() {
                items.push(Node::CreateOpClassItem(item));
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn parse_prop_graph_vertex_list(&mut self) -> NodeList {
        if !self.consume(TokenKind::Char('(')) {
            return Vec::new();
        }
        let mut vertices = Vec::new();
        while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
            let location = self.location();
            if let Some(vtable) = self.try_parse_qualified_range_var() {
                let vkey = self.parse_optional_key_clause();
                let labels = self.parse_prop_graph_labels();
                vertices.push(Node::PropGraphVertex(PropGraphVertex {
                    node_tag: NodeTag::PropGraphVertex,
                    vtable: Some(Box::new(vtable)),
                    vkey,
                    labels,
                    location: location as ParseLoc,
                }));
            } else {
                self.advance();
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.consume(TokenKind::Char(')'));
        vertices
    }

    fn parse_prop_graph_edge_list(&mut self) -> NodeList {
        if !self.consume(TokenKind::Char('(')) {
            return Vec::new();
        }
        let mut edges = Vec::new();
        while !self.at(TokenKind::Char(')')) && !self.at(TokenKind::Eof) {
            let location = self.location();
            if let Some(etable) = self.try_parse_qualified_range_var() {
                let ekey = self.parse_optional_key_clause();
                let mut edge = PropGraphEdge {
                    node_tag: NodeTag::PropGraphEdge,
                    etable: Some(Box::new(etable)),
                    ekey,
                    location: location as ParseLoc,
                    ..PropGraphEdge::default()
                };
                while !self.at_any(&[TokenKind::Char(','), TokenKind::Char(')'), TokenKind::Eof]) {
                    if self.consume(TokenKind::Source) {
                        edge.esrcvertex = self.consume_name();
                    } else if self.consume(TokenKind::Destination) {
                        edge.edestvertex = self.consume_name();
                    } else if matches!(
                        self.peek_kind(),
                        TokenKind::Label
                            | TokenKind::Default
                            | TokenKind::Properties
                            | TokenKind::No
                    ) {
                        edge.labels = self.parse_prop_graph_labels();
                    } else {
                        self.advance();
                    }
                }
                edges.push(Node::PropGraphEdge(edge));
            } else {
                self.advance();
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        self.consume(TokenKind::Char(')'));
        edges
    }

    fn parse_optional_key_clause(&mut self) -> NodeList {
        if self.consume(TokenKind::Key) && self.consume(TokenKind::Char('(')) {
            let cols = self.parse_expr_list_until(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            cols
        } else {
            Vec::new()
        }
    }

    fn parse_prop_graph_labels(&mut self) -> NodeList {
        let mut labels = Vec::new();
        while matches!(
            self.peek_kind(),
            TokenKind::Label | TokenKind::Default | TokenKind::Properties | TokenKind::No
        ) {
            let location = self.location();
            let mut label = None;
            if self.consume(TokenKind::Label) {
                label = self.consume_name();
            } else if self.consume(TokenKind::Default) {
                self.consume(TokenKind::Label);
            }
            let properties = if self.consume(TokenKind::No) {
                self.consume(TokenKind::Properties);
                Some(Box::new(PropGraphProperties {
                    node_tag: NodeTag::PropGraphProperties,
                    location: location as ParseLoc,
                    ..PropGraphProperties::default()
                }))
            } else if self.consume(TokenKind::Properties) {
                let all = self.consume(TokenKind::All);
                self.consume(TokenKind::Columns);
                let properties = if self.consume(TokenKind::Char('(')) {
                    let props = self.parse_res_target_list_until(&[TokenKind::Char(')')]);
                    self.consume(TokenKind::Char(')'));
                    props
                } else {
                    Vec::new()
                };
                Some(Box::new(PropGraphProperties {
                    node_tag: NodeTag::PropGraphProperties,
                    properties,
                    all,
                    location: location as ParseLoc,
                }))
            } else {
                Some(Box::new(PropGraphProperties {
                    node_tag: NodeTag::PropGraphProperties,
                    all: true,
                    location: location as ParseLoc,
                    ..PropGraphProperties::default()
                }))
            };
            labels.push(Node::PropGraphLabelAndProperties(
                PropGraphLabelAndProperties {
                    node_tag: NodeTag::PropGraphLabelAndProperties,
                    label,
                    properties,
                    location: location as ParseLoc,
                },
            ));
        }
        if labels.is_empty() {
            labels.push(Node::PropGraphLabelAndProperties(
                PropGraphLabelAndProperties {
                    node_tag: NodeTag::PropGraphLabelAndProperties,
                    properties: Some(Box::new(PropGraphProperties {
                        node_tag: NodeTag::PropGraphProperties,
                        all: true,
                        ..PropGraphProperties::default()
                    })),
                    ..PropGraphLabelAndProperties::default()
                },
            ));
        }
        labels
    }

    fn parse_func_call(&mut self) -> Option<FuncCall> {
        let location = self.location();
        let funcname = self.parse_name_list();
        if funcname.is_empty() {
            return None;
        }
        let args = if self.consume(TokenKind::Char('(')) {
            if self.consume(TokenKind::Char('*')) {
                self.consume(TokenKind::Char(')'));
                vec![Node::AStar(AStar {
                    node_tag: NodeTag::AStar,
                })]
            } else {
                let args = self.parse_expr_list_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                args
            }
        } else {
            Vec::new()
        };
        Some(FuncCall {
            node_tag: NodeTag::FuncCall,
            funcname,
            args,
            location: location as ParseLoc,
            ..FuncCall::default()
        })
    }

    fn parse_name_list(&mut self) -> NodeList {
        self.consume_name_parts()
            .into_iter()
            .map(make_string_node)
            .collect()
    }

    fn parse_name_list_until_keywords(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut names = Vec::new();
        while !self.at_any(stops) {
            if self.at(TokenKind::Char('.')) {
                self.advance();
                continue;
            }
            if let Some(name) = self.consume_name() {
                names.push(make_string_node(name));
            } else {
                break;
            }
        }
        names
    }

    fn parse_name_list_list_until(&mut self, stops: &[TokenKind]) -> NodeList {
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let parts = self.consume_name_parts();
            if parts.is_empty() {
                self.advance();
            } else {
                objects.push(Node::AArrayExpr(AArrayExpr {
                    node_tag: NodeTag::AArrayExpr,
                    elements: parts.into_iter().map(make_string_node).collect(),
                    ..AArrayExpr::default()
                }));
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        objects
    }

    fn try_parse_range_var(&mut self) -> Option<RangeVar> {
        let location = self.location();
        let parts = self.consume_name_parts();
        if parts.is_empty() {
            return None;
        }
        let mut range = range_var_from_parts(parts, location);
        if self.consume(TokenKind::As) {
            range.alias = self.consume_name().map(|aliasname| {
                Box::new(Alias {
                    node_tag: NodeTag::Alias,
                    aliasname: Some(aliasname),
                    ..Alias::default()
                })
            });
        }
        Some(range)
    }

    fn parse_optional_alias(&mut self) -> Option<Box<Alias>> {
        let has_as = self.consume(TokenKind::As);
        if has_as || matches!(self.peek_kind(), TokenKind::Ident | TokenKind::UIdent) {
            self.consume_name().map(|aliasname| {
                Box::new(Alias {
                    node_tag: NodeTag::Alias,
                    aliasname: Some(aliasname),
                    ..Alias::default()
                })
            })
        } else {
            None
        }
    }

    fn try_parse_qualified_range_var(&mut self) -> Option<RangeVar> {
        let location = self.location();
        let parts = self.consume_name_parts();
        if parts.is_empty() {
            None
        } else {
            Some(range_var_from_parts(parts, location))
        }
    }

    fn consume_name_parts(&mut self) -> Vec<std::string::String> {
        let mut parts = Vec::new();
        let Some(first) = self.consume_name() else {
            return parts;
        };
        parts.push(first);
        while self.consume(TokenKind::Char('.')) {
            if self.at(TokenKind::Char('*')) {
                break;
            }
            if let Some(name) = self.consume_name() {
                parts.push(name);
            } else {
                break;
            }
        }
        parts
    }

    fn consume_object_type(&mut self) -> Option<ObjectType> {
        let ty = match self.peek_kind() {
            TokenKind::Access if self.peek_kind_n(1) == TokenKind::Method => {
                self.advance();
                self.advance();
                ObjectType::AccessMethod
            }
            TokenKind::Aggregate => ObjectType::Aggregate,
            TokenKind::Table => ObjectType::Table,
            TokenKind::Sequence => ObjectType::Sequence,
            TokenKind::View => ObjectType::View,
            TokenKind::Index => ObjectType::Index,
            TokenKind::Schema => ObjectType::Schema,
            TokenKind::Database => ObjectType::Database,
            TokenKind::TypeP => ObjectType::Type,
            TokenKind::DomainP => ObjectType::Domain,
            TokenKind::Extension => ObjectType::Extension,
            TokenKind::Function => ObjectType::Function,
            TokenKind::Procedure => ObjectType::Procedure,
            TokenKind::Routine => ObjectType::Routine,
            TokenKind::Operator => ObjectType::Operator,
            TokenKind::Language => ObjectType::Language,
            TokenKind::Collation => ObjectType::Collation,
            TokenKind::ConversionP => ObjectType::Conversion,
            TokenKind::Policy => ObjectType::Policy,
            TokenKind::Publication => ObjectType::Publication,
            TokenKind::Subscription => ObjectType::Subscription,
            TokenKind::Server => ObjectType::ForeignServer,
            TokenKind::Cast => ObjectType::Cast,
            TokenKind::Transform => ObjectType::Transform,
            TokenKind::Trigger => ObjectType::Trigger,
            TokenKind::Rule => ObjectType::Rule,
            TokenKind::Tablespace => ObjectType::Tablespace,
            TokenKind::Statistics => ObjectType::StatisticExt,
            TokenKind::Foreign => {
                self.advance();
                if self.consume(TokenKind::Table) {
                    ObjectType::ForeignTable
                } else if self.consume(TokenKind::DataP) {
                    self.consume(TokenKind::Wrapper);
                    ObjectType::Fdw
                } else {
                    ObjectType::Fdw
                }
            }
            TokenKind::Materialized => {
                self.advance();
                self.consume(TokenKind::View);
                ObjectType::Matview
            }
            _ => return None,
        };
        if !matches!(
            ty,
            ObjectType::AccessMethod
                | ObjectType::ForeignTable
                | ObjectType::Fdw
                | ObjectType::Matview
        ) {
            self.advance();
        }
        Some(ty)
    }

    fn consume_alter_object_type(&mut self) -> ObjectType {
        match self.peek_kind() {
            TokenKind::Access if self.peek_kind_n(1) == TokenKind::Method => {
                self.advance();
                self.advance();
                ObjectType::AccessMethod
            }
            TokenKind::Aggregate => {
                self.advance();
                ObjectType::Aggregate
            }
            TokenKind::Collation => {
                self.advance();
                ObjectType::Collation
            }
            TokenKind::ConversionP => {
                self.advance();
                ObjectType::Conversion
            }
            TokenKind::Database => {
                self.advance();
                ObjectType::Database
            }
            TokenKind::DomainP => {
                self.advance();
                ObjectType::Domain
            }
            TokenKind::Extension => {
                self.advance();
                ObjectType::Extension
            }
            TokenKind::Function => {
                self.advance();
                ObjectType::Function
            }
            TokenKind::Procedure => {
                self.advance();
                ObjectType::Procedure
            }
            TokenKind::Routine => {
                self.advance();
                ObjectType::Routine
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                self.advance();
                self.advance();
                ObjectType::Opclass
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.advance();
                ObjectType::Opfamily
            }
            TokenKind::Operator => {
                self.advance();
                ObjectType::Operator
            }
            TokenKind::Policy => {
                self.advance();
                ObjectType::Policy
            }
            TokenKind::Publication => {
                self.advance();
                ObjectType::Publication
            }
            TokenKind::Schema => {
                self.advance();
                ObjectType::Schema
            }
            TokenKind::Server => {
                self.advance();
                ObjectType::ForeignServer
            }
            TokenKind::Subscription => {
                self.advance();
                ObjectType::Subscription
            }
            TokenKind::Table => {
                self.advance();
                ObjectType::Table
            }
            TokenKind::Sequence => {
                self.advance();
                ObjectType::Sequence
            }
            TokenKind::View => {
                self.advance();
                ObjectType::View
            }
            TokenKind::Materialized if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.advance();
                ObjectType::Matview
            }
            TokenKind::Index => {
                self.advance();
                ObjectType::Index
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Table => {
                self.advance();
                self.advance();
                ObjectType::ForeignTable
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.advance();
                self.consume(TokenKind::Wrapper);
                ObjectType::Fdw
            }
            TokenKind::Trigger => {
                self.advance();
                ObjectType::Trigger
            }
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.advance();
                ObjectType::EventTrigger
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => {
                self.advance();
                ObjectType::Role
            }
            TokenKind::Tablespace => {
                self.advance();
                ObjectType::Tablespace
            }
            TokenKind::Statistics => {
                self.advance();
                ObjectType::StatisticExt
            }
            TokenKind::TextP if self.peek_kind_n(1) == TokenKind::Search => {
                self.advance();
                self.advance();
                match self.advance().kind {
                    TokenKind::Parser => ObjectType::Tsparser,
                    TokenKind::Dictionary => ObjectType::Tsdictionary,
                    TokenKind::Template => ObjectType::Tstemplate,
                    TokenKind::Configuration => ObjectType::Tsconfiguration,
                    _ => ObjectType::Default,
                }
            }
            TokenKind::TypeP => {
                self.advance();
                ObjectType::Type
            }
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.advance();
                ObjectType::Propgraph
            }
            TokenKind::Language | TokenKind::Procedural => {
                self.consume(TokenKind::Procedural);
                self.consume(TokenKind::Language);
                ObjectType::Language
            }
            _ => {
                self.advance();
                ObjectType::Default
            }
        }
    }

    fn consume_role_spec(&mut self) -> Option<RoleSpec> {
        let location = self.location();
        let roletype = match self.peek_kind() {
            TokenKind::CurrentRole => {
                self.advance();
                return Some(RoleSpec {
                    node_tag: NodeTag::RoleSpec,
                    roletype: RoleSpecType::CurrentRole,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            TokenKind::CurrentUser => {
                self.advance();
                return Some(RoleSpec {
                    node_tag: NodeTag::RoleSpec,
                    roletype: RoleSpecType::CurrentUser,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            TokenKind::SessionUser => {
                self.advance();
                return Some(RoleSpec {
                    node_tag: NodeTag::RoleSpec,
                    roletype: RoleSpecType::SessionUser,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            _ => RoleSpecType::Cstring,
        };
        self.consume_name().map(|rolename| {
            let roletype = if rolename.eq_ignore_ascii_case("public") {
                RoleSpecType::Public
            } else {
                roletype
            };
            RoleSpec {
                node_tag: NodeTag::RoleSpec,
                roletype,
                rolename: Some(rolename),
                location: location as ParseLoc,
            }
        })
    }

    fn looks_like_alter_enum(&self) -> bool {
        if self.peek_kind() != TokenKind::TypeP {
            return false;
        }
        self.top_level_adjacent(TokenKind::AddP, TokenKind::ValueP)
            || self.top_level_adjacent(TokenKind::Rename, TokenKind::ValueP)
            || self.top_level_adjacent(TokenKind::Drop, TokenKind::ValueP)
    }

    fn looks_like_rename_stmt(&self) -> bool {
        if self.peek_kind() == TokenKind::TypeP
            && self.top_level_adjacent(TokenKind::Rename, TokenKind::ValueP)
        {
            return false;
        }
        self.top_level_contains(TokenKind::Rename)
    }

    fn looks_like_alter_object_depends_stmt(&self) -> bool {
        self.top_level_contains(TokenKind::Depends)
    }

    fn looks_like_alter_object_schema_stmt(&self) -> bool {
        self.top_level_adjacent(TokenKind::Set, TokenKind::Schema)
    }

    fn looks_like_alter_owner_stmt(&self) -> bool {
        self.top_level_adjacent(TokenKind::Owner, TokenKind::To)
    }

    fn looks_like_alter_composite_type(&self) -> bool {
        self.peek_kind() == TokenKind::TypeP
            && self.top_level_contains(TokenKind::Attribute)
            && (self.top_level_contains(TokenKind::AddP)
                || self.top_level_contains(TokenKind::Drop)
                || self.top_level_contains(TokenKind::Alter))
    }

    fn top_level_contains(&self, needle: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .any(|kind| kind == needle)
    }

    fn top_level_adjacent(&self, first: TokenKind, second: TokenKind) -> bool {
        self.top_level_kinds()
            .windows(2)
            .any(|pair| pair == [first, second])
    }

    fn top_level_kinds(&self) -> Vec<TokenKind> {
        let mut kinds = Vec::new();
        let mut depth = 0usize;
        let mut i = self.pos;
        while let Some(token) = self.tokens.get(i) {
            let kind = token.kind;
            if kind == TokenKind::Eof || (depth == 0 && kind == TokenKind::Char(';')) {
                break;
            }
            if depth == 0 {
                kinds.push(kind);
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        kinds
    }

    fn consume_if_exists(&mut self) -> bool {
        if self.consume(TokenKind::IfP) {
            self.consume(TokenKind::Exists);
            true
        } else {
            false
        }
    }

    fn consume_if_not_exists(&mut self) -> bool {
        if self.consume(TokenKind::IfP) {
            self.consume(TokenKind::Not);
            self.consume(TokenKind::Exists);
            true
        } else {
            false
        }
    }

    fn parse_drop_behavior(&mut self) -> DropBehavior {
        if self.consume(TokenKind::Cascade) {
            DropBehavior::Cascade
        } else {
            self.consume(TokenKind::Restrict);
            DropBehavior::Restrict
        }
    }

    fn consume_setting_name(&mut self) -> Option<std::string::String> {
        let mut parts = Vec::new();
        while !self.at_any(&[
            TokenKind::To,
            TokenKind::Char('='),
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]) {
            if let Some(name) = self.consume_name() {
                parts.push(name);
                if !self.consume(TokenKind::Char('.')) {
                    break;
                }
            } else {
                break;
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("."))
        }
    }

    fn consume_name(&mut self) -> Option<std::string::String> {
        let token = self.peek().clone();
        let name = token_name(&token)?;
        if matches!(
            token.kind,
            TokenKind::Ident
                | TokenKind::UIdent
                | TokenKind::SConst
                | TokenKind::FConst
                | TokenKind::Op
        ) || matches!(token.value, Some(TokenValue::Keyword(_)))
        {
            self.advance();
            Some(name)
        } else {
            None
        }
    }

    fn consume_string_like(&mut self) -> Option<std::string::String> {
        match self.peek().value.clone() {
            Some(TokenValue::String(value)) => {
                self.advance();
                Some(value)
            }
            Some(TokenValue::Keyword(value)) => {
                self.advance();
                Some(value.to_owned())
            }
            Some(TokenValue::Integer(value)) => {
                self.advance();
                Some(value.to_string())
            }
            None => None,
        }
    }

    fn parse_create_extension(&mut self) -> Node {
        self.consume(TokenKind::Extension);
        let if_not_exists = self.consume_if_not_exists();
        let extname = self.consume_name();
        let options = if self.consume(TokenKind::With) {
            self.parse_def_elem_list()
        } else {
            self.parse_options_clause()
        };
        self.skip_rest();
        Node::CreateExtensionStmt(CreateExtensionStmt {
            node_tag: NodeTag::CreateExtensionStmt,
            extname,
            if_not_exists,
            options,
        })
    }

    fn parse_create_publication(&mut self) -> Node {
        self.consume(TokenKind::Publication);
        let pubname = self.consume_name();
        let mut for_all_tables = false;
        let mut for_all_sequences = false;
        let mut pubobjects = Vec::new();
        if self.consume(TokenKind::For) {
            if self.consume(TokenKind::All) {
                if self.consume(TokenKind::Tables) {
                    for_all_tables = true;
                } else if self.consume(TokenKind::Sequences) {
                    for_all_sequences = true;
                }
            } else if self.consume(TokenKind::Table) || self.consume(TokenKind::Tables) {
                pubobjects = self.parse_from_clause_until(&[
                    TokenKind::With,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
            }
        }
        let options = if self.consume(TokenKind::With) {
            self.parse_def_elem_list()
        } else {
            Vec::new()
        };
        self.skip_rest();
        Node::CreatePublicationStmt(CreatePublicationStmt {
            node_tag: NodeTag::CreatePublicationStmt,
            pubname,
            options,
            pubobjects,
            for_all_tables,
            for_all_sequences,
        })
    }

    fn parse_create_subscription(&mut self) -> Node {
        self.consume(TokenKind::Subscription);
        let subname = self.consume_name();
        self.consume(TokenKind::Connection);
        let conninfo = self.consume_string_like();
        self.consume(TokenKind::Publication);
        let publication =
            self.parse_expr_list_until(&[TokenKind::With, TokenKind::Char(';'), TokenKind::Eof]);
        let options = if self.consume(TokenKind::With) {
            self.parse_def_elem_list()
        } else {
            Vec::new()
        };
        self.skip_rest();
        Node::CreateSubscriptionStmt(CreateSubscriptionStmt {
            node_tag: NodeTag::CreateSubscriptionStmt,
            subname,
            conninfo,
            publication,
            options,
            ..CreateSubscriptionStmt::default()
        })
    }

    fn parse_create_policy(&mut self) -> Node {
        self.consume(TokenKind::Policy);
        let policy_name = self.consume_name();
        self.consume(TokenKind::On);
        let table = self.try_parse_qualified_range_var().map(Box::new);
        let cmd_name = if self.consume(TokenKind::For) {
            self.consume_name()
        } else {
            None
        };
        let permissive = if self.consume(TokenKind::As) {
            self.consume_name()
                .is_none_or(|name| !name.eq_ignore_ascii_case("restrictive"))
        } else {
            true
        };
        let roles = if self.consume(TokenKind::To) {
            self.parse_name_list_list_until(&[
                TokenKind::Using,
                TokenKind::With,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ])
        } else {
            Vec::new()
        };
        let qual = if self.consume(TokenKind::Using) {
            if self.consume(TokenKind::Char('(')) {
                let expr = self.parse_expr_box_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                expr
            } else {
                self.parse_expr_box_until(&[TokenKind::With, TokenKind::Char(';'), TokenKind::Eof])
            }
        } else {
            None
        };
        let with_check = if self.consume(TokenKind::With) {
            self.consume(TokenKind::Check);
            if self.consume(TokenKind::Char('(')) {
                let expr = self.parse_expr_box_until(&[TokenKind::Char(')')]);
                self.consume(TokenKind::Char(')'));
                expr
            } else {
                self.parse_expr_box_until(&[TokenKind::Char(';'), TokenKind::Eof])
            }
        } else {
            None
        };
        self.skip_rest();
        Node::CreatePolicyStmt(CreatePolicyStmt {
            node_tag: NodeTag::CreatePolicyStmt,
            policy_name,
            table,
            cmd_name,
            permissive,
            roles,
            qual,
            with_check,
        })
    }

    fn parse_create_trigger(&mut self, replace: bool) -> Node {
        self.consume(TokenKind::Trigger);
        let trigname = self.consume_name();
        let mut timing = 0;
        let mut events = 0;
        if self.consume(TokenKind::Before) {
            timing = 1;
        } else if self.consume(TokenKind::After) {
            timing = 2;
        } else if self.consume(TokenKind::Instead) {
            timing = 3;
            self.consume(TokenKind::Of);
        }
        loop {
            if self.consume(TokenKind::Insert) {
                events |= 1;
            } else if self.consume(TokenKind::DeleteP) {
                events |= 2;
            } else if self.consume(TokenKind::Update) {
                events |= 4;
                if self.consume(TokenKind::Of) {
                    self.skip_until_top_level(&[
                        TokenKind::On,
                        TokenKind::Or,
                        TokenKind::Char(';'),
                        TokenKind::Eof,
                    ]);
                }
            } else if self.consume(TokenKind::Truncate) {
                events |= 8;
            } else {
                break;
            }
            if !self.consume(TokenKind::Or) {
                break;
            }
        }
        self.skip_until_top_level(&[
            TokenKind::On,
            TokenKind::Execute,
            TokenKind::When,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let relation = if self.consume(TokenKind::On) {
            self.try_parse_qualified_range_var().map(Box::new)
        } else {
            None
        };
        let row = if self.consume(TokenKind::For) {
            self.consume(TokenKind::Each);
            if self.consume(TokenKind::Row) {
                true
            } else {
                self.consume(TokenKind::Statement);
                false
            }
        } else {
            false
        };
        let when_clause = if self.consume(TokenKind::When) && self.consume(TokenKind::Char('(')) {
            let expr = self.parse_expr_box_until(&[TokenKind::Char(')')]);
            self.consume(TokenKind::Char(')'));
            expr
        } else {
            None
        };
        self.skip_until_top_level(&[TokenKind::Execute, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Execute);
        let _ = self.consume(TokenKind::Function) || self.consume(TokenKind::Procedure);
        let func = self.parse_func_call();
        let (funcname, args) = func
            .map(|func| (func.funcname, func.args))
            .unwrap_or_else(|| (Vec::new(), Vec::new()));
        self.skip_rest();
        Node::CreateTrigStmt(CreateTrigStmt {
            node_tag: NodeTag::CreateTrigStmt,
            replace,
            trigname,
            relation,
            funcname,
            args,
            row,
            timing,
            events,
            when_clause,
            ..CreateTrigStmt::default()
        })
    }

    fn parse_create_event_trigger(&mut self) -> Node {
        self.consume(TokenKind::Trigger);
        let trigname = self.consume_name();
        self.consume(TokenKind::On);
        let eventname = self.consume_name();
        let mut whenclause = Vec::new();
        if self.consume(TokenKind::When) {
            whenclause = self.parse_def_elem_list();
        }
        self.skip_until_top_level(&[TokenKind::Execute, TokenKind::Char(';'), TokenKind::Eof]);
        self.consume(TokenKind::Execute);
        self.consume(TokenKind::Function);
        let funcname = self
            .parse_func_call()
            .map_or_else(Vec::new, |func| func.funcname);
        self.skip_rest();
        Node::CreateEventTrigStmt(CreateEventTrigStmt {
            node_tag: NodeTag::CreateEventTrigStmt,
            trigname,
            eventname,
            whenclause,
            funcname,
        })
    }

    fn parse_create_language(&mut self, replace: bool) -> Node {
        self.consume(TokenKind::Language);
        let pltrusted = self.consume(TokenKind::Trusted);
        let plname = self.consume_name();
        let mut plhandler = Vec::new();
        let mut plinline = Vec::new();
        let mut plvalidator = Vec::new();
        while !self.at_statement_end() {
            if self.consume(TokenKind::Handler) {
                plhandler = self.parse_name_list_until_keywords(&[
                    TokenKind::InlineP,
                    TokenKind::Validator,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
            } else if self.consume(TokenKind::InlineP) {
                plinline = self.parse_name_list_until_keywords(&[
                    TokenKind::Validator,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ]);
            } else if self.consume(TokenKind::Validator) {
                plvalidator =
                    self.parse_name_list_until_keywords(&[TokenKind::Char(';'), TokenKind::Eof]);
            } else {
                self.advance();
            }
        }
        self.skip_rest();
        Node::CreatePLangStmt(CreatePLangStmt {
            node_tag: NodeTag::CreatePLangStmt,
            replace,
            plname,
            plhandler,
            plinline,
            plvalidator,
            pltrusted,
        })
    }

    fn parse_create_server(&mut self) -> Node {
        self.consume(TokenKind::Server);
        let if_not_exists = self.consume_if_not_exists();
        let servername = self.consume_name();
        let mut servertype = None;
        let mut version = None;
        if self.consume(TokenKind::TypeP) {
            servertype = self.consume_string_like().or_else(|| self.consume_name());
        }
        if self.consume(TokenKind::VersionP) {
            version = self.consume_string_like().or_else(|| self.consume_name());
        }
        self.consume(TokenKind::Foreign);
        self.consume(TokenKind::DataP);
        self.consume(TokenKind::Wrapper);
        let fdwname = self.consume_name();
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::CreateForeignServerStmt(CreateForeignServerStmt {
            node_tag: NodeTag::CreateForeignServerStmt,
            servername,
            servertype,
            version,
            fdwname,
            if_not_exists,
            options,
        })
    }

    fn parse_create_user_mapping(&mut self) -> Node {
        self.consume(TokenKind::User);
        self.consume(TokenKind::Mapping);
        let if_not_exists = self.consume_if_not_exists();
        self.consume(TokenKind::For);
        let user = self.consume_role_spec().map(Box::new);
        self.skip_until_top_level(&[
            TokenKind::Server,
            TokenKind::Options,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        let servername = if self.consume(TokenKind::Server) {
            self.consume_name()
        } else {
            None
        };
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::CreateUserMappingStmt(CreateUserMappingStmt {
            node_tag: NodeTag::CreateUserMappingStmt,
            user,
            servername,
            if_not_exists,
            options,
        })
    }

    fn parse_create_tablespace(&mut self) -> Node {
        self.consume(TokenKind::Tablespace);
        let tablespacename = self.consume_name();
        let owner = if self.consume(TokenKind::Owner) {
            self.consume_role_spec().map(Box::new)
        } else {
            None
        };
        self.consume(TokenKind::Location);
        let location = self.consume_string_like();
        let options = self.parse_options_clause();
        self.skip_rest();
        Node::CreateTableSpaceStmt(CreateTableSpaceStmt {
            node_tag: NodeTag::CreateTableSpaceStmt,
            tablespacename,
            owner,
            location,
            options,
        })
    }

    fn parse_create_am(&mut self) -> Node {
        self.consume(TokenKind::Method);
        let amname = self.consume_name();
        self.consume(TokenKind::TypeP);
        let amtype = self
            .consume_name()
            .and_then(|name| name.bytes().next())
            .unwrap_or_default();
        self.consume(TokenKind::Handler);
        let handler_name =
            self.parse_name_list_until_keywords(&[TokenKind::Char(';'), TokenKind::Eof]);
        self.skip_rest();
        Node::CreateAmStmt(CreateAmStmt {
            node_tag: NodeTag::CreateAmStmt,
            amname,
            handler_name,
            amtype,
        })
    }

    fn skip_rest(&mut self) {
        self.skip_until_top_level(&[TokenKind::Char(';'), TokenKind::Eof]);
    }

    fn skip_until_top_level(&mut self, stops: &[TokenKind]) {
        let _ = self.take_until_top_level(stops);
    }

    fn take_until_top_level(&mut self, stops: &[TokenKind]) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            if depth == 0 && stops.contains(&kind) {
                break;
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 && stops.contains(&kind) {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            out.push(self.advance().clone());
        }
        out
    }

    fn at_statement_end(&self) -> bool {
        self.at(TokenKind::Char(';')) || self.at(TokenKind::Eof)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek_kind())
    }

    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at(kind) {
            Ok(self.advance().clone())
        } else {
            Err(self.error_here(format!("expected {:?}, found {:?}", kind, self.peek_kind())))
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.at(TokenKind::Eof) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn previous_kind(&self) -> TokenKind {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn location(&self) -> usize {
        self.peek().location
    }

    fn previous_location(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.location)
            .unwrap_or(self.location())
    }

    fn error_here(&self, message: impl Into<std::string::String>) -> ParseError {
        ParseError::new(self.location(), message)
    }
}

fn extend_stops(stops: &[TokenKind], extra: TokenKind) -> Vec<TokenKind> {
    let mut out = stops.to_vec();
    if !out.contains(&extra) {
        out.push(extra);
    }
    out
}

fn tokens_to_statement_list(mut tokens: Vec<Token>) -> NodeList {
    let location = tokens.last().map_or(0, |token| token.location);
    tokens.push(Token {
        kind: TokenKind::Eof,
        location,
        value: None,
    });
    let mut parser = Parser { tokens, pos: 0 };
    parser
        .parse()
        .map(|stmts| {
            stmts
                .into_iter()
                .filter_map(|stmt| stmt.stmt.map(|node| *node))
                .collect()
        })
        .unwrap_or_default()
}

fn tokens_to_statement_node(mut tokens: Vec<Token>) -> Option<Node> {
    let location = tokens.last().map_or(0, |token| token.location);
    tokens.push(Token {
        kind: TokenKind::Eof,
        location,
        value: None,
    });
    Parser { tokens, pos: 0 }.parse_statement(None).ok()
}

fn relation_object_type(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Table
            | ObjectType::Sequence
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::Index
            | ObjectType::ForeignTable
            | ObjectType::Propgraph
    )
}

fn make_aexpr<I, S>(
    kind: AExprKind,
    name: I,
    lexpr: Option<Node>,
    rexpr: Option<Node>,
    location: usize,
) -> Node
where
    I: IntoIterator<Item = S>,
    S: Into<std::string::String>,
{
    Node::AExpr(AExpr {
        node_tag: NodeTag::AExpr,
        kind,
        name: name.into_iter().map(make_string_node).collect(),
        lexpr: lexpr.map(Box::new),
        rexpr: rexpr.map(Box::new),
        location: location as ParseLoc,
        ..AExpr::default()
    })
}

fn make_bool_expr(kind: BoolExprType, lhs: Node, rhs: Node, location: usize) -> Node {
    Node::BoolExpr(BoolExpr {
        xpr: Expr::new(NodeTag::BoolExpr),
        boolop: kind,
        args: vec![lhs, rhs],
        location: location as ParseLoc,
    })
}

fn comparison_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('=') => Some("="),
        TokenKind::Char('<') => Some("<"),
        TokenKind::Char('>') => Some(">"),
        TokenKind::LessEquals => Some("<="),
        TokenKind::GreaterEquals => Some(">="),
        TokenKind::NotEquals => Some("<>"),
        _ => None,
    }
}

fn additive_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('+') => Some("+"),
        TokenKind::Char('-') => Some("-"),
        _ => None,
    }
}

fn multiplicative_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('*') => Some("*"),
        TokenKind::Char('/') => Some("/"),
        TokenKind::Char('%') => Some("%"),
        _ => None,
    }
}

fn token_kind_text(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::InP => "in",
        TokenKind::Like => "like",
        TokenKind::Ilike => "ilike",
        TokenKind::Similar => "similar",
        TokenKind::Between => "between",
        _ => "op",
    }
}

fn expression_boundary(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Eof
            | TokenKind::Char(',')
            | TokenKind::Char(')')
            | TokenKind::Char(']')
            | TokenKind::Char('+')
            | TokenKind::Char('-')
            | TokenKind::Char('*')
            | TokenKind::Char('/')
            | TokenKind::Char('%')
            | TokenKind::Char('=')
            | TokenKind::Char('<')
            | TokenKind::Char('>')
            | TokenKind::LessEquals
            | TokenKind::GreaterEquals
            | TokenKind::NotEquals
            | TokenKind::Op
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::InP
            | TokenKind::Is
            | TokenKind::Like
            | TokenKind::Ilike
            | TokenKind::Similar
            | TokenKind::Between
            | TokenKind::Not
    )
}

fn split_alias(tokens: Vec<Token>) -> (Option<std::string::String>, Vec<Token>) {
    if let Some(index) = tokens.iter().position(|token| token.kind == TokenKind::As)
        && let Some(name) = tokens.get(index + 1).and_then(token_name)
    {
        return (Some(name), tokens[..index].to_vec());
    }
    (None, tokens)
}

fn token_name(token: &Token) -> Option<std::string::String> {
    match &token.value {
        Some(TokenValue::String(value)) => Some(value.clone()),
        Some(TokenValue::Keyword(value)) => Some((*value).to_owned()),
        Some(TokenValue::Integer(value)) => Some(value.to_string()),
        None => match token.kind {
            TokenKind::Char('*') => Some("*".to_owned()),
            _ => None,
        },
    }
}

fn token_text(token: &Token) -> std::string::String {
    token_name(token).unwrap_or_else(|| match token.kind {
        TokenKind::Char(ch) => ch.to_string(),
        other => format!("{:?}", other).to_ascii_lowercase(),
    })
}

fn tokens_to_node(tokens: Vec<Token>) -> Option<Node> {
    if tokens.is_empty() {
        return None;
    }
    if let Some(node) = ExprParser::new(tokens.clone()).parse() {
        return Some(node);
    }
    if tokens.len() == 1 {
        return token_to_leaf(&tokens[0]);
    }
    if let Some(node) = tokens_to_func_call(&tokens) {
        return Some(node);
    }
    Some(Node::ColumnRef(ColumnRef {
        node_tag: NodeTag::ColumnRef,
        fields: vec![make_string_node(tokens_to_text(&tokens))],
        location: tokens[0].location as ParseLoc,
    }))
}

struct ExprParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl ExprParser {
    fn new(mut tokens: Vec<Token>) -> Self {
        let location = tokens.last().map_or(0, |token| token.location);
        tokens.push(Token {
            kind: TokenKind::Eof,
            location,
            value: None,
        });
        Self { tokens, pos: 0 }
    }

    fn parse(mut self) -> Option<Node> {
        let node = self.parse_expr(0)?;
        if !self.at(TokenKind::Eof) {
            return None;
        }
        Some(node)
    }

    fn parse_expr(&mut self, min_bp: u8) -> Option<Node> {
        let mut lhs = self.parse_prefix()?;

        loop {
            lhs = match self.peek_kind() {
                TokenKind::TypeCast => {
                    if 80 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    let type_name = self.parse_cast_type_name().map(Box::new);
                    Node::TypeCast(TypeCast {
                        node_tag: NodeTag::TypeCast,
                        arg: Some(Box::new(lhs)),
                        type_name,
                        location: location as ParseLoc,
                    })
                }
                TokenKind::Collate => {
                    if 80 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    let collname = self.parse_name_nodes();
                    Node::CollateClause(CollateClause {
                        node_tag: NodeTag::CollateClause,
                        arg: Some(Box::new(lhs)),
                        collname,
                        location: location as ParseLoc,
                    })
                }
                TokenKind::Isnull => {
                    if 70 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    make_aexpr(AExprKind::Op, vec!["isnull"], Some(lhs), None, location)
                }
                TokenKind::Notnull => {
                    if 70 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    make_aexpr(AExprKind::Op, vec!["notnull"], Some(lhs), None, location)
                }
                TokenKind::Or => {
                    if 10 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    let rhs = self.parse_expr(11)?;
                    make_bool_expr(BoolExprType::OrExpr, lhs, rhs, location)
                }
                TokenKind::And => {
                    if 20 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    let rhs = self.parse_expr(21)?;
                    make_bool_expr(BoolExprType::AndExpr, lhs, rhs, location)
                }
                TokenKind::Not
                    if matches!(
                        self.peek_kind_n(1),
                        TokenKind::InP
                            | TokenKind::Like
                            | TokenKind::Ilike
                            | TokenKind::Similar
                            | TokenKind::Between
                    ) =>
                {
                    if 30 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    let op = self.advance().kind;
                    self.parse_special_infix(lhs, op, true, location)?
                }
                TokenKind::InP | TokenKind::Like | TokenKind::Ilike | TokenKind::Similar => {
                    if 30 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    self.parse_special_infix(lhs, token.kind, false, token.location)?
                }
                TokenKind::Between => {
                    if 30 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    self.parse_between(lhs, false, location)?
                }
                TokenKind::Is => {
                    if 30 < min_bp {
                        break;
                    }
                    let location = self.advance().location;
                    self.parse_is_expr(lhs, location)?
                }
                kind if comparison_operator(kind).is_some() => {
                    if 35 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let rhs = self.parse_expr(36)?;
                    make_aexpr(
                        AExprKind::Op,
                        vec![comparison_operator(token.kind).unwrap_or("=")],
                        Some(lhs),
                        Some(rhs),
                        token.location,
                    )
                }
                kind if additive_operator(kind).is_some() => {
                    if 40 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let rhs = self.parse_expr(41)?;
                    make_aexpr(
                        AExprKind::Op,
                        vec![additive_operator(token.kind).unwrap_or("+")],
                        Some(lhs),
                        Some(rhs),
                        token.location,
                    )
                }
                kind if multiplicative_operator(kind).is_some() => {
                    if 50 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let rhs = self.parse_expr(51)?;
                    make_aexpr(
                        AExprKind::Op,
                        vec![multiplicative_operator(token.kind).unwrap_or("*")],
                        Some(lhs),
                        Some(rhs),
                        token.location,
                    )
                }
                TokenKind::Op => {
                    if 45 < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let rhs = self.parse_expr(46)?;
                    make_aexpr(
                        AExprKind::Op,
                        vec![token_name(&token).unwrap_or_else(|| token_text(&token))],
                        Some(lhs),
                        Some(rhs),
                        token.location,
                    )
                }
                _ => break,
            };
        }

        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Node> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Not => {
                let location = self.advance().location;
                let arg = self.parse_expr(60)?;
                Some(Node::BoolExpr(BoolExpr {
                    xpr: Expr::new(NodeTag::BoolExpr),
                    boolop: BoolExprType::NotExpr,
                    args: vec![arg],
                    location: location as ParseLoc,
                }))
            }
            TokenKind::Char('+') | TokenKind::Char('-') => {
                let token = self.advance().clone();
                let rhs = self.parse_expr(60)?;
                Some(make_aexpr(
                    AExprKind::Op,
                    vec![token_text(&token)],
                    None,
                    Some(rhs),
                    token.location,
                ))
            }
            TokenKind::Exists => {
                let location = self.advance().location;
                let subselect = self
                    .parse_parenthesized_statement()
                    .or_else(|| self.parse_expr(60));
                Some(Node::SubLink(SubLink {
                    xpr: Expr::new(NodeTag::SubLink),
                    sub_link_type: SubLinkType::ExistsSublink,
                    subselect: subselect.map(Box::new),
                    location: location as ParseLoc,
                    ..SubLink::default()
                }))
            }
            TokenKind::Array => {
                let location = self.advance().location;
                if self.consume(TokenKind::Char('[')) {
                    let elements = self.parse_expr_list_until(TokenKind::Char(']'));
                    self.consume(TokenKind::Char(']'));
                    Some(Node::AArrayExpr(AArrayExpr {
                        node_tag: NodeTag::AArrayExpr,
                        elements,
                        location: location as ParseLoc,
                        ..AArrayExpr::default()
                    }))
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
                    if token_name(&token).is_some() {
                        self.parse_name_or_func()
                    } else {
                        self.advance();
                        Some(leaf)
                    }
                } else {
                    self.parse_name_or_func()
                }
            }
        }
    }

    fn parse_name_or_func(&mut self) -> Option<Node> {
        let location = self.location();
        let fields = self.parse_name_nodes();
        if fields.is_empty() {
            return None;
        }
        if self.consume(TokenKind::Char('(')) {
            let mut agg_star = false;
            let mut agg_distinct = false;
            let args = if self.consume(TokenKind::Char('*')) {
                agg_star = true;
                self.consume(TokenKind::Char(')'));
                Vec::new()
            } else {
                agg_distinct = self.consume(TokenKind::Distinct);
                let args = self.parse_expr_list_until(TokenKind::Char(')'));
                self.consume(TokenKind::Char(')'));
                args
            };
            Some(Node::FuncCall(FuncCall {
                node_tag: NodeTag::FuncCall,
                funcname: fields,
                args,
                agg_star,
                agg_distinct,
                location: location as ParseLoc,
                ..FuncCall::default()
            }))
        } else {
            Some(Node::ColumnRef(ColumnRef {
                node_tag: NodeTag::ColumnRef,
                fields,
                location: location as ParseLoc,
            }))
        }
    }

    fn parse_name_nodes(&mut self) -> NodeList {
        let mut fields = Vec::new();
        loop {
            if self.consume(TokenKind::Char('*')) {
                fields.push(Node::AStar(AStar {
                    node_tag: NodeTag::AStar,
                }));
            } else {
                let token = self.peek().clone();
                let Some(name) = token_name(&token) else {
                    break;
                };
                self.advance();
                fields.push(make_string_node(name));
            }
            if !self.consume(TokenKind::Char('.')) {
                break;
            }
        }
        fields
    }

    fn parse_parenthesized_expr(&mut self) -> Option<Node> {
        let location = self.expect(TokenKind::Char('('))?.location;
        if self.starts_statement() {
            let tokens = self.take_until_balanced(TokenKind::Char(')'));
            self.consume(TokenKind::Char(')'));
            return tokens_to_statement_node(tokens).map(|subselect| {
                Node::SubLink(SubLink {
                    xpr: Expr::new(NodeTag::SubLink),
                    sub_link_type: SubLinkType::ExprSublink,
                    subselect: Some(Box::new(subselect)),
                    location: location as ParseLoc,
                    ..SubLink::default()
                })
            });
        }
        let args = self.parse_expr_list_until(TokenKind::Char(')'));
        self.consume(TokenKind::Char(')'));
        if args.len() == 1 {
            args.into_iter().next()
        } else {
            Some(Node::RowExpr(RowExpr {
                xpr: Expr::new(NodeTag::RowExpr),
                args,
                location: location as ParseLoc,
                ..RowExpr::default()
            }))
        }
    }

    fn parse_parenthesized_statement(&mut self) -> Option<Node> {
        if !self.consume(TokenKind::Char('(')) {
            return None;
        }
        let tokens = self.take_until_balanced(TokenKind::Char(')'));
        self.consume(TokenKind::Char(')'));
        tokens_to_statement_node(tokens)
    }

    fn parse_keyword_call_as_coalesce(&mut self) -> Option<Node> {
        let location = self.advance().location;
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'));
        self.consume(TokenKind::Char(')'));
        Some(Node::CoalesceExpr(CoalesceExpr {
            xpr: Expr::new(NodeTag::CoalesceExpr),
            args,
            location: location as ParseLoc,
            ..CoalesceExpr::default()
        }))
    }

    fn parse_keyword_call_as_minmax(&mut self) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'));
        self.consume(TokenKind::Char(')'));
        Some(Node::MinMaxExpr(MinMaxExpr {
            xpr: Expr::new(NodeTag::MinMaxExpr),
            op: if token.kind == TokenKind::Least {
                MinMaxOp::Least
            } else {
                MinMaxOp::Greatest
            },
            args,
            location: token.location as ParseLoc,
            ..MinMaxExpr::default()
        }))
    }

    fn parse_keyword_call_as_aexpr(&mut self, kind: AExprKind) -> Option<Node> {
        let token = self.advance().clone();
        self.expect(TokenKind::Char('('))?;
        let args = self.parse_expr_list_until(TokenKind::Char(')'));
        self.consume(TokenKind::Char(')'));
        let mut iter = args.into_iter();
        let lhs = iter.next();
        let rhs = iter.next();
        Some(make_aexpr(
            kind,
            vec![token_text(&token)],
            lhs,
            rhs,
            token.location,
        ))
    }

    fn parse_special_infix(
        &mut self,
        lhs: Node,
        op: TokenKind,
        negated: bool,
        location: usize,
    ) -> Option<Node> {
        match op {
            TokenKind::InP => {
                let rhs = if self.consume(TokenKind::Char('(')) {
                    if self.starts_statement() {
                        let tokens = self.take_until_balanced(TokenKind::Char(')'));
                        self.consume(TokenKind::Char(')'));
                        tokens_to_statement_node(tokens)
                    } else {
                        let elements = self.parse_expr_list_until(TokenKind::Char(')'));
                        self.consume(TokenKind::Char(')'));
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
                            elements,
                            location: location as ParseLoc,
                            ..AArrayExpr::default()
                        }))
                    }
                } else {
                    self.parse_expr(31)
                };
                Some(make_aexpr(
                    AExprKind::In,
                    vec![if negated { "not in" } else { "in" }],
                    Some(lhs),
                    rhs,
                    location,
                ))
            }
            TokenKind::Like | TokenKind::Ilike | TokenKind::Similar => {
                let rhs = self.parse_expr(31)?;
                let kind = match op {
                    TokenKind::Ilike => AExprKind::Ilike,
                    TokenKind::Similar => AExprKind::Similar,
                    _ => AExprKind::Like,
                };
                Some(make_aexpr(
                    kind,
                    vec![if negated { "not" } else { "" }, token_kind_text(op)]
                        .into_iter()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>(),
                    Some(lhs),
                    Some(rhs),
                    location,
                ))
            }
            TokenKind::Between => self.parse_between(lhs, negated, location),
            _ => None,
        }
    }

    fn parse_between(&mut self, lhs: Node, negated: bool, location: usize) -> Option<Node> {
        let symmetric = self.consume(TokenKind::Symmetric);
        self.consume(TokenKind::Asymmetric);
        let lower = self.parse_expr(31)?;
        self.consume(TokenKind::And);
        let upper = self.parse_expr(31)?;
        let kind = match (negated, symmetric) {
            (true, true) => AExprKind::NotBetweenSym,
            (true, false) => AExprKind::NotBetween,
            (false, true) => AExprKind::BetweenSym,
            (false, false) => AExprKind::Between,
        };
        Some(make_aexpr(
            kind,
            vec![if negated { "not between" } else { "between" }],
            Some(lhs),
            Some(Node::AArrayExpr(AArrayExpr {
                node_tag: NodeTag::AArrayExpr,
                elements: vec![lower, upper],
                location: location as ParseLoc,
                ..AArrayExpr::default()
            })),
            location,
        ))
    }

    fn parse_is_expr(&mut self, lhs: Node, location: usize) -> Option<Node> {
        let negated = self.consume(TokenKind::Not);
        if self.consume(TokenKind::Distinct) {
            self.consume(TokenKind::From);
            let rhs = self.parse_expr(31)?;
            return Some(make_aexpr(
                if negated {
                    AExprKind::NotDistinct
                } else {
                    AExprKind::Distinct
                },
                vec![if negated {
                    "is not distinct from"
                } else {
                    "is distinct from"
                }],
                Some(lhs),
                Some(rhs),
                location,
            ));
        }
        let rhs = self.parse_expr(31)?;
        Some(make_aexpr(
            AExprKind::Op,
            vec![if negated { "is not" } else { "is" }],
            Some(lhs),
            Some(rhs),
            location,
        ))
    }

    fn parse_cast_type_name(&mut self) -> Option<TypeName> {
        let location = self.location();
        let mut names = Vec::new();
        while !self.at(TokenKind::Eof) {
            if self.consume(TokenKind::Char('.')) {
                continue;
            }
            if expression_boundary(self.peek_kind()) {
                break;
            }
            let token = self.peek().clone();
            let Some(name) = token_name(&token) else {
                break;
            };
            self.advance();
            names.push(make_string_node(name));
            if self.consume(TokenKind::Char('(')) {
                self.take_until_balanced(TokenKind::Char(')'));
                self.consume(TokenKind::Char(')'));
                break;
            }
            while self.consume(TokenKind::Char('[')) {
                self.take_until_balanced(TokenKind::Char(']'));
                self.consume(TokenKind::Char(']'));
            }
        }
        if names.is_empty() {
            None
        } else {
            Some(TypeName {
                node_tag: NodeTag::TypeName,
                names,
                location: location as ParseLoc,
                ..TypeName::default()
            })
        }
    }

    fn parse_expr_list_until(&mut self, stop: TokenKind) -> NodeList {
        let mut items = Vec::new();
        while !self.at(stop) && !self.at(TokenKind::Eof) {
            if let Some(expr) = self.parse_expr(0) {
                items.push(expr);
            } else {
                self.advance();
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        items
    }

    fn take_until_balanced(&mut self, stop: TokenKind) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            if depth == 0 && kind == stop {
                break;
            }
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            out.push(self.advance().clone());
        }
        out
    }

    fn starts_statement(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::With
        )
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at(kind) {
            Some(self.advance().clone())
        } else {
            None
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.at(TokenKind::Eof) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn location(&self) -> usize {
        self.peek().location
    }
}

fn token_to_leaf(token: &Token) -> Option<Node> {
    match token.kind {
        TokenKind::IConst => match token.value {
            Some(TokenValue::Integer(value)) => Some(Node::AConst(AConst::integer(
                value,
                token.location as ParseLoc,
            ))),
            _ => None,
        },
        TokenKind::FConst | TokenKind::SConst | TokenKind::BConst | TokenKind::XConst => {
            token_name(token)
                .map(|value| Node::AConst(AConst::string(value, token.location as ParseLoc)))
        }
        TokenKind::Param => match token.value {
            Some(TokenValue::Integer(number)) => Some(Node::ParamRef(ParamRef {
                node_tag: NodeTag::ParamRef,
                number,
                location: token.location as ParseLoc,
            })),
            _ => None,
        },
        TokenKind::NullP => Some(Node::AConst(AConst::null(token.location as ParseLoc))),
        TokenKind::TrueP => Some(Node::AConst(AConst {
            node_tag: NodeTag::AConst,
            val: ValUnion::Boolean(Boolean::new(true)),
            location: token.location as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::FalseP => Some(Node::AConst(AConst {
            node_tag: NodeTag::AConst,
            val: ValUnion::Boolean(Boolean::new(false)),
            location: token.location as ParseLoc,
            ..AConst::default()
        })),
        TokenKind::Char('*') => Some(Node::AStar(AStar {
            node_tag: NodeTag::AStar,
        })),
        _ => token_name(token).map(|name| {
            Node::ColumnRef(ColumnRef {
                node_tag: NodeTag::ColumnRef,
                fields: vec![make_string_node(name)],
                location: token.location as ParseLoc,
            })
        }),
    }
}

fn tokens_to_func_call(tokens: &[Token]) -> Option<Node> {
    let open = tokens
        .iter()
        .position(|token| token.kind == TokenKind::Char('('))?;
    if tokens.last().map(|token| token.kind) != Some(TokenKind::Char(')')) || open == 0 {
        return None;
    }
    let name_tokens = &tokens[..open];
    let mut funcname = Vec::new();
    for token in name_tokens {
        if token.kind == TokenKind::Char('.') {
            continue;
        }
        funcname.push(make_string_node(token_name(token)?));
    }
    Some(Node::FuncCall(FuncCall {
        node_tag: NodeTag::FuncCall,
        funcname,
        location: tokens[0].location as ParseLoc,
        ..FuncCall::default()
    }))
}

fn tokens_to_object_with_args(tokens: Vec<Token>) -> Option<ObjectWithArgs> {
    if tokens.is_empty() {
        return None;
    }
    let open = find_top_level_token(&tokens, TokenKind::Char('('));
    let (name_tokens, arg_tokens, args_unspecified) = if let Some(open) = open {
        let close = find_matching_close(&tokens, open).unwrap_or(tokens.len().saturating_sub(1));
        (
            tokens[..open].to_vec(),
            tokens[open + 1..close].to_vec(),
            false,
        )
    } else {
        (tokens, Vec::new(), true)
    };
    let objname = tokens_to_name_nodes(&name_tokens);
    if objname.is_empty() {
        return None;
    }
    let objargs = split_top_level_commas(arg_tokens)
        .into_iter()
        .filter_map(tokens_to_type_name)
        .map(Node::TypeName)
        .collect();
    Some(ObjectWithArgs {
        node_tag: NodeTag::ObjectWithArgs,
        objname,
        objargs,
        args_unspecified,
        ..ObjectWithArgs::default()
    })
}

fn tokens_to_def_elem(tokens: Vec<Token>, location: usize) -> Option<DefElem> {
    let mut tokens = tokens
        .into_iter()
        .filter(|token| token.kind != TokenKind::Char(' '))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }
    let defname = token_name(tokens.first()?)?;
    tokens.remove(0);
    if matches!(
        tokens.first().map(|token| token.kind),
        Some(TokenKind::Char('='))
    ) {
        tokens.remove(0);
    }
    let arg = tokens_to_node(tokens).map(Box::new);
    Some(DefElem {
        node_tag: NodeTag::DefElem,
        defname: Some(defname),
        arg,
        location: location as ParseLoc,
        ..DefElem::default()
    })
}

fn tokens_to_type_name(tokens: Vec<Token>) -> Option<TypeName> {
    let names: NodeList = tokens
        .into_iter()
        .take_while(|token| {
            !matches!(
                token.kind,
                TokenKind::Char(',')
                    | TokenKind::Char(')')
                    | TokenKind::Default
                    | TokenKind::Not
                    | TokenKind::NullP
                    | TokenKind::Constraint
                    | TokenKind::Primary
                    | TokenKind::Unique
                    | TokenKind::Check
            )
        })
        .filter(|token| token.kind != TokenKind::Char('.'))
        .filter_map(|token| token_name(&token).map(make_string_node))
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(TypeName {
            node_tag: NodeTag::TypeName,
            names,
            ..TypeName::default()
        })
    }
}

fn tokens_to_name_nodes(tokens: &[Token]) -> NodeList {
    tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Char('.'))
        .filter_map(|token| token_name(token).map(make_string_node))
        .collect()
}

fn split_top_level_commas(tokens: Vec<Token>) -> Vec<Vec<Token>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            TokenKind::Char(',') if depth == 0 => {
                chunks.push(current);
                current = Vec::new();
                continue;
            }
            _ => {}
        }
        current.push(token);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn find_top_level_token(tokens: &[Token], needle: TokenKind) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if depth == 0 && token.kind == needle {
            return Some(index);
        }
        match token.kind {
            TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
            TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn find_matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::Char('(') => depth += 1,
            TokenKind::Char(')') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn tokens_to_text(tokens: &[Token]) -> std::string::String {
    tokens.iter().map(token_text).collect::<Vec<_>>().join(" ")
}

fn make_string_node(value: impl Into<std::string::String>) -> Node {
    Node::String(String::new(value))
}

fn range_var_from_parts(parts: Vec<std::string::String>, location: usize) -> RangeVar {
    let mut range = RangeVar {
        node_tag: NodeTag::RangeVar,
        inh: true,
        location: location as ParseLoc,
        ..RangeVar::default()
    };
    match parts.as_slice() {
        [rel] => range.relname = Some(rel.clone()),
        [schema, rel] => {
            range.schemaname = Some(schema.clone());
            range.relname = Some(rel.clone());
        }
        [catalog, schema, rel, ..] => {
            range.catalogname = Some(catalog.clone());
            range.schemaname = Some(schema.clone());
            range.relname = Some(rel.clone());
        }
        [] => {}
    }
    range
}

fn list_to_names(list: &[Node]) -> Vec<std::string::String> {
    list.iter()
        .filter_map(|node| match node {
            Node::String(value) => value.sval.clone(),
            _ => None,
        })
        .collect()
}

fn node_to_range_var(node: Node) -> Option<RangeVar> {
    match node {
        Node::AArrayExpr(array) => Some(range_var_from_parts(list_to_names(&array.elements), 0)),
        Node::RangeVar(range) => Some(range),
        _ => None,
    }
}

fn is_table_constraint_name(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "constraint" | "primary" | "unique" | "check" | "foreign" | "exclude"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_node(sql: &str) -> Node {
        let stmt = parse_one(sql).unwrap();
        *stmt.stmt.unwrap()
    }

    #[test]
    fn parses_basic_select_insert_update_delete() {
        assert!(matches!(
            first_node("select a, b from t where id = 1"),
            Node::SelectStmt(_)
        ));
        assert!(matches!(
            first_node("insert into t (a) values (1) returning a"),
            Node::InsertStmt(_)
        ));
        assert!(matches!(
            first_node("update t set a = 1 where id = 2"),
            Node::UpdateStmt(_)
        ));
        assert!(matches!(
            first_node("delete from t where id = 3"),
            Node::DeleteStmt(_)
        ));
    }

    #[test]
    fn parses_multiple_raw_statements() {
        let stmts = parse("select 1; select 2;").unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            *stmts[0].stmt.clone().unwrap(),
            Node::SelectStmt(_)
        ));
        assert!(matches!(
            *stmts[1].stmt.clone().unwrap(),
            Node::SelectStmt(_)
        ));
    }

    #[test]
    fn parses_common_create_alter_drop_forms() {
        assert!(matches!(
            first_node("create table s.t (id int, name text)"),
            Node::CreateStmt(_)
        ));
        assert!(matches!(
            first_node("create unique index idx on t (id)"),
            Node::IndexStmt(_)
        ));
        assert!(matches!(
            first_node("create view v as select 1"),
            Node::ViewStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t add column x int"),
            Node::AlterTableStmt(_)
        ));
        assert!(matches!(
            first_node("drop table if exists t cascade"),
            Node::DropStmt(_)
        ));
    }

    #[test]
    fn parses_utility_statements() {
        let cases = [
            ("set search_path to public", "set"),
            ("show search_path", "show"),
            ("begin", "begin"),
            ("commit", "commit"),
            ("prepare q as select 1", "prepare"),
            ("execute q", "execute"),
            ("deallocate q", "deallocate"),
            ("explain select 1", "explain"),
            ("copy t from 'file.csv'", "copy"),
            ("vacuum t", "vacuum"),
            ("call f(1)", "call"),
            ("listen chan", "listen"),
            ("notify chan, 'payload'", "notify"),
        ];
        for (sql, label) in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{label}: {err}"));
        }
    }

    #[test]
    fn dispatches_broad_statement_family() {
        let cases = [
            "create schema s",
            "create database d",
            "create extension e",
            "create role r",
            "create sequence s",
            "create domain d as int",
            "create type mood as enum ('sad','ok')",
            "create publication p",
            "create subscription s connection 'x' publication p",
            "drop database if exists d",
            "drop role if exists r",
            "drop owned by r",
            "truncate table t",
            "comment on table t is 'x'",
            "security label on table t is 'x'",
            "grant select on table t to r",
            "revoke select on table t from r",
            "refresh materialized view mv",
            "reindex table t",
            "discard all",
            "lock table t",
            "load 'x'",
            "wait for '0/0'",
        ];
        for sql in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
        }
    }

    #[test]
    fn builds_expression_ast_for_common_precedence() {
        let Node::SelectStmt(stmt) =
            first_node("select a + 1 * 2 from t where b::int >= 3 and not c")
        else {
            panic!("expected select");
        };
        let Node::ResTarget(target) = &stmt.target_list[0] else {
            panic!("expected target");
        };
        assert!(matches!(target.val.as_deref(), Some(Node::AExpr(_))));
        assert!(matches!(
            stmt.where_clause.as_deref(),
            Some(Node::BoolExpr(_))
        ));
    }

    #[test]
    fn dispatches_official_top_level_statement_families() {
        let cases = [
            "alter event trigger et disable",
            "alter collation c refresh version",
            "alter database d refresh collation version",
            "alter database d set search_path to public",
            "alter default privileges grant select on tables to r",
            "alter domain d set default 1",
            "alter type mood add value 'ok'",
            "alter extension e add table t",
            "alter foreign data wrapper fdw options (foo 'bar')",
            "alter server s options (foo 'bar')",
            "alter function f() stable",
            "alter group g add user u",
            "alter function f() depends on extension e",
            "alter table t set schema s",
            "alter table t owner to r",
            "alter operator +(int, int) set (commutator = +)",
            "alter type t set (receive = r)",
            "alter policy p on t using (true)",
            "alter property graph g add vertex tables (t)",
            "alter sequence s restart",
            "alter system set work_mem = '4MB'",
            "alter table t add column c int",
            "alter tablespace ts set (random_page_cost = 2)",
            "alter type ct add attribute a int",
            "alter publication p set table t",
            "alter role r set search_path to public",
            "alter subscription s refresh publication",
            "alter statistics st set statistics 10",
            "alter text search dictionary d (template = simple)",
            "alter user mapping for u server s options (foo 'bar')",
            "analyze t",
            "call f(1)",
            "checkpoint",
            "close c",
            "comment on table t is 'x'",
            "set constraints all deferred",
            "copy t from 'file.csv'",
            "create access method am type table handler h",
            "create table ct_as as select 1",
            "create assertion a check (1 = 1)",
            "create cast (int as text) without function",
            "create conversion conv for 'utf8' to 'latin1' from f",
            "create domain d as int",
            "create extension e",
            "create foreign data wrapper fdw",
            "create server s foreign data wrapper fdw",
            "create foreign table ft (id int) server s",
            "create function f() returns int language sql as 'select 1'",
            "create group g",
            "create materialized view mv as select 1",
            "create operator class opc for type int using btree as operator 1 =",
            "create operator family opf using btree",
            "alter operator family opf using btree add operator 1 =(int,int)",
            "create policy p on t using (true)",
            "create language plpgsql",
            "create property graph g vertex tables (t)",
            "create schema s",
            "create sequence seq",
            "create table t (id int)",
            "create subscription sub connection 'c' publication p",
            "create statistics st on a from t",
            "create tablespace ts location '/tmp'",
            "create transform for int language plpgsql (from sql with function f(int))",
            "create trigger tr before insert on t execute function f()",
            "create event trigger et on ddl_command_start execute function f()",
            "create role r",
            "create user u",
            "create user mapping for u server s",
            "create database d",
            "deallocate q",
            "declare c cursor for select 1",
            "create aggregate agg(int) (sfunc = f, stype = int)",
            "delete from t where id = 1",
            "discard all",
            "do 'begin end'",
            "drop cast (int as text)",
            "drop operator class opc using btree",
            "drop operator family opf using btree",
            "drop owned by r",
            "drop table if exists t",
            "drop subscription if exists sub",
            "drop tablespace if exists ts",
            "drop transform for int language plpgsql",
            "drop role if exists r",
            "drop user mapping if exists for u server s",
            "drop database if exists d",
            "execute q",
            "explain select 1",
            "fetch next from c",
            "grant select on table t to r",
            "grant role r to u",
            "import foreign schema s from server srv into public",
            "create index idx on t (id)",
            "insert into t values (1)",
            "listen ch",
            "refresh materialized view mv",
            "load 'x'",
            "lock table t",
            "merge into t using s on t.id = s.id when matched then update set id = s.id",
            "notify ch, 'payload'",
            "prepare q as select 1",
            "reassign owned by r to u",
            "reindex table t",
            "drop aggregate if exists agg(int)",
            "drop function if exists f()",
            "drop operator if exists +(int, int)",
            "alter table t rename to t2",
            "repack t using index idx",
            "revoke select on table t from r",
            "revoke role r from u",
            "create rule r as on update to t do notify ch",
            "security label on table t is 'x'",
            "select 1",
            "begin",
            "truncate table t",
            "unlisten *",
            "update t set id = 2",
            "vacuum t",
            "reset search_path",
            "set search_path to public",
            "show search_path",
            "create view v as select 1",
            "wait for '0/0'",
        ];

        for sql in cases {
            parse_one(sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
        }
    }

    #[test]
    fn dispatches_specific_extended_statement_nodes() {
        assert!(matches!(
            first_node("create table t as select 1"),
            Node::CreateTableAsStmt(_)
        ));
        assert!(matches!(
            first_node("create foreign data wrapper fdw"),
            Node::CreateFdwStmt(_)
        ));
        assert!(matches!(
            first_node("create property graph g vertex tables (t)"),
            Node::CreatePropGraphStmt(_)
        ));
        assert!(matches!(
            first_node("alter extension e add table t"),
            Node::AlterExtensionContentsStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t set schema s"),
            Node::AlterObjectSchemaStmt(_)
        ));
        assert!(matches!(
            first_node("alter table t owner to r"),
            Node::AlterOwnerStmt(_)
        ));
        assert!(matches!(
            first_node("alter role r set search_path to public"),
            Node::AlterRoleSetStmt(_)
        ));
        assert!(matches!(
            first_node("alter type ct add attribute a int"),
            Node::AlterTableStmt(AlterTableStmt {
                objtype: ObjectType::Type,
                ..
            })
        ));
        assert!(matches!(
            first_node("drop cast (int as text)"),
            Node::DropStmt(DropStmt {
                remove_type: ObjectType::Cast,
                ..
            })
        ));
        assert!(matches!(
            first_node("create rule r as on update to t do notify ch"),
            Node::RuleStmt(_)
        ));
        assert!(matches!(first_node("repack t"), Node::RepackStmt(_)));
        assert!(matches!(
            first_node("create recursive view v (n) as select 1"),
            Node::ViewStmt(_)
        ));
    }

    #[test]
    fn fills_complex_create_and_alter_fields() {
        let Node::CreateCastStmt(cast) =
            first_node("create cast (int as text) with inout as assignment")
        else {
            panic!("expected cast");
        };
        assert!(cast.sourcetype.is_some());
        assert!(cast.targettype.is_some());
        assert!(cast.inout);
        assert_eq!(cast.context, CoercionContext::Assignment);

        let Node::CreateForeignServerStmt(server) = first_node(
            "create server if not exists s type 't' version '1' foreign data wrapper fdw options (host 'x')",
        ) else {
            panic!("expected server");
        };
        assert_eq!(server.servername.as_deref(), Some("s"));
        assert_eq!(server.fdwname.as_deref(), Some("fdw"));
        assert!(server.if_not_exists);
        assert!(!server.options.is_empty());

        let Node::CreatePolicyStmt(policy) =
            first_node("create policy p on t for select to r using (id > 0) with check (id > 0)")
        else {
            panic!("expected policy");
        };
        assert_eq!(policy.policy_name.as_deref(), Some("p"));
        assert!(policy.table.is_some());
        assert!(policy.qual.is_some());
        assert!(policy.with_check.is_some());

        let Node::AlterPolicyStmt(policy) = first_node("alter policy p on t to r using (id > 1)")
        else {
            panic!("expected alter policy");
        };
        assert_eq!(policy.policy_name.as_deref(), Some("p"));
        assert!(policy.table.is_some());
        assert!(policy.qual.is_some());

        let Node::SelectStmt(select) = first_node(
            "select * from (select 1) s join f(1) g on true window w as (partition by a order by b) fetch first 2 rows with ties for update of s nowait",
        ) else {
            panic!("expected select");
        };
        assert!(matches!(
            select.from_clause.first(),
            Some(Node::JoinExpr(_))
        ));
        assert!(!select.window_clause.is_empty());
        assert!(!select.locking_clause.is_empty());
        assert_eq!(select.limit_option, LimitOption::WithTies);

        let Node::AlterTableStmt(alter) = first_node(
            "alter table t add column c int, alter column c set default 1, drop column if exists d cascade",
        ) else {
            panic!("expected alter table");
        };
        assert_eq!(alter.cmds.len(), 3);
        assert!(matches!(
            alter.cmds.first(),
            Some(Node::AlterTableCmd(AlterTableCmd {
                subtype: AlterTableType::AddColumn,
                ..
            }))
        ));
    }
}
