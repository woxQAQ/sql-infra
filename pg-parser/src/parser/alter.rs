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
            TokenKind::TypeP,
            TokenKind::Table,
            TokenKind::Index,
            TokenKind::Sequence,
            TokenKind::View,
            TokenKind::Materialized,
            TokenKind::Foreign,
            TokenKind::ConversionP,
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
            && self.should_enter_generic_identity_completion()
        {
            let is_collation = self.peek_kind() == TokenKind::Collation;
            let identity = self.parse_alter_identity(&[
                TokenKind::Rename,
                TokenKind::Depends,
                TokenKind::Owner,
                TokenKind::Set,
                TokenKind::Completion,
            ])?;
            self.record_alter_identity_actions(&identity);
            if is_collation {
                self.record_completion_tokens(&[TokenKind::Refresh]);
                if self.consume(TokenKind::Refresh) {
                    self.record_completion_tokens(&[TokenKind::VersionP]);
                }
            }
            self.record_alter_identity_action_continuation(&identity);
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
            TokenKind::Sequence => {
                // PostgreSQL routes `ALTER SEQUENCE ... OWNER TO` through
                // the relation command grammar. Sequence options use a
                // separate AST node, so select the relation grammar only for
                // this command form.
                if self.top_level_action_pair(TokenKind::Owner, TokenKind::To)
                    || self.top_level_action_completion(TokenKind::Owner)
                {
                    self.advance();
                    self.parse_alter_table_after_kind(ObjectType::Sequence)?
                } else {
                    self.parse_alter_sequence()?
                }
            }
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

    fn looks_like_alter_enum(&self) -> bool {
        if self.peek_kind() != TokenKind::TypeP {
            return false;
        }
        let mut kinds = self.top_level_kinds();
        if let Some(comma) = kinds.iter().position(|kind| *kind == TokenKind::Char(',')) {
            kinds.truncate(comma);
        }
        let completion = kinds.iter().position(|kind| *kind == TokenKind::Completion);
        let action_end = completion.unwrap_or(kinds.len());
        let mut action_start = 1usize;
        if kinds.get(action_start).is_some() {
            action_start += 1;
            while kinds.get(action_start) == Some(&TokenKind::Char('.'))
                && kinds.get(action_start + 1).is_some()
            {
                action_start += 2;
            }
        }
        for index in action_start..action_end {
            let action = kinds[index];
            if !matches!(
                action,
                TokenKind::AddP | TokenKind::Rename | TokenKind::Drop
            ) {
                continue;
            }
            let next = kinds[index + 1..action_end].first().copied();
            match next {
                Some(TokenKind::ValueP) => return true,
                Some(TokenKind::Attribute) => return false,
                Some(TokenKind::Completion) | None => {
                    let after = completion.and_then(|completion| {
                        kinds[completion + 1..]
                            .iter()
                            .find(|kind| **kind != TokenKind::Completion)
                            .copied()
                    });
                    return match after {
                        Some(TokenKind::ValueP) => true,
                        Some(TokenKind::Attribute) => false,
                        _ => true,
                    };
                }
                _ => continue,
            }
        }
        completion.is_some_and(|completion| {
            kinds[completion + 1..]
                .iter()
                .find(|kind| **kind != TokenKind::Completion)
                == Some(&TokenKind::ValueP)
        })
    }

    fn should_enter_generic_identity_completion(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Collation | TokenKind::Trigger => true,
            TokenKind::Extension => self.top_level_action_completion(TokenKind::Set),
            TokenKind::Materialized => false,
            TokenKind::ConversionP => self.top_level_action_completion(TokenKind::Set),
            TokenKind::Sequence => self.top_level_action_completion(TokenKind::Set),
            TokenKind::TextP => {
                matches!(
                    self.peek_kind_n(2),
                    TokenKind::Parser
                        | TokenKind::Dictionary
                        | TokenKind::Template
                        | TokenKind::Configuration
                ) && (self.top_level_text_search_action_completion(TokenKind::Set)
                    || matches!(self.peek_kind_n(2), TokenKind::Parser | TokenKind::Template)
                        && self.top_level_completion_at_end())
            }
            TokenKind::Aggregate => true,
            TokenKind::Operator => matches!(
                self.peek_kind_n(1),
                TokenKind::Class | TokenKind::Completion
            ),
            _ => false,
        }
    }

    fn looks_like_rename_stmt(&self) -> bool {
        if self.peek_kind() == TokenKind::TypeP
            && self.top_level_adjacent(TokenKind::Rename, TokenKind::ValueP)
        {
            return false;
        }
        self.top_level_rename_action()
    }

    fn looks_like_alter_object_depends_stmt(&self) -> bool {
        if self.peek_kind() == TokenKind::Policy {
            let positions = self.top_level_token_positions();
            let identity_start = self.alter_identity_start_index(&positions);
            if positions.windows(2).enumerate().any(|(index, pair)| {
                self.tokens[pair[0]].kind == TokenKind::Depends
                    && self.tokens[pair[1]].kind == TokenKind::On
                    && index == identity_start
            }) {
                return false;
            }
        }
        self.top_level_action_pair(TokenKind::Depends, TokenKind::On)
            || self.top_level_action_completion(TokenKind::Depends)
    }

    fn looks_like_alter_object_schema_stmt(&self) -> bool {
        if !self.top_level_action_pair(TokenKind::Set, TokenKind::Schema) {
            return false;
        }
        if self.peek_kind() == TokenKind::SystemP {
            return false;
        }
        // FUNCTION/PROCEDURE/ROUTINE/ROLE/DATABASE also have a GUC form:
        // `SET schema TO ...` (or `SET schema = ...`). It must remain in the
        // object's dedicated SET parser instead of being interpreted as SET
        // SCHEMA. Relation and type forms accept `TO` as a schema name, so
        // apply this disambiguation only to GUC-capable object kinds.
        if !self.alter_object_has_guc_set() {
            return true;
        }
        let kinds = self.top_level_kinds();
        let Some(schema) = kinds
            .windows(2)
            .position(|pair| pair == [TokenKind::Set, TokenKind::Schema])
        else {
            return true;
        };
        let next = kinds
            .iter()
            .skip(schema + 2)
            .find(|kind| **kind != TokenKind::Completion);
        if matches!(
            next,
            Some(TokenKind::To | TokenKind::From | TokenKind::Char('='))
        ) {
            return false;
        }
        // `SET SCHEMA 'value'` is PostgreSQL's shorthand for setting the
        // `search_path` GUC (`set_rest_more: SCHEMA Sconst`), reachable from
        // ALTER FUNCTION/PROCEDURE/ROUTINE/ROLE/USER/DATABASE ... SET.
        if matches!(next, Some(TokenKind::SConst)) {
            return false;
        }
        if kinds.get(schema + 2) == Some(&TokenKind::Completion) {
            // At a completion point after `schema`, a literal or numeric
            // suffix indicates `SET schema TO ...`; an identifier suffix is
            // the target schema in `SET SCHEMA ...`.
            if matches!(
                next,
                Some(
                    TokenKind::SConst
                        | TokenKind::IConst
                        | TokenKind::FConst
                        | TokenKind::Param
                        | TokenKind::CurrentP
                        | TokenKind::CurrentRole
                        | TokenKind::CurrentUser
                        | TokenKind::SessionUser
                        | TokenKind::Default
                        | TokenKind::NullP
                        | TokenKind::TrueP
                        | TokenKind::FalseP
                )
            ) {
                return false;
            }
        }
        true
    }

    fn looks_like_alter_owner_stmt(&self) -> bool {
        if !self.top_level_action_pair(TokenKind::Owner, TokenKind::To)
            && !self.top_level_action_completion(TokenKind::Owner)
        {
            return false;
        }
        !matches!(
            (self.peek_kind(), self.peek_kind_n(1)),
            (TokenKind::Table, _)
                | (TokenKind::Index, _)
                | (TokenKind::Sequence, _)
                | (TokenKind::View, _)
                | (TokenKind::Materialized, TokenKind::View)
                | (TokenKind::Foreign, TokenKind::Table)
        )
    }

    fn looks_like_alter_composite_type(&self) -> bool {
        self.peek_kind() == TokenKind::TypeP
            && (self.top_level_action_pair(TokenKind::AddP, TokenKind::Attribute)
                || self.top_level_action_pair(TokenKind::Drop, TokenKind::Attribute)
                || self.top_level_action_pair(TokenKind::Alter, TokenKind::Attribute)
                || self
                    .top_level_action_completion_followed_by(TokenKind::AddP, TokenKind::Attribute)
                || self
                    .top_level_action_completion_followed_by(TokenKind::Drop, TokenKind::Attribute)
                || self.top_level_action_completion_followed_by(
                    TokenKind::Alter,
                    TokenKind::Attribute,
                )
                || (self.top_level_action_completion(TokenKind::Alter)
                    && self.top_level_completion_at_end()))
    }

    fn top_level_kinds_without_completion(&self) -> Vec<TokenKind> {
        let mut kinds = self.top_level_kinds();
        if let Some(completion) = kinds.iter().position(|kind| *kind == TokenKind::Completion) {
            kinds.truncate(completion);
        }
        kinds
    }

    fn top_level_action_pair(&self, first: TokenKind, second: TokenKind) -> bool {
        let positions = self.top_level_token_positions();
        positions.windows(2).enumerate().any(|(index, pair)| {
            self.tokens[pair[0]].kind == first
                && self.tokens[pair[1]].kind == second
                && !self.top_level_action_is_guc_name(&positions, index)
        })
    }

    fn top_level_action_completion(&self, action: TokenKind) -> bool {
        let kinds = self.top_level_kinds_without_completion();
        if !self.top_level_contains(TokenKind::Completion) {
            return false;
        }
        let Some(action_index) = kinds.iter().rposition(|kind| *kind == action) else {
            return false;
        };
        if action_index + 1 != kinds.len() {
            return false;
        }
        // An action keyword at a name slot (for example ADD COLUMN owner|)
        // must remain an identifier. These preceding grammar markers cannot
        // introduce an ALTER identity action.
        let positions = self.top_level_token_positions();
        if action_index < positions.len()
            && self.top_level_action_is_guc_name(&positions, action_index)
        {
            return false;
        }
        !self.top_level_action_prefix_is_name(&positions, action_index)
    }

    fn top_level_action_completion_followed_by(
        &self,
        action: TokenKind,
        expected: TokenKind,
    ) -> bool {
        let kinds = self.top_level_kinds();
        let Some(completion) = kinds.iter().position(|kind| *kind == TokenKind::Completion) else {
            return false;
        };
        if kinds[..completion].last() != Some(&action) {
            return false;
        }
        let positions = self.top_level_token_positions();
        if let Some(action_index) = positions.len().checked_sub(1)
            && self.top_level_action_is_guc_name(&positions, action_index)
        {
            return false;
        }
        kinds[completion + 1..]
            .iter()
            .find(|kind| **kind != TokenKind::Completion)
            == Some(&expected)
    }

    fn top_level_text_search_action_completion(&self, action: TokenKind) -> bool {
        let kinds = self.top_level_kinds_without_completion();
        kinds
            .iter()
            .rposition(|kind| *kind == action)
            .is_some_and(|index| index >= 4 && self.top_level_action_completion(action))
    }

    fn top_level_completion_at_end(&self) -> bool {
        let kinds = self.top_level_kinds();
        let Some(completion) = kinds.iter().position(|kind| *kind == TokenKind::Completion) else {
            return false;
        };
        kinds[completion + 1..]
            .iter()
            .all(|kind| *kind == TokenKind::Completion)
    }

    fn top_level_rename_action(&self) -> bool {
        let positions = self.top_level_token_positions();
        let identity_end = self.alter_identity_end_index(&positions);
        for (index, position) in positions.iter().enumerate() {
            if self.tokens[*position].kind != TokenKind::Rename {
                continue;
            }
            if identity_end.is_some_and(|identity_end| index < identity_end) {
                continue;
            }
            if self.top_level_action_is_guc_name(&positions, index) {
                continue;
            }
            let mut following = positions[index + 1..].iter().copied();
            let Some(next) = following.next() else {
                if self.top_level_contains(TokenKind::Completion) {
                    return !self.top_level_action_prefix_is_name(&positions, index);
                }
                continue;
            };
            let next_kind = self.tokens[next].kind;
            if matches!(
                next_kind,
                TokenKind::To
                    | TokenKind::Column
                    | TokenKind::Constraint
                    | TokenKind::Attribute
                    | TokenKind::ValueP
            ) {
                return true;
            }
            if self.top_level_name_token(next)
                && following
                    .next()
                    .is_some_and(|position| self.tokens[position].kind == TokenKind::To)
            {
                return true;
            }
        }
        false
    }

    fn alter_identity_end_index(&self, positions: &[usize]) -> Option<usize> {
        let kind_at = |index: usize| {
            positions
                .get(index)
                .map(|position| self.tokens[*position].kind)
        };
        let first = kind_at(0)?;
        let second = kind_at(1);
        let mut index = self.alter_identity_start_index(positions);
        if kind_at(index) == Some(TokenKind::IfP) && kind_at(index + 1) == Some(TokenKind::Exists) {
            index += 2;
        }

        let relation_identity = matches!(
            first,
            TokenKind::Table | TokenKind::Sequence | TokenKind::View | TokenKind::Index
        ) || matches!(first, TokenKind::Materialized | TokenKind::Property)
            || first == TokenKind::Foreign && second == Some(TokenKind::Table);
        if relation_identity {
            if kind_at(index) == Some(TokenKind::Only) {
                index += 1;
            }
            if kind_at(index) == Some(TokenKind::Char('(')) {
                index += 1;
            } else {
                index = self.skip_top_level_qualified_name(positions, index);
            }
            if kind_at(index) == Some(TokenKind::Char('*')) {
                index += 1;
            }
            return Some(index);
        }

        if matches!(
            first,
            TokenKind::Policy | TokenKind::Rule | TokenKind::Trigger
        ) {
            index += usize::from(kind_at(index).is_some());
            if kind_at(index) == Some(TokenKind::On) {
                index += 1;
                index = self.skip_top_level_qualified_name(positions, index);
            }
            return Some(index);
        }

        if matches!(
            first,
            TokenKind::Function | TokenKind::Procedure | TokenKind::Routine
        ) || first == TokenKind::Aggregate
            || first == TokenKind::Operator
                && !matches!(second, Some(TokenKind::Class | TokenKind::Family))
        {
            index = self.skip_top_level_qualified_name(positions, index);
            if kind_at(index) == Some(TokenKind::Char('(')) {
                index += 1;
            }
            return Some(index);
        }

        if first == TokenKind::Operator
            && matches!(second, Some(TokenKind::Class | TokenKind::Family))
        {
            index = self.skip_top_level_qualified_name(positions, index);
            if kind_at(index) == Some(TokenKind::Using) {
                index += 1 + usize::from(kind_at(index + 1).is_some());
            }
            return Some(index);
        }

        if matches!(
            first,
            TokenKind::Collation
                | TokenKind::ConversionP
                | TokenKind::DomainP
                | TokenKind::Statistics
                | TokenKind::TextP
                | TokenKind::TypeP
        ) {
            return Some(self.skip_top_level_qualified_name(positions, index));
        }

        Some(index + usize::from(kind_at(index).is_some()))
    }

    fn skip_top_level_qualified_name(&self, positions: &[usize], mut index: usize) -> usize {
        if positions.get(index).is_none() {
            return index;
        }
        index += 1;
        while positions
            .get(index)
            .is_some_and(|position| self.tokens[*position].kind == TokenKind::Char('.'))
            && positions.get(index + 1).is_some()
        {
            index += 2;
        }
        index
    }

    fn alter_identity_start_index(&self, positions: &[usize]) -> usize {
        let Some(first) = positions
            .first()
            .map(|position| self.tokens[*position].kind)
        else {
            return 1;
        };
        match first {
            TokenKind::Access
            | TokenKind::Event
            | TokenKind::LargeP
            | TokenKind::Materialized
            | TokenKind::Operator
            | TokenKind::Procedural
            | TokenKind::Property => 2,
            TokenKind::Foreign => {
                if positions.get(1).map(|position| self.tokens[*position].kind)
                    == Some(TokenKind::DataP)
                {
                    3
                } else {
                    2
                }
            }
            TokenKind::TextP => 3,
            _ => 1,
        }
    }

    fn top_level_action_is_guc_name(&self, positions: &[usize], action_index: usize) -> bool {
        for (index, position) in positions[..action_index].iter().enumerate().rev() {
            match self.tokens[*position].kind {
                TokenKind::Char(',') => return false,
                TokenKind::Set | TokenKind::Reset => return self.top_level_guc_marker(index),
                _ => {}
            }
        }
        false
    }

    fn top_level_action_prefix_is_name(&self, positions: &[usize], action_index: usize) -> bool {
        let Some(previous_index) = action_index.checked_sub(1) else {
            return false;
        };
        let previous_kind = self.tokens[positions[previous_index]].kind;
        match previous_kind {
            TokenKind::Set | TokenKind::Reset => self.top_level_guc_marker(previous_index),
            TokenKind::AddP
            | TokenKind::Drop
            | TokenKind::Default
            | TokenKind::Column
            | TokenKind::Constraint
            | TokenKind::Attribute
            | TokenKind::ValueP => true,
            kind => self.is_alter_object_kind(kind),
        }
    }

    fn top_level_guc_marker(&self, marker_index: usize) -> bool {
        if self.peek_kind() == TokenKind::SystemP {
            return true;
        }
        if !matches!(
            self.peek_kind(),
            TokenKind::Database
                | TokenKind::Function
                | TokenKind::Procedure
                | TokenKind::Routine
                | TokenKind::Role
                | TokenKind::User
        ) {
            return false;
        }
        marker_index > 1
    }

    fn top_level_name_token(&self, position: usize) -> bool {
        let token = &self.tokens[position];
        if matches!(token.kind, TokenKind::Ident | TokenKind::UIdent) {
            return true;
        }
        matches!(&token.value, Some(TokenValue::Keyword(word))
        if lookup_keyword(word).is_some_and(|keyword| {
            matches!(
                keyword.category,
                KeywordCategory::Unreserved | KeywordCategory::ColName
            )
        }))
    }

    fn alter_object_has_guc_set(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Database
                | TokenKind::Function
                | TokenKind::Procedure
                | TokenKind::Routine
                | TokenKind::Role
                | TokenKind::User
                | TokenKind::SystemP
        )
    }

    fn is_alter_object_kind(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Access
                | TokenKind::Aggregate
                | TokenKind::Collation
                | TokenKind::ConversionP
                | TokenKind::Database
                | TokenKind::DomainP
                | TokenKind::Event
                | TokenKind::Extension
                | TokenKind::Foreign
                | TokenKind::Function
                | TokenKind::GroupP
                | TokenKind::Index
                | TokenKind::Language
                | TokenKind::LargeP
                | TokenKind::Materialized
                | TokenKind::Operator
                | TokenKind::Policy
                | TokenKind::Procedure
                | TokenKind::Procedural
                | TokenKind::Property
                | TokenKind::Publication
                | TokenKind::Role
                | TokenKind::Routine
                | TokenKind::Rule
                | TokenKind::Schema
                | TokenKind::Sequence
                | TokenKind::Server
                | TokenKind::Statistics
                | TokenKind::Subscription
                | TokenKind::Table
                | TokenKind::Tablespace
                | TokenKind::TextP
                | TokenKind::Trigger
                | TokenKind::TypeP
                | TokenKind::User
                | TokenKind::View
        )
    }

    fn top_level_token_positions(&self) -> Vec<usize> {
        let mut positions = Vec::new();
        let mut depth = 0usize;
        let mut index = self.pos;
        while let Some(token) = self.tokens.get(index) {
            if token.kind == TokenKind::Eof
                || (depth == 0
                    && matches!(token.kind, TokenKind::Completion | TokenKind::Char(';')))
            {
                break;
            }
            if depth == 0 {
                positions.push(index);
            }
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => depth = depth.saturating_sub(1),
                _ => {}
            }
            index += 1;
        }
        positions
    }

    fn top_level_adjacent(&self, first: TokenKind, second: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .filter(|kind| *kind != TokenKind::Completion)
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == [first, second])
    }
}
