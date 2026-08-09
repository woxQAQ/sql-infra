//! Function and procedure creation.
//!
//! Parameters, result types, transforms, definition options, SQL bodies, and
//! `BEGIN ATOMIC` statement lists are assembled into routine raw nodes.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createfunction.html
    // CREATE [ OR REPLACE ] FUNCTION
    //     name ( [ [ argmode ] [ argname ] argtype [ { DEFAULT | = } default_expr ] [, ...] ] )
    //     [ RETURNS rettype
    //       | RETURNS TABLE ( column_name column_type [, ...] ) ]
    //   { LANGUAGE lang_name
    //     | TRANSFORM { FOR TYPE type_name } [, ... ]
    //     | WINDOW
    //     | { IMMUTABLE | STABLE | VOLATILE }
    //     | [ NOT ] LEAKPROOF
    //     | { CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT }
    //     | { [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER }
    //     | PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     | COST execution_cost
    //     | ROWS result_rows
    //     | SUPPORT support_function
    //     | SET configuration_parameter { TO value | = value | FROM CURRENT }
    //     | AS 'definition'
    //     | AS 'obj_file', 'link_symbol'
    //     | sql_body
    //   } ...
    pub(super) fn parse_create_function(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::Function)?;
        self.record_completion_slot(GrammarSlot::Function);
        let funcname = self.parse_func_name_list();
        if funcname.is_empty() {
            return Err(self.error_here("CREATE FUNCTION requires a function name"));
        }
        let mut parameters = self.parse_function_parameters()?;
        self.record_completion_tokens(&[TokenKind::Returns]);
        let has_return_clause = self.at(TokenKind::Returns)
            && !(self.peek_kind_n(1) == TokenKind::NullP && self.peek_kind_n(2) == TokenKind::On);
        let return_type = if has_return_clause {
            self.advance();
            if self.at(TokenKind::Table) {
                let table_location = self.advance().location();
                for parameter in &parameters {
                    let Node::FunctionParameter(parameter) = parameter else {
                        continue;
                    };
                    if !matches!(
                        parameter.mode,
                        FunctionParameterMode::Default
                            | FunctionParameterMode::In
                            | FunctionParameterMode::Variadic
                    ) {
                        return Err(ParseError::syntax_exit(
                            parameter.location as usize,
                            "OUT and INOUT arguments aren't allowed in TABLE functions",
                        ));
                    }
                }
                let columns = self.parse_table_function_columns()?;
                let mut table_type =
                    if columns.len() == 1 {
                        let Node::FunctionParameter(column) = &columns[0] else {
                            unreachable!("table function columns are FunctionParameter nodes");
                        };
                        column.arg_type.as_deref().cloned().ok_or_else(|| {
                            self.error_here("table function column requires a type")
                        })?
                    } else {
                        TypeName {
                            names: system_type_names("record"),
                            ..TypeName::default()
                        }
                    };
                table_type.setof = true;
                table_type.location = table_location as ParseLoc;
                parameters.extend(columns);
                Some(Box::new(table_type))
            } else {
                let location = self.location();
                self.record_completion_slot(GrammarSlot::Type);
                self.record_completion_qualified_name_slot(
                    GrammarSlot::Type,
                    Self::create_function_option_starts(),
                );
                let tokens = self.take_until_top_level(Self::create_function_option_starts());
                if self.at_completion() {
                    let mut completion_tokens = tokens.clone();
                    self.append_completion_marker(&mut completion_tokens);
                    record_type_name_completion(&completion_tokens, self.completion.as_ref());
                }
                Some(Box::new(parse_func_type_tokens(tokens).map_err(
                    |mut error| {
                        if error.location() == 0 {
                            error.reanchor(location);
                        }
                        error
                    },
                )?))
            }
        } else {
            None
        };
        let mut options = Vec::new();
        let mut sql_body = None;
        while !self.at_statement_end() {
            self.record_completion_tokens(Self::create_function_option_starts());
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Language => {
                    self.advance();
                    self.record_completion_slot(GrammarSlot::Language);
                    let language = self
                        .consume_non_reserved_word_or_sconst()
                        .ok_or_else(|| self.error_here("expected a language name"))?;
                    options.push(make_def_elem(
                        "language",
                        Some(make_string_node(language)),
                        location,
                    ));
                }
                TokenKind::As => {
                    self.advance();
                    let first = self.consume_required_string("expected a function body string")?;
                    let mut bodies = vec![make_string_node(first)];
                    if self.consume(TokenKind::Char(',')) {
                        let second =
                            self.consume_required_string("expected a second function body string")?;
                        bodies.push(make_string_node(second));
                    }
                    options.push(make_def_elem(
                        "as",
                        Some(node!(AArrayExpr {
                            elements: bodies,
                            ..AArrayExpr::default()
                        })),
                        location,
                    ));
                }
                TokenKind::Immutable | TokenKind::Stable | TokenKind::Volatile => {
                    let value = token_text(self.advance());
                    options.push(make_def_elem(
                        "volatility",
                        Some(make_string_node(value)),
                        location,
                    ));
                }
                TokenKind::StrictP => {
                    self.advance();
                    options.push(make_def_elem(
                        "strict",
                        Some(node!(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Security => {
                    self.advance();
                    let value = if self.consume(TokenKind::Definer) {
                        true
                    } else if self.consume(TokenKind::Invoker) {
                        false
                    } else {
                        return Err(self.error_here("expected DEFINER or INVOKER"));
                    };
                    options.push(make_def_elem(
                        "security",
                        Some(node!(Boolean::new(value))),
                        location,
                    ));
                }
                TokenKind::Parallel => {
                    self.advance();
                    let value = self
                        .consume_col_id()
                        .ok_or_else(|| self.error_here("expected a PARALLEL mode"))?;
                    options.push(make_def_elem(
                        "parallel",
                        Some(make_string_node(value)),
                        location,
                    ));
                }
                TokenKind::Cost | TokenKind::Rows => {
                    let name = token_text(self.advance());
                    let value = self.parse_numeric_only()?;
                    options.push(make_def_elem(&name, Some(value), location));
                }
                TokenKind::Support => {
                    self.advance();
                    self.record_completion_slot(GrammarSlot::Function);
                    let name = self.parse_name_list();
                    if name.is_empty() {
                        return Err(self.error_here("SUPPORT requires a function name"));
                    }
                    options.push(make_def_elem(
                        "support",
                        Some(node!(AArrayExpr {
                            elements: name,
                            ..AArrayExpr::default()
                        })),
                        location,
                    ));
                }
                TokenKind::Return => {
                    sql_body = Some(Box::new(self.parse_return()?));
                    break;
                }
                TokenKind::BeginP => {
                    sql_body = Some(Box::new(self.parse_atomic_routine_body()?));
                    break;
                }
                TokenKind::Called => {
                    self.advance();
                    self.expect(TokenKind::On)?;
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::InputP)?;
                    options.push(make_def_elem(
                        "strict",
                        Some(node!(Boolean::new(false))),
                        location,
                    ));
                }
                TokenKind::Returns => {
                    self.advance();
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::On)?;
                    self.expect(TokenKind::NullP)?;
                    self.expect(TokenKind::InputP)?;
                    options.push(make_def_elem(
                        "strict",
                        Some(node!(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::External => {
                    self.advance();
                    self.expect(TokenKind::Security)?;
                    let value = if self.consume(TokenKind::Definer) {
                        true
                    } else if self.consume(TokenKind::Invoker) {
                        false
                    } else {
                        return Err(self.error_here("SECURITY requires DEFINER or INVOKER"));
                    };
                    options.push(make_def_elem(
                        "security",
                        Some(node!(Boolean::new(value))),
                        location,
                    ));
                }
                TokenKind::Leakproof => {
                    self.advance();
                    options.push(make_def_elem(
                        "leakproof",
                        Some(node!(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Not => {
                    self.advance();
                    self.expect(TokenKind::Leakproof)?;
                    options.push(make_def_elem(
                        "leakproof",
                        Some(node!(Boolean::new(false))),
                        location,
                    ));
                }
                TokenKind::Transform => {
                    self.advance();
                    let types = self.parse_transform_type_list()?;
                    options.push(make_def_elem(
                        "transform",
                        Some(name_list_node(types)),
                        location,
                    ));
                }
                TokenKind::Window => {
                    self.advance();
                    options.push(make_def_elem(
                        "window",
                        Some(node!(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Set | TokenKind::Reset => {
                    let setstmt = self.parse_function_set_reset_clause_until(
                        Self::create_function_option_starts(),
                    )?;
                    options.push(make_def_elem(
                        "set",
                        Some(Node::VariableSetStmt(setstmt)),
                        location,
                    ));
                }
                other => {
                    return Err(
                        self.error_here(format!("unsupported CREATE FUNCTION option {:?}", other))
                    );
                }
            }
        }
        Ok(node!(CreateFunctionStmt {
            is_procedure: false,
            replace,
            funcname,
            parameters,
            return_type,
            options,
            sql_body,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createprocedure.html
    // CREATE [ OR REPLACE ] PROCEDURE
    //     name ( [ [ argmode ] [ argname ] argtype [ { DEFAULT | = } default_expr ] [, ...] ] )
    //   { LANGUAGE lang_name
    //     | TRANSFORM { FOR TYPE type_name } [, ... ]
    //     | [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     | SET configuration_parameter { TO value | = value | FROM CURRENT }
    //     | AS 'definition'
    //     | AS 'obj_file', 'link_symbol'
    //     | sql_body
    //   } ...
    pub(super) fn parse_create_procedure(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::Procedure)?;
        self.record_completion_slot(GrammarSlot::Procedure);
        let funcname = self.parse_func_name_list();
        if funcname.is_empty() {
            return Err(self.error_here("CREATE PROCEDURE requires a procedure name"));
        }
        let parameters = self.parse_function_parameters()?;
        let mut options = Vec::new();
        let mut sql_body = None;
        while !self.at_statement_end() {
            self.record_completion_tokens(Self::create_procedure_option_starts());
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Language => {
                    self.advance();
                    self.record_completion_slot(GrammarSlot::Language);
                    let language = self
                        .consume_non_reserved_word_or_sconst()
                        .ok_or_else(|| self.error_here("expected a language name"))?;
                    options.push(make_def_elem(
                        "language",
                        Some(make_string_node(language)),
                        location,
                    ));
                }
                TokenKind::As => {
                    self.advance();
                    let first = self.consume_required_string("expected a procedure body string")?;
                    let mut bodies = vec![make_string_node(first)];
                    if self.consume(TokenKind::Char(',')) {
                        let second = self
                            .consume_required_string("expected a second procedure body string")?;
                        bodies.push(make_string_node(second));
                    }
                    options.push(make_def_elem(
                        "as",
                        Some(node!(AArrayExpr {
                            elements: bodies,
                            ..AArrayExpr::default()
                        })),
                        location,
                    ));
                }
                TokenKind::Security => {
                    self.advance();
                    let value = if self.consume(TokenKind::Definer) {
                        true
                    } else if self.consume(TokenKind::Invoker) {
                        false
                    } else {
                        return Err(self.error_here("expected DEFINER or INVOKER"));
                    };
                    options.push(make_def_elem(
                        "security",
                        Some(node!(Boolean::new(value))),
                        location,
                    ));
                }
                TokenKind::External => {
                    self.advance();
                    self.expect(TokenKind::Security)?;
                    let value = if self.consume(TokenKind::Definer) {
                        true
                    } else if self.consume(TokenKind::Invoker) {
                        false
                    } else {
                        return Err(self.error_here("SECURITY requires DEFINER or INVOKER"));
                    };
                    options.push(make_def_elem(
                        "security",
                        Some(node!(Boolean::new(value))),
                        location,
                    ));
                }
                TokenKind::Transform => {
                    self.advance();
                    let types = self.parse_transform_type_list()?;
                    options.push(make_def_elem(
                        "transform",
                        Some(name_list_node(types)),
                        location,
                    ));
                }
                TokenKind::Set | TokenKind::Reset => {
                    let setstmt = self.parse_function_set_reset_clause_until(
                        Self::create_procedure_option_starts(),
                    )?;
                    options.push(make_def_elem(
                        "set",
                        Some(Node::VariableSetStmt(setstmt)),
                        location,
                    ));
                }
                TokenKind::BeginP => {
                    sql_body = Some(Box::new(self.parse_atomic_routine_body()?));
                    break;
                }
                other => {
                    return Err(
                        self.error_here(format!("unsupported CREATE PROCEDURE option {:?}", other))
                    );
                }
            }
        }
        Ok(node!(CreateFunctionStmt {
            is_procedure: true,
            replace,
            funcname,
            parameters,
            return_type: None,
            options,
            sql_body,
        }))
    }

    fn parse_function_parameters(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        let mut parameters = Vec::new();
        while !self.at(TokenKind::Char(')')) {
            let mut tokens = self.take_until_top_level(COMMA_OR_CLOSE_PAREN_TOKENS);
            self.append_completion_marker(&mut tokens);
            parameters.push(Node::FunctionParameter(
                function_parameter_from_tokens_with_completion(tokens, self.completion.clone())?,
            ));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a function parameter after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(parameters)
    }

    fn parse_table_function_columns(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        let mut columns = Vec::new();
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("RETURNS TABLE requires at least one column"));
        }
        while !self.at(TokenKind::Char(')')) {
            let stops = [TokenKind::Char(','), TokenKind::Char(')')];
            let mut tokens = self.take_until_top_level(&stops);
            self.append_completion_marker(&mut tokens);
            let location = tokens.first().location_or(self.location());
            if let Some(completion_index) = tokens
                .iter()
                .position(|token| token.kind == TokenKind::Completion)
                && let Some(collector) = &self.completion
            {
                let mut collector = collector.borrow_mut();
                if completion_index == 0 {
                    collector.record_slot(GrammarSlot::AnyName);
                } else {
                    collector.record_slot(GrammarSlot::Type);
                }
            }
            let name = tokens
                .first()
                .and_then(|token| {
                    token_name_in_categories(
                        token,
                        &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName],
                    )
                })
                .ok_or_else(|| {
                    ParseError::syntax_exit(location, "expected a table function column name")
                })?;
            let arg_type = parse_func_type_tokens(tokens[1..].to_vec())
                .map(Box::new)
                .map_err(|_| {
                    ParseError::syntax_exit(location, "expected a table function column type")
                })?;
            columns.push(node!(FunctionParameter {
                name: Some(name),
                arg_type: Some(arg_type),
                mode: FunctionParameterMode::Table,
                location: location as ParseLoc,
                ..FunctionParameter::default()
            }));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a table function column after ','"));
            }
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(columns)
    }

    fn parse_transform_type_list(&mut self) -> PResult<NodeList> {
        let mut types = Vec::new();
        loop {
            self.expect(TokenKind::For)?;
            self.expect(TokenKind::TypeP)?;
            let mut stops = Self::create_function_option_starts().to_vec();
            stops.push(TokenKind::Char(','));
            let type_name = self
                .parse_type_name_until(&stops)
                .ok_or_else(|| self.error_here("TRANSFORM FOR TYPE requires a type"))?;
            types.push(Node::TypeName(type_name));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if !self.at(TokenKind::For) {
                return Err(self.error_here("expected FOR TYPE after ',' in TRANSFORM clause"));
            }
        }
        Ok(types)
    }

    fn parse_atomic_routine_body(&mut self) -> PResult<Node> {
        self.expect(TokenKind::BeginP)?;
        self.expect(TokenKind::Atomic)?;
        let mut statements = Vec::new();
        loop {
            while self.consume(TokenKind::Char(';')) {}
            if self.consume(TokenKind::EndP) {
                break;
            }
            if self.at(TokenKind::Eof) {
                return Err(self.error_here("BEGIN ATOMIC routine body requires END"));
            }
            let statement = if self.at(TokenKind::Return) {
                self.parse_return()?
            } else {
                self.parse_statement(None)?
            };
            statements.push(statement);
            self.expect(TokenKind::Char(';'))?;
        }
        Ok(name_list_node(vec![name_list_node(statements)]))
    }

    fn create_function_option_starts() -> &'static [TokenKind] {
        &[
            TokenKind::As,
            TokenKind::Language,
            TokenKind::Transform,
            TokenKind::Window,
            TokenKind::Called,
            TokenKind::Returns,
            TokenKind::StrictP,
            TokenKind::Immutable,
            TokenKind::Stable,
            TokenKind::Volatile,
            TokenKind::External,
            TokenKind::Security,
            TokenKind::Leakproof,
            TokenKind::Not,
            TokenKind::Cost,
            TokenKind::Rows,
            TokenKind::Support,
            TokenKind::Set,
            TokenKind::Reset,
            TokenKind::Parallel,
            TokenKind::Return,
            TokenKind::BeginP,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]
    }

    fn create_procedure_option_starts() -> &'static [TokenKind] {
        &[
            TokenKind::As,
            TokenKind::Language,
            TokenKind::Transform,
            TokenKind::External,
            TokenKind::Security,
            TokenKind::Set,
            TokenKind::Reset,
            TokenKind::BeginP,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]
    }
}
