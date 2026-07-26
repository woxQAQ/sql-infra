use super::*;

impl Parser {
    pub(super) fn parse_create(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Create)?;
        let replace = if self.consume(TokenKind::Or) {
            self.expect(TokenKind::Replace)?;
            true
        } else {
            false
        };
        let relpersistence = self.parse_create_relpersistence()?;
        let trusted = self.consume(TokenKind::Trusted);
        let procedural = self.consume(TokenKind::Procedural);
        if (trusted || procedural) && self.peek_kind() != TokenKind::Language {
            return Err(self.error_here("TRUSTED/PROCEDURAL is only valid for CREATE LANGUAGE"));
        }
        if relpersistence != b'p'
            && !matches!(
                self.peek_kind(),
                TokenKind::Table
                    | TokenKind::View
                    | TokenKind::Recursive
                    | TokenKind::Materialized
                    | TokenKind::Sequence
                    | TokenKind::Property
            )
        {
            return Err(self.error_here(
                "TEMP, TEMPORARY, LOCAL, GLOBAL, or UNLOGGED is not allowed for this CREATE statement",
            ));
        }
        if replace
            && !matches!(
                self.peek_kind(),
                TokenKind::Aggregate
                    | TokenKind::Function
                    | TokenKind::Procedure
                    | TokenKind::Language
                    | TokenKind::Transform
                    | TokenKind::Rule
                    | TokenKind::Trigger
                    | TokenKind::Constraint
                    | TokenKind::View
                    | TokenKind::Recursive
            )
        {
            return Err(self.error_here("OR REPLACE is not allowed for this CREATE statement"));
        }
        self.record_completion_tokens(&[
            TokenKind::Table,
            TokenKind::Foreign,
            TokenKind::Unique,
            TokenKind::Index,
            TokenKind::Schema,
            TokenKind::Database,
            TokenKind::Recursive,
            TokenKind::View,
            TokenKind::Materialized,
            TokenKind::Extension,
            TokenKind::Function,
            TokenKind::Procedure,
            TokenKind::User,
            TokenKind::Role,
            TokenKind::GroupP,
            TokenKind::Sequence,
            TokenKind::DomainP,
            TokenKind::TypeP,
            TokenKind::Publication,
            TokenKind::Subscription,
            TokenKind::Policy,
            TokenKind::Trigger,
            TokenKind::Constraint,
            TokenKind::Event,
            TokenKind::Language,
            TokenKind::Server,
            TokenKind::Tablespace,
            TokenKind::Access,
            TokenKind::Cast,
            TokenKind::Default,
            TokenKind::ConversionP,
            TokenKind::Transform,
            TokenKind::Statistics,
            TokenKind::Operator,
            TokenKind::Property,
            TokenKind::Rule,
            TokenKind::Assertion,
            TokenKind::Aggregate,
            TokenKind::Collation,
            TokenKind::TextP,
        ]);
        let node = match self.peek_kind() {
            TokenKind::Table => self.parse_create_table(false, relpersistence)?,
            TokenKind::Foreign => {
                self.advance();
                self.record_completion_tokens(&[TokenKind::Table, TokenKind::DataP]);
                if self.at(TokenKind::Table) {
                    self.parse_create_table(true, b'p')?
                } else {
                    self.expect(TokenKind::DataP)?;
                    self.expect(TokenKind::Wrapper)?;
                    self.parse_create_fdw()?
                }
            }
            TokenKind::Unique | TokenKind::Index => self.parse_index(false)?,
            TokenKind::Schema => self.parse_create_schema()?,
            TokenKind::Database => self.parse_createdb()?,
            TokenKind::Recursive => {
                self.advance();
                self.parse_view(replace, relpersistence, true)?
            }
            TokenKind::View => self.parse_view(replace, relpersistence, false)?,
            TokenKind::Materialized => {
                if relpersistence == b't' {
                    return Err(self.error_here("MATERIALIZED VIEW cannot be temporary"));
                }
                self.advance();
                self.parse_create_table_as(ObjectType::Matview, relpersistence)?
            }
            TokenKind::Extension => self.parse_create_extension()?,
            TokenKind::Function => self.parse_create_function(replace)?,
            TokenKind::Procedure => self.parse_create_procedure(replace)?,
            TokenKind::User if self.peek_kind_n(1) == TokenKind::Mapping => {
                self.parse_create_user_mapping()?
            }
            TokenKind::Role | TokenKind::User | TokenKind::GroupP => self.parse_create_role()?,
            TokenKind::Sequence => self.parse_create_sequence(relpersistence)?,
            TokenKind::DomainP => self.parse_create_domain()?,
            TokenKind::TypeP => self.parse_create_type()?,
            TokenKind::Publication => self.parse_create_publication()?,
            TokenKind::Subscription => self.parse_create_subscription()?,
            TokenKind::Policy => self.parse_create_policy()?,
            TokenKind::Trigger => self.parse_create_trigger(replace, false)?,
            TokenKind::Constraint => {
                self.advance();
                self.parse_create_trigger(replace, true)?
            }
            TokenKind::Event => {
                self.advance();
                self.parse_create_event_trigger()?
            }
            TokenKind::Language => self.parse_create_language(replace, trusted)?,
            TokenKind::Server => self.parse_create_server()?,
            TokenKind::Tablespace => self.parse_create_tablespace()?,
            TokenKind::Access => {
                self.advance();
                self.parse_create_am()?
            }
            TokenKind::Cast => self.parse_create_cast()?,
            TokenKind::Default => {
                self.advance();
                self.parse_create_conversion(true)?
            }
            TokenKind::ConversionP => self.parse_create_conversion(false)?,
            TokenKind::Transform => self.parse_create_transform(replace)?,
            TokenKind::Statistics => self.parse_create_stats()?,
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                self.advance();
                self.parse_create_op_class()?
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.parse_create_op_family()?
            }
            TokenKind::Property => {
                self.advance();
                self.parse_create_prop_graph(relpersistence)?
            }
            TokenKind::Rule => self.parse_rule(replace)?,
            // PostgreSQL recognizes ASSERTION as a keyword but does not implement
            // CREATE ASSERTION. Keep this branch to emit its specific diagnostic;
            // no corresponding AST node is constructed.
            TokenKind::Assertion => self.parse_create_assertion()?,
            TokenKind::Aggregate => self.parse_define(ObjectType::Aggregate, replace)?,
            TokenKind::Operator => self.parse_define(ObjectType::Operator, replace)?,
            TokenKind::Collation => self.parse_define(ObjectType::Collation, replace)?,
            TokenKind::TextP => self.parse_define_text_search()?,
            other => return Err(self.error_here(format!("unsupported CREATE form {:?}", other))),
        };
        Ok(node)
    }

    fn parse_create_relpersistence(&mut self) -> PResult<u8> {
        if self.consume(TokenKind::Temporary) || self.consume(TokenKind::Temp) {
            return Ok(b't');
        }
        if self.consume(TokenKind::Local) || self.consume(TokenKind::Global) {
            if !(self.consume(TokenKind::Temporary) || self.consume(TokenKind::Temp)) {
                return Err(
                    self.error_here("LOCAL or GLOBAL must be followed by TEMPORARY or TEMP")
                );
            }
            return Ok(b't');
        }
        if self.consume(TokenKind::Unlogged) {
            return Ok(b'u');
        }
        Ok(b'p')
    }

    fn parse_create_assertion(&mut self) -> PResult<Node> {
        let location = self.expect(TokenKind::Assertion)?.location();
        Err(ParseError::syntax_exit(
            location,
            "CREATE ASSERTION is not implemented by PostgreSQL",
        ))
    }
}
