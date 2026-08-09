//! PostgreSQL name categories, qualified identities, aliases, and role names.
//!
//! This module is the central seam for keyword-category-sensitive name parsing;
//! callers choose the required grammar instead of accepting arbitrary identifiers.

use super::*;

impl Parser {
    pub(super) fn parse_access_method_name(&mut self) -> PResult<std::string::String> {
        self.record_completion_slot(completion::GrammarSlot::AccessMethod);
        self.consume_col_id()
            .ok_or_else(|| self.error_here("USING requires an access method"))
    }

    pub(super) fn parse_optional_constraint_name(
        &mut self,
    ) -> PResult<Option<std::string::String>> {
        if !self.consume(TokenKind::Constraint) {
            return Ok(None);
        }
        self.record_completion_slot(completion::GrammarSlot::Constraint);
        self.consume_col_id()
            .map(Some)
            .ok_or_else(|| self.error_here("CONSTRAINT requires a name"))
    }

    pub(super) fn parse_optional_tablespace_name(
        &mut self,
    ) -> PResult<Option<std::string::String>> {
        if !self.consume(TokenKind::Tablespace) {
            return Ok(None);
        }
        self.record_completion_slot(completion::GrammarSlot::Tablespace);
        self.consume_col_id()
            .map(Some)
            .ok_or_else(|| self.error_here("TABLESPACE requires a name"))
    }

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
        self.record_completion_qualified_name_slot(slot, stops);
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

    pub(super) fn parse_name_list_until_keywords_allow_initial_stop(
        &mut self,
        stops: &[TokenKind],
    ) -> NodeList {
        if self.at_completion() || !self.at_any(stops) {
            return self.parse_name_list_until_keywords(stops);
        }
        let Some(first) = self.consume_col_id() else {
            return Vec::new();
        };
        let mut parts = vec![first];
        while self.at(TokenKind::Char('.')) {
            let separator_position = self.pos;
            self.advance();
            if let Some(name) = self.consume_col_label() {
                parts.push(name);
            } else {
                self.pos = separator_position;
                break;
            }
        }
        self.record_completion_tokens(&[TokenKind::Char('.')]);
        parts.into_iter().map(make_string_node).collect()
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

    pub(super) fn parse_relation_expr(&mut self) -> PResult<RangeVar> {
        self.parse_relation_expr_with_alias_and_slot(false, completion::GrammarSlot::Relation)
    }

    pub(super) fn parse_relation_expr_with_alias(&mut self) -> PResult<RangeVar> {
        self.parse_relation_expr_with_alias_and_slot(true, completion::GrammarSlot::Relation)
    }

    pub(super) fn parse_relation_expr_with_slot(
        &mut self,
        slot: completion::GrammarSlot,
    ) -> PResult<RangeVar> {
        self.parse_relation_expr_with_alias_and_slot(false, slot)
    }

    fn parse_relation_expr_with_alias_and_slot(
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
            let separator_position = self.pos;
            self.advance();
            if self.at(TokenKind::Char('*')) {
                break;
            }
            if let Some(name) = self.consume_col_label() {
                parts.push(name);
            } else {
                self.pos = separator_position;
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
        self.record_completion_tokens(&[TokenKind::Char('.')]);
        while self.at(TokenKind::Char('.')) {
            let separator_position = self.pos;
            self.advance();
            let Some(name) = self.consume_col_label() else {
                self.pos = separator_position;
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
        self.record_completion_tokens(&[TokenKind::Char('.')]);
        while parts.len() < 3 && self.at(TokenKind::Char('.')) {
            let separator_position = self.pos;
            self.advance();
            if self.at(TokenKind::Char('*')) {
                self.pos = separator_position;
                break;
            }
            if self.at_completion() {
                self.record_completion_slot(slot);
                self.pos = separator_position;
                break;
            }
            let Some(name) = self.consume_col_label() else {
                self.pos = separator_position;
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
            return self
                .recover_completion_hole()
                .and_then(|token| token_name(&token));
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
            return self
                .recover_completion_hole()
                .and_then(|token| token_name(&token));
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

    pub(super) fn consume_auth_ident(&mut self) -> Option<RoleSpec> {
        if self.consume(TokenKind::User) {
            Some(RoleSpec {
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
        let role_start = self.pos;
        let location = self.location();
        let roletype = match self.peek_kind() {
            TokenKind::CurrentRole => {
                self.advance();
                return Some(RoleSpec {
                    roletype: RoleSpecType::CurrentRole,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            TokenKind::CurrentUser => {
                self.advance();
                return Some(RoleSpec {
                    roletype: RoleSpecType::CurrentUser,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            TokenKind::SessionUser => {
                self.advance();
                return Some(RoleSpec {
                    roletype: RoleSpecType::SessionUser,
                    location: location as ParseLoc,
                    ..RoleSpec::default()
                });
            }
            _ => RoleSpecType::Cstring,
        };
        let rolename = self.consume_non_reserved_word()?;
        if rolename == "none" {
            self.pos = role_start;
            return None;
        }
        let roletype = if rolename == "public" {
            RoleSpecType::Public
        } else {
            roletype
        };
        Some(RoleSpec {
            roletype,
            rolename: (roletype == RoleSpecType::Cstring).then_some(rolename),
            location: location as ParseLoc,
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

    pub(super) fn consume_setting_name(&mut self) -> Option<std::string::String> {
        self.record_completion_slot(completion::GrammarSlot::AnyName);
        let setting_start = self.pos;
        let mut parts = vec![self.consume_col_id()?];
        while self.consume(TokenKind::Char('.')) {
            let Some(part) = self.consume_col_id() else {
                self.pos = setting_start;
                return None;
            };
            parts.push(part);
        }
        Some(parts.join("."))
    }
}
