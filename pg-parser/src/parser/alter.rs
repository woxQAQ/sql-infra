//! Top-level `ALTER` statement dispatch.
//!
//! Object-specific grammar remains in neighboring modules; this module selects
//! the correct parser without weakening each object's syntax requirements.

use super::*;

impl Parser {
    pub(super) fn parse_alter(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Alter)?;
        self.record_completion_tokens(&[
            TokenKind::Default,
            TokenKind::Access,
            TokenKind::TypeP,
            TokenKind::Table,
            TokenKind::Index,
            TokenKind::Sequence,
            TokenKind::View,
            TokenKind::Materialized,
            TokenKind::Foreign,
            TokenKind::Database,
            TokenKind::SystemP,
            TokenKind::Tablespace,
            TokenKind::User,
            TokenKind::Role,
            TokenKind::GroupP,
            TokenKind::DomainP,
            TokenKind::Extension,
            TokenKind::Collation,
            TokenKind::Policy,
            TokenKind::Property,
            TokenKind::Publication,
            TokenKind::Subscription,
            TokenKind::Statistics,
            TokenKind::Event,
            TokenKind::Language,
            TokenKind::LargeP,
            TokenKind::Procedural,
            TokenKind::Rule,
            TokenKind::Schema,
            TokenKind::Server,
            TokenKind::Function,
            TokenKind::Procedure,
            TokenKind::Routine,
            TokenKind::Aggregate,
            TokenKind::Operator,
            TokenKind::TextP,
            TokenKind::Trigger,
        ]);
        if self.peek_kind() == TokenKind::Default {
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
        if self.top_level_contains(TokenKind::Completion)
            && (self.peek_kind() == TokenKind::Aggregate
                || (self.peek_kind() == TokenKind::Operator
                    && matches!(
                        self.peek_kind_n(1),
                        TokenKind::Class | TokenKind::Completion
                    )))
        {
            let identity = self.parse_alter_identity(&[
                TokenKind::Rename,
                TokenKind::Depends,
                TokenKind::Owner,
                TokenKind::Set,
                TokenKind::Completion,
            ])?;
            self.record_alter_identity_actions(&identity);
            return Err(self.error_here("expected an ALTER action"));
        }
        let node = match self.peek_kind() {
            TokenKind::Table => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::All]);
                if self.at(TokenKind::All) {
                    self.parse_alter_table_move_all(ObjectType::Table)?
                } else {
                    self.parse_alter_table_after_kind(ObjectType::Table)?
                }
            }
            TokenKind::Index => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::All]);
                if self.at(TokenKind::All) {
                    self.parse_alter_table_move_all(ObjectType::Index)?
                } else {
                    self.parse_alter_table_after_kind(ObjectType::Index)?
                }
            }
            TokenKind::Sequence => self.parse_alter_sequence()?,
            TokenKind::View => self.parse_alter_table(ObjectType::View)?,
            TokenKind::Materialized => {
                self.advance();
                self.expect(TokenKind::View)?;
                self.record_completion_tokens(&[TokenKind::All]);
                if self.at(TokenKind::All) {
                    self.parse_alter_table_move_all(ObjectType::Matview)?
                } else {
                    self.parse_alter_table_after_kind(ObjectType::Matview)?
                }
            }
            TokenKind::Foreign => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::Table, TokenKind::DataP]);
                if self.consume(TokenKind::Table) {
                    self.parse_alter_table_after_kind(ObjectType::ForeignTable)?
                } else {
                    self.expect(TokenKind::DataP)?;
                    self.expect(TokenKind::Wrapper)?;
                    self.parse_alter_fdw()?
                }
            }
            TokenKind::Database => self.parse_alter_database()?,
            TokenKind::SystemP => self.parse_alter_system()?,
            TokenKind::Tablespace => self.parse_alter_tablespace()?,
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_alter_user_mapping()?
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_alter_role()?,
            TokenKind::DomainP => self.parse_alter_domain()?,
            TokenKind::TypeP if self.looks_like_alter_composite_type() => {
                self.parse_alter_composite_type()?
            }
            TokenKind::TypeP => self.parse_alter_type()?,
            TokenKind::Extension => self.parse_alter_extension()?,
            TokenKind::Collation => self.parse_alter_collation()?,
            TokenKind::Policy => self.parse_alter_policy()?,
            TokenKind::Property => {
                self.advance();
                self.parse_alter_prop_graph()?
            }
            TokenKind::Publication => self.parse_alter_publication()?,
            TokenKind::Subscription => self.parse_alter_subscription()?,
            TokenKind::Statistics => self.parse_alter_stats()?,
            TokenKind::Event => {
                self.advance();
                self.parse_alter_event_trigger()?
            }
            TokenKind::Server => self.parse_alter_foreign_server()?,
            TokenKind::Function
            | TokenKind::Procedure
            | TokenKind::Routine
            | TokenKind::Aggregate => self.parse_alter_function()?,
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.parse_alter_op_family()?
            }
            TokenKind::Operator => self.parse_alter_operator()?,
            TokenKind::TextP => {
                self.advance();
                self.expect(TokenKind::Search)?;
                if self.consume(TokenKind::Dictionary) {
                    self.parse_alter_ts_dictionary()?
                } else {
                    self.expect(TokenKind::Configuration)?;
                    self.parse_alter_ts_configuration()?
                }
            }
            _ if self.top_level_contains(TokenKind::Completion) => {
                // Several ALTER families share an object identity and are
                // dispatched by a later action keyword. At an identity
                // completion point that keyword does not exist yet, so enter
                // the shared identity production to publish its typed slot.
                let identity = self.parse_alter_identity(&[
                    TokenKind::Rename,
                    TokenKind::Depends,
                    TokenKind::Owner,
                    TokenKind::Set,
                    TokenKind::Completion,
                ])?;
                self.record_alter_identity_actions(&identity);
                return Err(self.error_here("expected an ALTER action"));
            }
            other => return Err(self.error_here(format!("unsupported ALTER form {:?}", other))),
        };
        Ok(node)
    }
}
