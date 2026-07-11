use super::*;

impl Parser {
    pub(super) fn parse_alter(&mut self) -> PResult<Node> {
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
            TokenKind::Table if self.peek_kind_n(1) == TokenKind::All => {
                self.advance();
                self.parse_alter_table_move_all(ObjectType::Table)?
            }
            TokenKind::Table => self.parse_alter_table(ObjectType::Table)?,
            TokenKind::Index if self.peek_kind_n(1) == TokenKind::All => {
                self.advance();
                self.parse_alter_table_move_all(ObjectType::Index)?
            }
            TokenKind::Index => self.parse_alter_table(ObjectType::Index)?,
            TokenKind::Sequence => self.parse_alter_sequence()?,
            TokenKind::View => self.parse_alter_table(ObjectType::View)?,
            TokenKind::Materialized if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.expect(TokenKind::View)?;
                if self.at(TokenKind::All) {
                    self.parse_alter_table_move_all(ObjectType::Matview)?
                } else {
                    self.parse_alter_table_after_kind(ObjectType::Matview)?
                }
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Table => {
                self.advance();
                self.parse_alter_table(ObjectType::ForeignTable)?
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
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.parse_alter_prop_graph()?
            }
            TokenKind::Publication => self.parse_alter_publication()?,
            TokenKind::Subscription => self.parse_alter_subscription()?,
            TokenKind::Statistics => self.parse_alter_stats()?,
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.parse_alter_event_trigger()?
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.expect(TokenKind::DataP)?;
                self.expect(TokenKind::Wrapper)?;
                self.parse_alter_fdw()?
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
            TokenKind::TextP if self.peek_kind_n(1) == TokenKind::Search => {
                self.advance();
                self.consume(TokenKind::Search);
                if self.consume(TokenKind::Dictionary) {
                    self.parse_alter_ts_dictionary()?
                } else {
                    self.expect(TokenKind::Configuration)?;
                    self.parse_alter_ts_configuration()?
                }
            }
            other => return Err(self.error_here(format!("unsupported ALTER form {:?}", other))),
        };
        Ok(node)
    }
}
