use super::*;

impl Parser {
    pub(super) fn parse_simple_name_list_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            if self.at_any(stops) {
                break;
            }
            names.push(make_string_node(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("expected a name"))?,
            ));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a name after ','"));
            }
        }
        if names.is_empty() {
            return Err(self.error_here("expected at least one name"));
        }
        Ok(names)
    }

    pub(super) fn parse_one_any_name(&mut self, stops: &[TokenKind]) -> PResult<Node> {
        if self.at_any(stops) {
            return Err(self.error_here("expected a qualified name"));
        }
        let parts = self.consume_name_parts();
        if parts.is_empty() {
            return Err(self.error_here("expected a qualified name"));
        }
        Ok(name_list_node(
            parts.into_iter().map(make_string_node).collect(),
        ))
    }

    pub(super) fn parse_any_name_list_until(&mut self, stops: &[TokenKind]) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            names.push(self.parse_one_any_name(stops)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected a qualified name after ','"));
            }
        }
        Ok(names)
    }

    pub(super) fn parse_function_expression(&mut self) -> PResult<Node> {
        let start = self.pos;
        let remaining = &self.tokens[start..self.end];
        let open = find_top_level_token(remaining, TokenKind::Char('('))
            .ok_or_else(|| self.error_here("function expression requires '('"))?;
        let close = find_matching_close(remaining, open)
            .ok_or_else(|| self.error_here("unterminated function expression"))?;
        let expression = self.parse_expression_range(start..start + close + 1)?;
        if !is_function_expression_node(&expression) {
            return Err(self.error_here("expected a function expression"));
        }
        self.pos = start + close + 1;
        Ok(expression)
    }

    pub(super) fn parse_name_list(&mut self) -> NodeList {
        self.consume_name_parts()
            .into_iter()
            .map(make_string_node)
            .collect()
    }

    pub(super) fn parse_func_name_list(&mut self) -> NodeList {
        self.consume_func_name_parts()
            .into_iter()
            .map(make_string_node)
            .collect()
    }

    pub(super) fn parse_name_list_until_keywords(&mut self, stops: &[TokenKind]) -> NodeList {
        if self.at_any(stops) {
            Vec::new()
        } else {
            self.parse_name_list()
        }
    }

    pub(super) fn try_parse_range_var(&mut self, allow_set_alias: bool) -> Option<RangeVar> {
        let location = self.location();
        let parts = self.consume_qualified_name_parts();
        if parts.is_empty() {
            return None;
        }
        let mut range = range_var_from_parts(parts, location);
        range.alias = self.parse_optional_alias(allow_set_alias);
        Some(range)
    }

    pub(super) fn parse_relation_expr(&mut self, allow_alias: bool) -> PResult<RangeVar> {
        let only = self.consume(TokenKind::Only);
        let parenthesized = only && self.consume(TokenKind::Char('('));
        let mut range = self.try_parse_qualified_range_var().ok_or_else(|| {
            self.error_here(if only {
                "ONLY requires a table reference"
            } else {
                "expected a table reference"
            })
        })?;
        if parenthesized {
            self.expect(TokenKind::Char(')'))?;
        }
        if !only {
            self.consume(TokenKind::Char('*'));
        }
        range.inh = !only;
        if allow_alias {
            range.alias = self.parse_optional_alias_clause()?;
        }
        Ok(range)
    }

    pub(super) fn parse_optional_alias(&mut self, allow_set_alias: bool) -> Option<Box<Alias>> {
        let has_as = self.consume(TokenKind::As);
        if has_as || (allow_set_alias || !self.at(TokenKind::Set)) {
            self.consume_col_id().map(|aliasname| {
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

    pub(super) fn parse_optional_alias_clause(&mut self) -> PResult<Option<Box<Alias>>> {
        let Some(mut alias) = self.parse_optional_alias(true) else {
            return Ok(None);
        };
        if self.consume(TokenKind::Char('(')) {
            alias.colnames = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
        }
        Ok(Some(alias))
    }

    pub(super) fn try_parse_qualified_range_var(&mut self) -> Option<RangeVar> {
        let location = self.location();
        let parts = self.consume_qualified_name_parts();
        if parts.is_empty() {
            None
        } else {
            Some(range_var_from_parts(parts, location))
        }
    }

    pub(super) fn consume_name_parts(&mut self) -> Vec<std::string::String> {
        let mut parts = Vec::new();
        let Some(first) = self.consume_col_id() else {
            return parts;
        };
        parts.push(first);
        while self.at(TokenKind::Char('.')) {
            let dot_pos = self.pos;
            self.advance();
            if self.at(TokenKind::Char('*')) {
                break;
            }
            if let Some(name) = self.consume_col_label() {
                parts.push(name);
            } else {
                self.pos = dot_pos;
                break;
            }
        }
        parts
    }

    pub(super) fn consume_func_name_parts(&mut self) -> Vec<std::string::String> {
        let mut parts = Vec::new();
        let first = if self.peek_kind_n(1) == TokenKind::Char('.') {
            self.consume_col_id()
        } else {
            self.consume_identifier_in_categories(&[
                KeywordCategory::Unreserved,
                KeywordCategory::TypeFuncName,
            ])
        };
        let Some(first) = first else {
            return parts;
        };
        parts.push(first);
        while self.consume(TokenKind::Char('.')) {
            let Some(name) = self.consume_col_label() else {
                self.pos = self.pos.saturating_sub(1);
                break;
            };
            parts.push(name);
        }
        parts
    }

    pub(super) fn consume_qualified_name_parts(&mut self) -> Vec<std::string::String> {
        let mut parts = Vec::new();
        let Some(first) = self.consume_col_id() else {
            return parts;
        };
        parts.push(first);
        while parts.len() < 3 && self.consume(TokenKind::Char('.')) {
            if self.at(TokenKind::Char('*')) {
                self.pos = self.pos.saturating_sub(1);
                break;
            }
            let Some(name) = self.consume_col_label() else {
                self.pos = self.pos.saturating_sub(1);
                break;
            };
            parts.push(name);
        }
        parts
    }

    pub(super) fn consume_col_id(&mut self) -> Option<std::string::String> {
        self.consume_identifier_in_categories(&[
            KeywordCategory::Unreserved,
            KeywordCategory::ColName,
        ])
    }

    pub(super) fn consume_identifier(&mut self) -> Option<std::string::String> {
        if !matches!(self.peek_kind(), TokenKind::Ident | TokenKind::UIdent) {
            return None;
        }
        let token = self.advance().clone();
        token_name(&token)
    }

    pub(super) fn consume_col_label(&mut self) -> Option<std::string::String> {
        self.consume_identifier_in_categories(&[
            KeywordCategory::Unreserved,
            KeywordCategory::ColName,
            KeywordCategory::TypeFuncName,
            KeywordCategory::Reserved,
        ])
    }

    pub(super) fn consume_non_reserved_word(&mut self) -> Option<std::string::String> {
        self.consume_identifier_in_categories(&[
            KeywordCategory::Unreserved,
            KeywordCategory::ColName,
            KeywordCategory::TypeFuncName,
        ])
    }

    pub(super) fn consume_non_reserved_word_or_sconst(&mut self) -> Option<std::string::String> {
        if self.at(TokenKind::SConst) {
            self.consume_string_like()
        } else {
            self.consume_non_reserved_word()
        }
    }

    pub(super) fn consume_identifier_in_categories(
        &mut self,
        categories: &[KeywordCategory],
    ) -> Option<std::string::String> {
        let token = self.peek().clone();
        let accepted = matches!(token.kind, TokenKind::Ident | TokenKind::UIdent)
            || match &token.value {
                Some(TokenValue::Keyword(word)) => lookup_keyword(word)
                    .is_some_and(|keyword| categories.contains(&keyword.category)),
                _ => false,
            };
        if accepted {
            self.advance();
            token_name(&token)
        } else {
            None
        }
    }

    pub(super) fn consume_object_type(&mut self) -> Option<ObjectType> {
        let ty = match self.peek_kind() {
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.advance();
                return Some(ObjectType::EventTrigger);
            }
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.advance();
                return Some(ObjectType::Propgraph);
            }
            TokenKind::TextP
                if self.peek_kind_n(1) == TokenKind::Search
                    && matches!(
                        self.peek_kind_n(2),
                        TokenKind::Parser
                            | TokenKind::Dictionary
                            | TokenKind::Template
                            | TokenKind::Configuration
                    ) =>
            {
                self.advance();
                self.advance();
                let ty = match self.advance().kind {
                    TokenKind::Parser => ObjectType::Tsparser,
                    TokenKind::Dictionary => ObjectType::Tsdictionary,
                    TokenKind::Template => ObjectType::Tstemplate,
                    TokenKind::Configuration => ObjectType::Tsconfiguration,
                    _ => unreachable!(),
                };
                return Some(ty);
            }
            TokenKind::Procedural if self.peek_kind_n(1) == TokenKind::Language => {
                self.advance();
                self.advance();
                return Some(ObjectType::Language);
            }
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
                if self.peek_kind_n(1) == TokenKind::Table {
                    self.advance();
                    self.advance();
                    ObjectType::ForeignTable
                } else if self.peek_kind_n(1) == TokenKind::DataP
                    && self.peek_kind_n(2) == TokenKind::Wrapper
                {
                    self.advance();
                    self.advance();
                    self.advance();
                    ObjectType::Fdw
                } else {
                    return None;
                }
            }
            TokenKind::Materialized => {
                if self.peek_kind_n(1) != TokenKind::View {
                    return None;
                }
                self.advance();
                self.advance();
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

    pub(super) fn consume_auth_ident(&mut self) -> Option<RoleSpec> {
        if self.consume(TokenKind::User) {
            Some(RoleSpec {
                node_tag: NodeTag::RoleSpec,
                roletype: RoleSpecType::CurrentUser,
                location: self.previous_location() as ParseLoc,
                ..RoleSpec::default()
            })
        } else {
            self.consume_role_spec()
        }
    }

    pub(super) fn consume_role_spec(&mut self) -> Option<RoleSpec> {
        let start = self.pos;
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
        self.consume_non_reserved_word().and_then(|rolename| {
            if rolename == "none" {
                self.pos = start;
                return None;
            }
            let roletype = if rolename == "public" {
                RoleSpecType::Public
            } else {
                roletype
            };
            Some(RoleSpec {
                node_tag: NodeTag::RoleSpec,
                roletype,
                rolename: (roletype == RoleSpecType::Cstring).then_some(rolename),
                location: location as ParseLoc,
            })
        })
    }

    pub(super) fn consume_role_id(&mut self) -> PResult<Option<std::string::String>> {
        let location = self.location();
        let Some(role) = self.consume_role_spec() else {
            return Ok(None);
        };
        if role.roletype != RoleSpecType::Cstring {
            let name = role
                .rolename
                .as_deref()
                .unwrap_or_else(|| match role.roletype {
                    RoleSpecType::CurrentRole => "current_role",
                    RoleSpecType::CurrentUser => "current_user",
                    RoleSpecType::SessionUser => "session_user",
                    RoleSpecType::Public => "public",
                    RoleSpecType::Cstring => unreachable!(),
                });
            return Err(ParseError::new(
                location,
                format!("{name} cannot be used as a role name here"),
            ));
        }
        Ok(role.rolename)
    }

    pub(super) fn looks_like_alter_enum(&self) -> bool {
        if self.peek_kind() != TokenKind::TypeP {
            return false;
        }
        self.top_level_adjacent(TokenKind::AddP, TokenKind::ValueP)
            || self.top_level_adjacent(TokenKind::Rename, TokenKind::ValueP)
            || self.top_level_adjacent(TokenKind::Drop, TokenKind::ValueP)
    }

    pub(super) fn looks_like_rename_stmt(&self) -> bool {
        if self.peek_kind() == TokenKind::TypeP
            && self.top_level_adjacent(TokenKind::Rename, TokenKind::ValueP)
        {
            return false;
        }
        self.top_level_contains(TokenKind::Rename)
    }

    pub(super) fn looks_like_alter_object_depends_stmt(&self) -> bool {
        self.top_level_contains(TokenKind::Depends)
    }

    pub(super) fn looks_like_alter_object_schema_stmt(&self) -> bool {
        self.top_level_adjacent(TokenKind::Set, TokenKind::Schema)
    }

    pub(super) fn looks_like_alter_owner_stmt(&self) -> bool {
        if !self.top_level_adjacent(TokenKind::Owner, TokenKind::To) {
            return false;
        }
        !matches!(
            (self.peek_kind(), self.peek_kind_n(1)),
            (TokenKind::Table, _)
                | (TokenKind::View, _)
                | (TokenKind::Materialized, TokenKind::View)
                | (TokenKind::Foreign, TokenKind::Table)
        )
    }

    pub(super) fn looks_like_alter_composite_type(&self) -> bool {
        self.peek_kind() == TokenKind::TypeP
            && self.top_level_contains(TokenKind::Attribute)
            && (self.top_level_contains(TokenKind::AddP)
                || self.top_level_contains(TokenKind::Drop)
                || self.top_level_contains(TokenKind::Alter))
    }

    pub(super) fn top_level_contains(&self, needle: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .any(|kind| kind == needle)
    }

    pub(super) fn top_level_adjacent(&self, first: TokenKind, second: TokenKind) -> bool {
        self.top_level_kinds()
            .windows(2)
            .any(|pair| pair == [first, second])
    }

    pub(super) fn top_level_kinds(&self) -> Vec<TokenKind> {
        let mut kinds = Vec::new();
        let mut depth = 0usize;
        let mut i = self.pos;
        while i < self.end {
            let token = &self.tokens[i];
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

    pub(super) fn consume_if_exists(&mut self) -> PResult<bool> {
        if self.consume(TokenKind::IfP) {
            self.expect(TokenKind::Exists)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn consume_if_not_exists(&mut self) -> PResult<bool> {
        if self.consume(TokenKind::IfP) {
            self.expect(TokenKind::Not)?;
            self.expect(TokenKind::Exists)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn parse_drop_behavior(&mut self) -> DropBehavior {
        if self.consume(TokenKind::Cascade) {
            DropBehavior::Cascade
        } else {
            self.consume(TokenKind::Restrict);
            DropBehavior::Restrict
        }
    }

    pub(super) fn consume_setting_name(&mut self) -> Option<std::string::String> {
        let start = self.pos;
        let mut parts = vec![self.consume_col_id()?];
        while self.consume(TokenKind::Char('.')) {
            let Some(part) = self.consume_col_id() else {
                self.pos = start;
                return None;
            };
            parts.push(part);
        }
        Some(parts.join("."))
    }

    pub(super) fn consume_string_like(&mut self) -> Option<std::string::String> {
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

    pub(super) fn consume_opt_boolean_or_string(&mut self) -> Option<std::string::String> {
        let token = self.peek().clone();
        let accepted = matches!(
            token.kind,
            TokenKind::SConst | TokenKind::TrueP | TokenKind::FalseP | TokenKind::On
        ) || token_name_in_categories(
            &token,
            &[
                KeywordCategory::Unreserved,
                KeywordCategory::ColName,
                KeywordCategory::TypeFuncName,
            ],
        )
        .is_some();
        if !accepted {
            return None;
        }
        let value = token_name(&token)?;
        self.advance();
        Some(value)
    }

    pub(super) fn consume_required_string(
        &mut self,
        message: &str,
    ) -> PResult<std::string::String> {
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here(message));
        }
        self.consume_string_like()
            .ok_or_else(|| self.error_here(message))
    }
}
