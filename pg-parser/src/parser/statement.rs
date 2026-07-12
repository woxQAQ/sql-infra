use super::*;

impl Parser {
    pub(super) fn parse_statement(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        match self.peek_kind() {
            TokenKind::With => self.parse_with_statement(),
            TokenKind::Select | TokenKind::Values | TokenKind::Table | TokenKind::Char('(') => {
                Ok(Node::SelectStmt(self.parse_select(with_clause)?))
            }
            TokenKind::Insert => self.parse_insert(with_clause),
            TokenKind::Update => self.parse_update(with_clause),
            TokenKind::DeleteP => self.parse_delete(with_clause),
            TokenKind::Merge => self.parse_merge(with_clause),
            TokenKind::Create => self.parse_create(),
            TokenKind::Alter => self.parse_alter(),
            TokenKind::Drop => self.parse_drop(),
            TokenKind::Set if self.peek_kind_n(1) == TokenKind::Constraints => {
                self.parse_set_constraints()
            }
            TokenKind::Set => self.parse_variable_set(),
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
            TokenKind::Wait => self.parse_wait(),
            other => Err(self.error_here(format!("unexpected token {:?}", other))),
        }
    }
}
