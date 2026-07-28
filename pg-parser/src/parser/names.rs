use super::*;

impl Parser {
    pub(super) fn parse_simple_name_list_until(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            if !self.at_completion() && self.at_any(stops) {
                break;
            }
            self.record_completion_slot(slot);
            names.push(make_string_node(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("expected a name"))?,
            ));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected a name after ','"));
            }
        }
        if names.is_empty() {
            return Err(self.error_here("expected at least one name"));
        }
        Ok(names)
    }

    pub(super) fn parse_one_any_name_with_slot(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<Node> {
        self.record_completion_slot(slot);
        self.record_completion_slot_before(slot, stops);
        if !self.at_completion() && self.at_any(stops) {
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

    pub(super) fn parse_any_name_list_until_with_slot(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            names.push(self.parse_one_any_name_with_slot(stops, slot)?);
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at_completion() && self.at_any(stops) {
                return Err(self.error_here("expected a qualified name after ','"));
            }
        }
        Ok(names)
    }

    pub(super) fn parse_function_expression(&mut self) -> PResult<Node> {
        self.record_completion_slot(completion::GrammarSlot::Function);
        let start = self.pos;
        let remaining = &self.tokens[start..];
        let open = find_top_level_token(remaining, TokenKind::Char('('))
            .ok_or_else(|| self.error_here("function expression requires '('"))?;
        let close = match find_matching_close(remaining, open) {
            Some(close) => close,
            None => remaining
                .iter()
                .position(|token| token.kind == TokenKind::Completion)
                .ok_or_else(|| self.error_here("unterminated function expression"))?,
        };
        let expression = parse_expression_tokens_with_completion(
            remaining[..=close].to_vec(),
            self.completion.clone(),
        )?;
        if !is_function_expression_node(&expression) {
            return Err(self.error_here("expected a function expression"));
        }
        self.pos = start + close + 1;
        Ok(expression)
    }

    pub(super) fn parse_name_list(&mut self) -> NodeList {
        let parts = self.consume_name_parts();
        if !parts.is_empty() {
            self.record_completion_tokens(&[TokenKind::Char('.')]);
        }
        parts.into_iter().map(make_string_node).collect()
    }

    pub(super) fn parse_func_name_list(&mut self) -> NodeList {
        self.consume_func_name_parts()
            .into_iter()
            .map(make_string_node)
            .collect()
    }

    pub(super) fn parse_name_list_until_keywords(&mut self, stops: &[TokenKind]) -> NodeList {
        if !self.at_completion() && self.at_any(stops) {
            Vec::new()
        } else {
            self.parse_name_list()
        }
    }

    pub(super) fn try_parse_range_var_with_slot(
        &mut self,
        allow_set_alias: bool,
        slot: completion::GrammarSlot,
    ) -> PResult<Option<RangeVar>> {
        self.record_completion_slot(slot);
        let location = self.location();
        let parts = self.consume_qualified_name_parts(slot);
        if parts.is_empty() {
            return Ok(None);
        }
        let mut range = range_var_from_parts(parts, location);
        range.alias = self.parse_optional_alias(allow_set_alias)?;
        Ok(Some(range))
    }

    pub(super) fn parse_relation_expr(&mut self, allow_alias: bool) -> PResult<RangeVar> {
        self.parse_relation_expr_with_slot(allow_alias, completion::GrammarSlot::Relation)
    }

    pub(super) fn parse_relation_expr_with_slot(
        &mut self,
        allow_alias: bool,
        slot: completion::GrammarSlot,
    ) -> PResult<RangeVar> {
        self.record_completion_tokens(&[TokenKind::Only]);
        self.record_completion_slot(slot);
        let only = self.consume(TokenKind::Only);
        let parenthesized = only && self.consume(TokenKind::Char('('));
        let mut range = self
            .try_parse_qualified_range_var_with_slot(slot)
            .ok_or_else(|| {
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

    pub(super) fn parse_optional_alias(
        &mut self,
        allow_set_alias: bool,
    ) -> PResult<Option<Box<Alias>>> {
        let has_as = self.consume(TokenKind::As);
        self.record_completion_slot(completion::GrammarSlot::Alias);
        let aliasname = if has_as {
            self.consume_col_id()
                .ok_or_else(|| self.error_here("AS requires an alias"))?
        } else if allow_set_alias || !self.at(TokenKind::Set) {
            let Some(aliasname) = self.consume_col_id() else {
                return Ok(None);
            };
            aliasname
        } else {
            return Ok(None);
        };
        Ok(Some(Box::new(Alias {
            node_tag: NodeTag::Alias,
            aliasname: Some(aliasname),
            ..Alias::default()
        })))
    }

    pub(super) fn parse_optional_alias_clause(&mut self) -> PResult<Option<Box<Alias>>> {
        let Some(mut alias) = self.parse_optional_alias(true)? else {
            return Ok(None);
        };
        if self.consume(TokenKind::Char('(')) {
            alias.colnames = self.parse_parenthesized_name_list_body()?;
            self.expect(TokenKind::Char(')'))?;
        }
        Ok(Some(alias))
    }

    pub(super) fn try_parse_qualified_range_var(&mut self) -> Option<RangeVar> {
        self.try_parse_qualified_range_var_with_slot(completion::GrammarSlot::Relation)
    }

    pub(super) fn try_parse_qualified_range_var_with_slot(
        &mut self,
        slot: completion::GrammarSlot,
    ) -> Option<RangeVar> {
        self.record_completion_slot(slot);
        let location = self.location();
        let parts = self.consume_qualified_name_parts(slot);
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

    pub(super) fn consume_qualified_name_parts(
        &mut self,
        slot: completion::GrammarSlot,
    ) -> Vec<std::string::String> {
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
            if self.at_completion() {
                self.record_completion_slot(slot);
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
        if self.at_completion() {
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            return None;
        }
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
        if self.at_completion() {
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            return None;
        }
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
        self.record_completion_lookahead_tokens(&[
            TokenKind::Access,
            TokenKind::Aggregate,
            TokenKind::Cast,
            TokenKind::Collation,
            TokenKind::ConversionP,
            TokenKind::Database,
            TokenKind::DomainP,
            TokenKind::Event,
            TokenKind::Extension,
            TokenKind::Foreign,
            TokenKind::Function,
            TokenKind::Index,
            TokenKind::Language,
            TokenKind::Materialized,
            TokenKind::Operator,
            TokenKind::Policy,
            TokenKind::Procedure,
            TokenKind::Procedural,
            TokenKind::Property,
            TokenKind::Publication,
            TokenKind::Routine,
            TokenKind::Rule,
            TokenKind::Schema,
            TokenKind::Sequence,
            TokenKind::Server,
            TokenKind::Statistics,
            TokenKind::Subscription,
            TokenKind::Table,
            TokenKind::Tablespace,
            TokenKind::TextP,
            TokenKind::Transform,
            TokenKind::Trigger,
            TokenKind::TypeP,
            TokenKind::View,
        ]);
        let start = self.pos;
        let ty = match self.peek_kind() {
            TokenKind::Event => {
                self.advance();
                if !self.consume(TokenKind::Trigger) {
                    self.pos = start;
                    return None;
                }
                return Some(ObjectType::EventTrigger);
            }
            TokenKind::Property => {
                self.advance();
                if !self.consume(TokenKind::Graph) {
                    self.pos = start;
                    return None;
                }
                return Some(ObjectType::Propgraph);
            }
            TokenKind::TextP => {
                self.advance();
                if !self.consume(TokenKind::Search) {
                    self.pos = start;
                    return None;
                }
                self.record_completion_lookahead_tokens(&[
                    TokenKind::Parser,
                    TokenKind::Dictionary,
                    TokenKind::Template,
                    TokenKind::Configuration,
                ]);
                let ty = match self.peek_kind() {
                    TokenKind::Parser => ObjectType::Tsparser,
                    TokenKind::Dictionary => ObjectType::Tsdictionary,
                    TokenKind::Template => ObjectType::Tstemplate,
                    TokenKind::Configuration => ObjectType::Tsconfiguration,
                    _ => {
                        self.pos = start;
                        return None;
                    }
                };
                self.advance();
                return Some(ty);
            }
            TokenKind::Procedural => {
                self.advance();
                if !self.consume(TokenKind::Language) {
                    self.pos = start;
                    return None;
                }
                return Some(ObjectType::Language);
            }
            TokenKind::Access => {
                self.advance();
                if !self.consume(TokenKind::Method) {
                    self.pos = start;
                    return None;
                }
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
                    if !self.consume(TokenKind::Wrapper) {
                        self.pos = start;
                        return None;
                    }
                    ObjectType::Fdw
                } else {
                    self.pos = start;
                    return None;
                }
            }
            TokenKind::Materialized => {
                self.advance();
                if !self.consume(TokenKind::View) {
                    self.pos = start;
                    return None;
                }
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
                | ObjectType::EventTrigger
                | ObjectType::Propgraph
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Language
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
            self.consume_role_spec_with_slot_and_specials(
                completion::GrammarSlot::Role,
                &[TokenKind::CurrentRole, TokenKind::CurrentUser],
            )
            .filter(|role| role.roletype != RoleSpecType::SessionUser)
        }
    }

    pub(super) fn consume_role_spec(&mut self) -> Option<RoleSpec> {
        self.consume_role_spec_with_slot(completion::GrammarSlot::Role)
    }

    pub(super) fn consume_role_spec_without_special_suggestions(&mut self) -> Option<RoleSpec> {
        self.consume_role_spec_with_slot_and_specials(completion::GrammarSlot::Role, &[])
    }

    fn consume_role_spec_with_slot(&mut self, slot: completion::GrammarSlot) -> Option<RoleSpec> {
        self.consume_role_spec_with_slot_and_specials(
            slot,
            &[
                TokenKind::CurrentRole,
                TokenKind::CurrentUser,
                TokenKind::SessionUser,
            ],
        )
    }

    fn consume_role_spec_with_slot_and_specials(
        &mut self,
        slot: completion::GrammarSlot,
        suggested_specials: &[TokenKind],
    ) -> Option<RoleSpec> {
        self.record_completion_slot(slot);
        self.record_completion_lookahead_tokens(suggested_specials);
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
        self.consume_role_id_with_slot(completion::GrammarSlot::Role)
    }

    pub(super) fn consume_new_role_id(&mut self) -> PResult<Option<std::string::String>> {
        self.consume_role_id_with_slot(completion::GrammarSlot::AnyName)
    }

    fn consume_role_id_with_slot(
        &mut self,
        slot: completion::GrammarSlot,
    ) -> PResult<Option<std::string::String>> {
        let location = self.location();
        let Some(role) = self.consume_role_spec_with_slot_and_specials(slot, &[]) else {
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
            return Err(ParseError::syntax_exit(
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
        let kinds = self.top_level_kinds();
        let Some(action) = kinds.iter().position(|kind| {
            matches!(
                kind,
                TokenKind::AddP
                    | TokenKind::Rename
                    | TokenKind::Drop
                    | TokenKind::Alter
                    | TokenKind::Set
            )
        }) else {
            return false;
        };
        matches!(
            kinds.get(action),
            Some(TokenKind::AddP | TokenKind::Rename | TokenKind::Drop)
        ) && matches!(
            kinds.get(action + 1),
            Some(TokenKind::ValueP | TokenKind::Completion)
        )
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
        if !self.top_level_adjacent(TokenKind::Owner, TokenKind::To)
            && !self.top_level_precedes_completion(TokenKind::Owner)
        {
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
            && ((self.top_level_contains(TokenKind::Attribute)
                && (self.top_level_contains(TokenKind::AddP)
                    || self.top_level_contains(TokenKind::Drop)
                    || self.top_level_contains(TokenKind::Alter)))
                || self.top_level_precedes_completion(TokenKind::Alter))
    }

    pub(super) fn top_level_contains(&self, needle: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .any(|kind| kind == needle)
    }

    pub(super) fn top_level_adjacent(&self, first: TokenKind, second: TokenKind) -> bool {
        self.top_level_kinds()
            .into_iter()
            .filter(|kind| *kind != TokenKind::Completion)
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == [first, second])
    }

    fn top_level_precedes_completion(&self, kind: TokenKind) -> bool {
        self.top_level_kinds()
            .windows(2)
            .any(|pair| pair == [kind, TokenKind::Completion])
    }

    pub(super) fn top_level_kinds(&self) -> Vec<TokenKind> {
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

    pub(super) fn consume_if_exists(&mut self) -> PResult<bool> {
        self.consume_phrase(&[TokenKind::IfP, TokenKind::Exists])
    }

    pub(super) fn consume_if_not_exists(&mut self) -> PResult<bool> {
        self.consume_phrase(&[TokenKind::IfP, TokenKind::Not, TokenKind::Exists])
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
        self.record_completion_slot(completion::GrammarSlot::AnyName);
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
        self.record_completion_tokens(&[
            TokenKind::SConst,
            TokenKind::TrueP,
            TokenKind::FalseP,
            TokenKind::On,
        ]);
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
        self.record_completion_tokens(&[TokenKind::SConst]);
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here(message));
        }
        self.consume_string_like()
            .ok_or_else(|| self.error_here(message))
    }
}
