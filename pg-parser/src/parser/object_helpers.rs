//! Shared object identities, object types, signatures, and DDL clauses.
//!
//! These helpers retain strict object-specific seams while centralizing balanced
//! token fragments, recurring `DefElem` shapes, and common object clauses such as
//! `IF EXISTS` and drop behavior.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DefElemValueGrammar {
    Generic,
    Subscription,
}

pub(super) fn parse_aggregate_with_args_tokens(
    tokens: Vec<Token>,
    location: usize,
) -> PResult<ObjectWithArgs> {
    let open = find_top_level_token(&tokens, TokenKind::Char('('))
        .ok_or_else(|| ParseError::syntax_exit(location, "aggregate requires argument types"))?;
    let close = find_matching_close(&tokens, open).ok_or_else(|| {
        ParseError::ranged(tokens[open].range, "unterminated aggregate arguments")
    })?;
    if close + 1 != tokens.len() {
        return Err(ParseError::ranged(
            tokens[close + 1].range,
            "unexpected token after aggregate signature",
        ));
    }
    let name_tokens = tokens[..open].to_vec();
    if name_tokens
        .iter()
        .rev()
        .find(|token| token.kind != TokenKind::Char('.'))
        .is_some_and(|token| is_operator_name_kind(token.kind))
    {
        return Err(ParseError::syntax_exit(
            location,
            "aggregate requires a function name",
        ));
    }
    let name = parse_object_with_args_tokens(name_tokens, location)?;
    let mut parsed_args = parse_aggregate_args(tokens[open + 1..close].to_vec())?;
    let parameters = match parsed_args.remove(0) {
        Node::AArrayExpr(list) => list.elements,
        _ => unreachable!("aggregate argument parser returned a non-list"),
    };
    let objargs = parameters
        .iter()
        .map(|parameter| match parameter {
            Node::FunctionParameter(parameter) => parameter
                .arg_type
                .as_deref()
                .cloned()
                .map(Node::TypeName)
                .map(Some)
                .unwrap_or(None),
            _ => unreachable!("aggregate argument parser returned a non-parameter"),
        })
        .collect();
    Ok(ObjectWithArgs {
        node_tag: NodeTag::ObjectWithArgs,
        objname: name.objname,
        objargs,
        objfuncargs: parameters,
        args_unspecified: false,
    })
}

pub(super) fn parse_operator_with_args_tokens(
    tokens: Vec<Token>,
    location: usize,
) -> PResult<ObjectWithArgs> {
    let open = find_top_level_token(&tokens, TokenKind::Char('('))
        .ok_or_else(|| ParseError::syntax_exit(location, "operator requires argument types"))?;
    validate_operator_name_tokens(&tokens[..open], location)?;
    let signature = parse_object_with_args_tokens_impl(tokens, location, true)?;
    if signature.objargs.len() != 2 {
        return Err(ParseError::syntax_exit(
            location,
            "operator signatures require two argument positions",
        ));
    }
    Ok(signature)
}

pub(super) fn validate_operator_name_tokens(tokens: &[Token], location: usize) -> PResult<()> {
    let name_end = tokens
        .iter()
        .rposition(|token| token.kind != TokenKind::Char('.'))
        .ok_or_else(|| ParseError::syntax_exit(location, "operator requires a name"))?;
    if !is_operator_name_kind(tokens[name_end].kind) {
        return Err(ParseError::ranged(
            tokens[name_end].range,
            "operator name must end with an operator",
        ));
    }
    Ok(())
}

pub(super) fn parse_qualified_all_operator_tokens(
    tokens: Vec<Token>,
    location: usize,
) -> PResult<NodeList> {
    if tokens.first().map(|token| token.kind) != Some(TokenKind::Operator)
        || tokens.get(1).map(|token| token.kind) != Some(TokenKind::Char('('))
    {
        return Err(ParseError::syntax_exit(location, "expected OPERATOR(...)"));
    }
    let close = find_matching_close(&tokens, 1)
        .ok_or_else(|| ParseError::syntax_exit(location, "unterminated OPERATOR(...) value"))?;
    if close + 1 != tokens.len() {
        return Err(ParseError::ranged(
            tokens[close + 1].range,
            "unexpected token after OPERATOR(...) value",
        ));
    }
    let operator = tokens[2..close].to_vec();
    validate_operator_name_tokens(&operator, location)?;
    parse_operator_name_tokens(operator, location)
}

pub(super) fn parse_object_with_args_tokens(
    tokens: Vec<Token>,
    location: usize,
) -> PResult<ObjectWithArgs> {
    parse_object_with_args_tokens_impl(tokens, location, false)
}

fn parse_object_with_args_tokens_impl(
    tokens: Vec<Token>,
    location: usize,
    allow_operator: bool,
) -> PResult<ObjectWithArgs> {
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "expected an object signature",
        ));
    }
    let open = find_top_level_token(&tokens, TokenKind::Char('('));
    let (name_tokens, arg_tokens, args_unspecified) = if let Some(open) = open {
        let close = find_matching_close(&tokens, open)
            .ok_or_else(|| ParseError::ranged(tokens[open].range, "unterminated argument list"))?;
        if close + 1 != tokens.len() {
            return Err(ParseError::ranged(
                tokens[close + 1].range,
                "unexpected token after object signature",
            ));
        }
        (
            tokens[..open].to_vec(),
            tokens[open + 1..close].to_vec(),
            false,
        )
    } else {
        (tokens, Vec::new(), true)
    };
    if name_tokens.is_empty() {
        return Err(ParseError::syntax_exit(location, "expected an object name"));
    }
    let mut objname = Vec::new();
    let mut expect_component = true;
    let qualified = name_tokens
        .iter()
        .any(|token| token.kind == TokenKind::Char('.'));
    let mut component_index = 0usize;
    for (index, token) in name_tokens.iter().enumerate() {
        if token.kind == TokenKind::Char('.') {
            if expect_component {
                return Err(ParseError::ranged(
                    token.range,
                    "invalid qualified object name",
                ));
            }
            expect_component = true;
            continue;
        }
        if !expect_component {
            return Err(ParseError::ranged(
                token.range,
                "object name components must be separated by '.'",
            ));
        }
        let categories: &[KeywordCategory] = if component_index == 0 && !qualified {
            &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName]
        } else if component_index == 0 {
            &[KeywordCategory::Unreserved, KeywordCategory::ColName]
        } else {
            &[
                KeywordCategory::Unreserved,
                KeywordCategory::ColName,
                KeywordCategory::TypeFuncName,
                KeywordCategory::Reserved,
            ]
        };
        let value = token_name_in_categories(token, categories).or_else(|| {
            (is_operator_name_kind(token.kind) && index + 1 == name_tokens.len())
                .then(|| token_text(token))
        });
        objname.push(make_string_node(value.ok_or_else(|| {
            ParseError::ranged(token.range, "invalid object name")
        })?));
        component_index += 1;
        expect_component = false;
    }
    if expect_component {
        return Err(ParseError::syntax_exit(
            location,
            "qualified object name cannot end with '.'",
        ));
    }

    let mut objargs = Vec::new();
    let mut objfuncargs = Vec::new();
    let operator_signature = name_tokens
        .iter()
        .rev()
        .find(|token| token.kind != TokenKind::Char('.'))
        .is_some_and(|token| is_operator_name_kind(token.kind));
    if operator_signature && !allow_operator {
        return Err(ParseError::syntax_exit(
            location,
            "function signatures require a function name",
        ));
    }
    if !arg_tokens.is_empty() {
        let trailing_comma =
            arg_tokens.last().map(|token| token.kind) == Some(TokenKind::Char(','));
        let chunks = split_top_level_commas(arg_tokens);
        if trailing_comma || chunks.iter().any(Vec::is_empty) {
            return Err(ParseError::syntax_exit(
                location,
                "invalid object argument list",
            ));
        }
        if operator_signature {
            if chunks.len() != 2 {
                return Err(ParseError::syntax_exit(
                    location,
                    "operator signatures require two argument positions",
                ));
            }
            for chunk in chunks {
                let arg_location = chunk.first().map_or(location, |token| token.location());
                if chunk.len() == 1 && chunk[0].kind == TokenKind::None {
                    objargs.push(None);
                } else {
                    let type_name = parse_type_name_tokens(chunk).map_err(|_| {
                        ParseError::syntax_exit(arg_location, "invalid operator argument type")
                    })?;
                    objargs.push(Some(Node::TypeName(type_name)));
                }
            }
            if objargs.iter().all(Option::is_none) {
                return Err(ParseError::syntax_exit(
                    location,
                    "operator signatures cannot omit both arguments",
                ));
            }
        } else {
            for chunk in chunks {
                let arg_location = chunk.first().map_or(location, |token| token.location());
                let parameter = function_parameter_from_tokens(chunk).map_err(|_| {
                    ParseError::syntax_exit(arg_location, "invalid function argument")
                })?;
                if parameter.defexpr.is_some() {
                    return Err(ParseError::syntax_exit(
                        arg_location,
                        "function signatures cannot contain default values",
                    ));
                }
                let type_name = parameter.arg_type.as_deref().cloned().ok_or_else(|| {
                    ParseError::syntax_exit(arg_location, "invalid argument type")
                })?;
                objargs.push(Some(Node::TypeName(type_name)));
                objfuncargs.push(Node::FunctionParameter(parameter));
            }
        }
    }
    Ok(ObjectWithArgs {
        node_tag: NodeTag::ObjectWithArgs,
        objname,
        objargs,
        objfuncargs,
        args_unspecified,
    })
}

impl Parser {
    pub(super) fn parse_type_name_until(&mut self, stops: &[TokenKind]) -> Option<TypeName> {
        self.record_completion_slot_within_fragment(completion::GrammarSlot::Type, stops);
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        if self.at_completion() {
            let mut completion_tokens = tokens.clone();
            self.append_completion_marker(&mut completion_tokens);
            record_type_name_completion(&completion_tokens, self.completion.as_ref());
        }
        tokens_to_type_name(tokens).map(|mut type_name| {
            type_name.location = location as ParseLoc;
            type_name
        })
    }

    pub(super) fn parse_object_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        self.parse_object_with_args_until_with_slot(stops, completion::GrammarSlot::Function)
    }

    pub(super) fn parse_object_with_args_until_with_slot(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<ObjectWithArgs> {
        self.record_completion_slot(slot);
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        self.record_signature_fragment_slot(slot, &tokens);
        parse_object_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_operator_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        self.record_completion_slot(completion::GrammarSlot::Operator);
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        self.record_signature_fragment_slot(completion::GrammarSlot::Operator, &tokens);
        parse_operator_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_aggregate_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        self.record_completion_slot(completion::GrammarSlot::Aggregate);
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        self.record_signature_fragment_slot(completion::GrammarSlot::Aggregate, &tokens);
        parse_aggregate_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_aggregate_with_args_list_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        self.record_completion_slot(completion::GrammarSlot::Aggregate);
        if self.at_completion() {
            return Err(self.error_here("expected an aggregate signature"));
        }
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            self.record_signature_fragment_slot(completion::GrammarSlot::Aggregate, &tokens);
            objects.push(Node::ObjectWithArgs(parse_aggregate_with_args_tokens(
                tokens, location,
            )?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an aggregate signature after ','"));
            }
        }
        Ok(objects)
    }

    pub(super) fn parse_operator_with_args_list_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        self.record_completion_slot(completion::GrammarSlot::Operator);
        if self.at_completion() {
            return Err(self.error_here("expected an operator signature"));
        }
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            self.record_signature_fragment_slot(completion::GrammarSlot::Operator, &tokens);
            objects.push(Node::ObjectWithArgs(parse_operator_with_args_tokens(
                tokens, location,
            )?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an operator signature after ','"));
            }
        }
        Ok(objects)
    }

    pub(super) fn parse_opclass_operator_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        self.record_signature_fragment_slot(completion::GrammarSlot::Operator, &tokens);
        if find_top_level_token(&tokens, TokenKind::Char('(')).is_some() {
            parse_operator_with_args_tokens(tokens, location)
        } else {
            validate_operator_name_tokens(&tokens, location)?;
            let mut signature = parse_object_with_args_tokens_impl(tokens, location, true)?;
            signature.args_unspecified = false;
            Ok(signature)
        }
    }

    pub(super) fn parse_object_with_args_list_until_with_slot(
        &mut self,
        stops: &[TokenKind],
        slot: completion::GrammarSlot,
    ) -> PResult<NodeList> {
        self.record_completion_slot(slot);
        if self.at_completion() {
            return Err(self.error_here("expected a routine signature"));
        }
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
            self.record_signature_fragment_slot(slot, &tokens);
            objects.push(Node::ObjectWithArgs(parse_object_with_args_tokens(
                tokens, location,
            )?));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at_any(stops) {
                return Err(self.error_here("expected an object signature after ','"));
            }
        }
        Ok(objects)
    }

    fn record_signature_fragment_slot(&self, name_slot: completion::GrammarSlot, tokens: &[Token]) {
        if !self.at_completion() {
            return;
        }
        let mut depth = 0usize;
        let mut saw_open = false;
        for token in tokens {
            match token.kind {
                TokenKind::Char('(') => {
                    saw_open = true;
                    depth += 1;
                }
                TokenKind::Char(')') => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if depth != 0 {
            self.record_completion_slot(completion::GrammarSlot::Type);
            if let Some(open) = find_top_level_token(tokens, TokenKind::Char('(')) {
                let arguments = &tokens[open + 1..];
                if name_slot == completion::GrammarSlot::Aggregate
                    && !arguments.iter().any(|token| token.kind == TokenKind::Order)
                    && parse_aggregate_args(arguments.to_vec()).is_ok()
                {
                    self.record_completion_phrase(&[TokenKind::Order, TokenKind::By]);
                }
                if name_slot != completion::GrammarSlot::Operator {
                    let mut nested_depth = 0usize;
                    let mut active_start = 0usize;
                    for (index, token) in arguments.iter().enumerate() {
                        match token.kind {
                            TokenKind::Char('(') | TokenKind::Char('[') => nested_depth += 1,
                            TokenKind::Char(')') | TokenKind::Char(']') => {
                                nested_depth = nested_depth.saturating_sub(1)
                            }
                            TokenKind::Char(',') if nested_depth == 0 => active_start = index + 1,
                            _ => {}
                        }
                    }
                    let mut active_parameter = arguments[active_start..].to_vec();
                    active_parameter.push(self.peek().clone());
                    let _ = function_parameter_from_tokens_with_completion(
                        active_parameter,
                        self.completion.clone(),
                    );
                }
            }
            if depth == 1 && tokens.last().map(|token| token.kind) != Some(TokenKind::Char(',')) {
                self.record_completion_tokens(&[TokenKind::Char(')')]);
                if tokens.last().map(|token| token.kind) != Some(TokenKind::Char('(')) {
                    self.record_completion_tokens(&[TokenKind::Char(',')]);
                }
            }
        } else if tokens.is_empty()
            || tokens.last().map(|token| token.kind) == Some(TokenKind::Char('.'))
        {
            self.record_completion_slot(name_slot);
        } else if !saw_open {
            self.record_completion_tokens(&[TokenKind::Char('(')]);
        }
    }

    pub(super) fn parse_parenthesized_def_elem_list_strict(&mut self) -> PResult<NodeList> {
        self.parse_parenthesized_def_elem_list_with(DefElemValueGrammar::Generic)
    }

    pub(super) fn parse_subscription_option_list(&mut self) -> PResult<NodeList> {
        self.parse_parenthesized_def_elem_list_with(DefElemValueGrammar::Subscription)
    }

    fn parse_parenthesized_def_elem_list_with(
        &mut self,
        value_grammar: DefElemValueGrammar,
    ) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("option list cannot be empty"));
        }
        let mut defs = Vec::new();
        loop {
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            self.record_def_elem_value_candidates(value_grammar, &tokens);
            if self.at_completion() && tokens.len() == 1 && token_name(&tokens[0]).is_some() {
                self.record_completion_follow_tokens(&[TokenKind::Char('=')]);
            }
            let def = tokens_to_def_elem(tokens, location)?;
            defs.push(Node::DefElem(def));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(defs)
    }

    fn record_def_elem_value_candidates(
        &self,
        value_grammar: DefElemValueGrammar,
        tokens: &[Token],
    ) {
        if !self.at_completion()
            || value_grammar == DefElemValueGrammar::Generic
            || tokens.get(1).map(|token| token.kind) != Some(TokenKind::Char('='))
            || tokens.len() != 2
        {
            return;
        }
        let Some(name) = token_name(&tokens[0]).map(|name| name.to_ascii_lowercase()) else {
            return;
        };
        match name.as_str() {
            "binary" | "copy_data" | "create_slot" | "disable_on_error" | "enabled"
            | "failover" | "password_required" | "refresh" | "run_as_owner" | "streaming"
            | "two_phase" => {
                self.record_completion_tokens(&[TokenKind::TrueP, TokenKind::FalseP]);
            }
            "slot_name" => self.record_completion_tokens(&[TokenKind::None]),
            _ => {}
        }
    }

    pub(super) fn parse_def_elem_list(&mut self) -> PResult<NodeList> {
        self.parse_parenthesized_def_elem_list_strict()
    }

    pub(super) fn parse_parenthesized_utility_option_list(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("utility option list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            self.record_completion_tokens(&[TokenKind::Analyze, TokenKind::Format]);
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let location = self.location();
            let name = if matches!(self.peek_kind(), TokenKind::Analyze | TokenKind::Analyse) {
                self.advance();
                "analyze".to_owned()
            } else if self.peek_kind() == TokenKind::FormatLa {
                self.advance();
                "format".to_owned()
            } else {
                self.consume_non_reserved_word()
                    .ok_or_else(|| self.error_here("expected a utility option name"))?
            };
            let arg = if self.at_any(&[TokenKind::Char(','), TokenKind::Char(')')]) {
                None
            } else if matches!(
                self.peek_kind(),
                TokenKind::IConst | TokenKind::FConst | TokenKind::Char('+') | TokenKind::Char('-')
            ) {
                Some(self.parse_numeric_only()?)
            } else {
                let value = match self.peek_kind() {
                    TokenKind::TrueP => {
                        self.advance();
                        "true".to_owned()
                    }
                    TokenKind::FalseP => {
                        self.advance();
                        "false".to_owned()
                    }
                    TokenKind::On => {
                        self.advance();
                        "on".to_owned()
                    }
                    TokenKind::SConst => self.consume_string_like().unwrap_or_default(),
                    _ => self.consume_non_reserved_word().ok_or_else(|| {
                        self.error_here("expected a utility option boolean, string, or number")
                    })?,
                };
                Some(make_string_node(value))
            };
            if !self.at_completion() && !self.at_any(&[TokenKind::Char(','), TokenKind::Char(')')])
            {
                return Err(self.error_here("unexpected token after utility option"));
            }
            options.push(make_def_elem(&name, arg, location));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a utility option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }

    pub(super) fn parse_parenthesized_reloptions(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("relation option list cannot be empty"));
        }
        let mut options = Vec::new();
        loop {
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let first = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected a relation option name"))?;
            let (defnamespace, defname) = if self.consume(TokenKind::Char('.')) {
                self.record_completion_slot(completion::GrammarSlot::AnyName);
                let second = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("expected a relation option name after '.'"))?;
                (Some(first), second)
            } else {
                (None, first)
            };
            let arg = if self.consume(TokenKind::Char('=')) {
                self.record_completion_tokens(&[
                    TokenKind::IConst,
                    TokenKind::FConst,
                    TokenKind::SConst,
                    TokenKind::TrueP,
                    TokenKind::FalseP,
                    TokenKind::On,
                    TokenKind::Default,
                ]);
                self.record_completion_slot(completion::GrammarSlot::AnyName);
                let tokens =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                if tokens.is_empty() {
                    return Err(self.error_here("relation option '=' requires a value"));
                }
                Some(Box::new(parse_operator_def_arg(
                    &defname, tokens, location,
                )?))
            } else {
                None
            };
            self.record_completion_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
            if !self.at_any(&[TokenKind::Char(','), TokenKind::Char(')')]) {
                return Err(self.error_here("relation option values require '='"));
            }
            options.push(Node::DefElem(DefElem {
                node_tag: NodeTag::DefElem,
                defnamespace,
                defname: Some(defname),
                arg,
                location: location as ParseLoc,
                ..DefElem::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a relation option after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(options)
    }

    pub(super) fn parse_parenthesized_definition(&mut self) -> PResult<NodeList> {
        self.parse_parenthesized_definition_for(None)
    }

    pub(super) fn parse_parenthesized_definition_for(
        &mut self,
        object_type: Option<ObjectType>,
    ) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("definition list cannot be empty"));
        }
        let mut definition = Vec::new();
        loop {
            let location = self.location();
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected a definition name"))?;
            let arg = if self.consume(TokenKind::Char('=')) {
                let value_slot = object_type
                    .and_then(|object_type| completion::definition_value_slot(object_type, &name));
                if let Some(slot) = value_slot {
                    self.record_completion_slot(slot);
                    self.record_completion_slot_within_fragment(
                        slot,
                        &[TokenKind::Char(','), TokenKind::Char(')')],
                    );
                } else {
                    self.record_completion_tokens(&[
                        TokenKind::IConst,
                        TokenKind::FConst,
                        TokenKind::SConst,
                        TokenKind::TrueP,
                        TokenKind::FalseP,
                        TokenKind::On,
                        TokenKind::Default,
                    ]);
                    self.record_completion_slot(completion::GrammarSlot::AnyName);
                }
                let tokens =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                if let Some(slot) = value_slot
                    && self.at_completion()
                    && (tokens.is_empty()
                        || matches!(
                            tokens.last().map(|token| token.kind),
                            Some(TokenKind::Char('.') | TokenKind::Char('('))
                        ))
                {
                    self.record_completion_slot(slot);
                }
                if tokens.is_empty() {
                    return Err(self.error_here("definition '=' requires a value"));
                }
                Some(parse_operator_def_arg(&name, tokens, location)?)
            } else {
                None
            };
            self.record_completion_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
            if !self.at_any(&[TokenKind::Char(','), TokenKind::Char(')')]) {
                return Err(self.error_here("definition values require '='"));
            }
            definition.push(make_def_elem(&name, arg, location));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a definition after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(definition)
    }

    pub(super) fn parse_parenthesized_name_list_body(&mut self) -> PResult<NodeList> {
        let mut names = Vec::new();
        loop {
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("expected a name"))?;
            names.push(make_string_node(name));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(names)
    }

    pub(super) fn parse_relation_name_list_body(&mut self) -> PResult<NodeList> {
        let mut relations = Vec::new();
        loop {
            let relation = self.parse_relation_expr()?;
            relations.push(Node::RangeVar(relation));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
        }
        Ok(relations)
    }

    pub(super) fn parse_trigger_events(&mut self) -> PResult<(i16, NodeList)> {
        let mut events = 0i16;
        let mut columns = Vec::new();
        loop {
            self.record_completion_tokens(&[
                TokenKind::Insert,
                TokenKind::DeleteP,
                TokenKind::Update,
                TokenKind::Truncate,
            ]);
            let event = match self.peek_kind() {
                TokenKind::Insert => {
                    self.advance();
                    4
                }
                TokenKind::DeleteP => {
                    self.advance();
                    8
                }
                TokenKind::Update => {
                    self.advance();
                    if self.consume(TokenKind::Of) {
                        loop {
                            self.record_completion_slot(completion::GrammarSlot::Column);
                            self.request_completion_membership_recovery();
                            let column = self.consume_col_id().ok_or_else(|| {
                                self.error_here("UPDATE OF requires a column name")
                            })?;
                            columns.push(make_string_node(column));
                            if !self.consume(TokenKind::Char(',')) {
                                break;
                            }
                        }
                    }
                    16
                }
                TokenKind::Truncate => {
                    self.advance();
                    32
                }
                _ => return Err(self.error_here("expected a trigger event")),
            };
            if events & event != 0 {
                return Err(self.error_here("duplicate trigger event"));
            }
            events |= event;
            if !self.consume(TokenKind::Or) {
                break;
            }
        }
        Ok((events, columns))
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
        let object_type_start = self.pos;
        let ty = match self.peek_kind() {
            TokenKind::Event => {
                self.advance();
                if !self.consume(TokenKind::Trigger) {
                    self.pos = object_type_start;
                    return None;
                }
                return Some(ObjectType::EventTrigger);
            }
            TokenKind::Property => {
                self.advance();
                if !self.consume(TokenKind::Graph) {
                    self.pos = object_type_start;
                    return None;
                }
                return Some(ObjectType::Propgraph);
            }
            TokenKind::TextP => {
                self.advance();
                if !self.consume(TokenKind::Search) {
                    self.pos = object_type_start;
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
                        self.pos = object_type_start;
                        return None;
                    }
                };
                self.advance();
                return Some(ty);
            }
            TokenKind::Procedural => {
                self.advance();
                if !self.consume(TokenKind::Language) {
                    self.pos = object_type_start;
                    return None;
                }
                return Some(ObjectType::Language);
            }
            TokenKind::Access => {
                self.advance();
                if !self.consume(TokenKind::Method) {
                    self.pos = object_type_start;
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
                        self.pos = object_type_start;
                        return None;
                    }
                    ObjectType::Fdw
                } else {
                    self.pos = object_type_start;
                    return None;
                }
            }
            TokenKind::Materialized => {
                self.advance();
                if !self.consume(TokenKind::View) {
                    self.pos = object_type_start;
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
}
