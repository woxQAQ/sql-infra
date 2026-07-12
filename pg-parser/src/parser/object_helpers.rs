use super::*;

pub(super) fn parse_aggregate_with_args_tokens(
    tokens: Vec<Token>,
    location: usize,
) -> PResult<ObjectWithArgs> {
    let open = find_top_level_token(&tokens, TokenKind::Char('('))
        .ok_or_else(|| ParseError::new(location, "aggregate requires argument types"))?;
    let close = find_matching_close(&tokens, open).ok_or_else(|| {
        ParseError::new(tokens[open].location, "unterminated aggregate arguments")
    })?;
    if close + 1 != tokens.len() {
        return Err(ParseError::new(
            tokens[close + 1].location,
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
        return Err(ParseError::new(
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
        .ok_or_else(|| ParseError::new(location, "operator requires argument types"))?;
    validate_operator_name_tokens(&tokens[..open], location)?;
    let signature = parse_object_with_args_tokens_impl(tokens, location, true)?;
    if signature.objargs.len() != 2 {
        return Err(ParseError::new(
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
        .ok_or_else(|| ParseError::new(location, "operator requires a name"))?;
    if !is_operator_name_kind(tokens[name_end].kind) {
        return Err(ParseError::new(
            tokens[name_end].location,
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
        return Err(ParseError::new(location, "expected OPERATOR(...)"));
    }
    let close = find_matching_close(&tokens, 1)
        .ok_or_else(|| ParseError::new(location, "unterminated OPERATOR(...) value"))?;
    if close + 1 != tokens.len() {
        return Err(ParseError::new(
            tokens[close + 1].location,
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
        return Err(ParseError::new(location, "expected an object signature"));
    }
    let open = find_top_level_token(&tokens, TokenKind::Char('('));
    let (name_tokens, arg_tokens, args_unspecified) = if let Some(open) = open {
        let close = find_matching_close(&tokens, open)
            .ok_or_else(|| ParseError::new(tokens[open].location, "unterminated argument list"))?;
        if close + 1 != tokens.len() {
            return Err(ParseError::new(
                tokens[close + 1].location,
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
        return Err(ParseError::new(location, "expected an object name"));
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
                return Err(ParseError::new(
                    token.location,
                    "invalid qualified object name",
                ));
            }
            expect_component = true;
            continue;
        }
        if !expect_component {
            return Err(ParseError::new(
                token.location,
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
            ParseError::new(token.location, "invalid object name")
        })?));
        component_index += 1;
        expect_component = false;
    }
    if expect_component {
        return Err(ParseError::new(
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
        return Err(ParseError::new(
            location,
            "function signatures require a function name",
        ));
    }
    if !arg_tokens.is_empty() {
        let trailing_comma =
            arg_tokens.last().map(|token| token.kind) == Some(TokenKind::Char(','));
        let chunks = split_top_level_commas(arg_tokens);
        if trailing_comma || chunks.iter().any(Vec::is_empty) {
            return Err(ParseError::new(location, "invalid object argument list"));
        }
        if operator_signature {
            if chunks.len() != 2 {
                return Err(ParseError::new(
                    location,
                    "operator signatures require two argument positions",
                ));
            }
            for chunk in chunks {
                let arg_location = chunk.first().map_or(location, |token| token.location);
                if chunk.len() == 1 && chunk[0].kind == TokenKind::None {
                    objargs.push(None);
                } else {
                    let type_name = parse_type_name_tokens(chunk).map_err(|_| {
                        ParseError::new(arg_location, "invalid operator argument type")
                    })?;
                    objargs.push(Some(Node::TypeName(type_name)));
                }
            }
            if objargs.iter().all(Option::is_none) {
                return Err(ParseError::new(
                    location,
                    "operator signatures cannot omit both arguments",
                ));
            }
        } else {
            for chunk in chunks {
                let arg_location = chunk.first().map_or(location, |token| token.location);
                let parameter = function_parameter_from_tokens(chunk)
                    .map_err(|_| ParseError::new(arg_location, "invalid function argument"))?;
                if parameter.defexpr.is_some() {
                    return Err(ParseError::new(
                        arg_location,
                        "function signatures cannot contain default values",
                    ));
                }
                let type_name = parameter
                    .arg_type
                    .as_deref()
                    .cloned()
                    .ok_or_else(|| ParseError::new(arg_location, "invalid argument type"))?;
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
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        tokens_to_type_name(tokens).map(|mut type_name| {
            type_name.location = location as ParseLoc;
            type_name
        })
    }

    pub(super) fn parse_object_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        parse_object_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_operator_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        parse_operator_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_aggregate_with_args_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<ObjectWithArgs> {
        let location = self.location();
        let tokens = self.take_until_top_level(stops);
        parse_aggregate_with_args_tokens(tokens, location)
    }

    pub(super) fn parse_aggregate_with_args_list_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
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
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
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
        if find_top_level_token(&tokens, TokenKind::Char('(')).is_some() {
            parse_operator_with_args_tokens(tokens, location)
        } else {
            validate_operator_name_tokens(&tokens, location)?;
            let mut signature = parse_object_with_args_tokens_impl(tokens, location, true)?;
            signature.args_unspecified = false;
            Ok(signature)
        }
    }

    pub(super) fn parse_object_with_args_list_until(
        &mut self,
        stops: &[TokenKind],
    ) -> PResult<NodeList> {
        let mut objects = Vec::new();
        while !self.at_any(stops) {
            let location = self.location();
            let tokens = self.take_until_top_level(&extend_stops(stops, TokenKind::Char(',')));
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

    pub(super) fn parse_parenthesized_def_elem_list_strict(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("option list cannot be empty"));
        }
        let mut defs = Vec::new();
        loop {
            let location = self.location();
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
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
            if !self.at_any(&[TokenKind::Char(','), TokenKind::Char(')')]) {
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
            let first = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected a relation option name"))?;
            let (defnamespace, defname) = if self.consume(TokenKind::Char('.')) {
                let second = self
                    .consume_col_label()
                    .ok_or_else(|| self.error_here("expected a relation option name after '.'"))?;
                (Some(first), second)
            } else {
                (None, first)
            };
            let arg = if self.consume(TokenKind::Char('=')) {
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
        self.expect(TokenKind::Char('('))?;
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("definition list cannot be empty"));
        }
        let mut definition = Vec::new();
        loop {
            let location = self.location();
            let name = self
                .consume_col_label()
                .ok_or_else(|| self.error_here("expected a definition name"))?;
            let arg = if self.consume(TokenKind::Char('=')) {
                let tokens =
                    self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
                if tokens.is_empty() {
                    return Err(self.error_here("definition '=' requires a value"));
                }
                Some(parse_operator_def_arg(&name, tokens, location)?)
            } else {
                None
            };
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
            let relation = self.parse_relation_expr(false)?;
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
}
