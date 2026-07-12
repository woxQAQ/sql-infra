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
    //
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
    pub(super) fn parse_create_function(&mut self, replace: bool) -> PResult<Node> {
        let is_procedure = self.consume(TokenKind::Procedure);
        if !is_procedure {
            self.expect(TokenKind::Function)?;
        }
        let funcname = self.parse_func_name_list();
        if funcname.is_empty() {
            return Err(self.error_here("CREATE FUNCTION requires a function name"));
        }
        let mut parameters = self.parse_function_parameters()?;
        let has_return_clause = self.at(TokenKind::Returns)
            && !(self.peek_kind_n(1) == TokenKind::NullP && self.peek_kind_n(2) == TokenKind::On);
        let return_type = if has_return_clause {
            self.advance();
            if is_procedure {
                return Err(self.error_here("CREATE PROCEDURE cannot specify RETURNS"));
            }
            if self.at(TokenKind::Table) {
                let table_location = self.advance().location;
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
                        return Err(ParseError::new(
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
                            node_tag: NodeTag::TypeName,
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
                let tokens = self.take_until_top_level(Self::create_function_option_starts());
                Some(Box::new(parse_func_type_tokens(tokens).map_err(
                    |mut error| {
                        if error.location == 0 {
                            error.location = location;
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
            let location = self.location();
            match self.peek_kind() {
                TokenKind::Language => {
                    self.advance();
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
                    let first = self
                        .consume_string_like()
                        .ok_or_else(|| self.error_here("expected a function body string"))?;
                    let mut bodies = vec![make_string_node(first)];
                    if self.consume(TokenKind::Char(',')) {
                        let second = self.consume_string_like().ok_or_else(|| {
                            self.error_here("expected a second function body string")
                        })?;
                        bodies.push(make_string_node(second));
                    }
                    options.push(make_def_elem(
                        "as",
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
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
                        Some(Node::Boolean(Boolean::new(true))),
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
                        Some(Node::Boolean(Boolean::new(value))),
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
                    let name = self.parse_name_list();
                    if name.is_empty() {
                        return Err(self.error_here("SUPPORT requires a function name"));
                    }
                    options.push(make_def_elem(
                        "support",
                        Some(Node::AArrayExpr(AArrayExpr {
                            node_tag: NodeTag::AArrayExpr,
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
                        Some(Node::Boolean(Boolean::new(false))),
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
                        Some(Node::Boolean(Boolean::new(true))),
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
                        Some(Node::Boolean(Boolean::new(value))),
                        location,
                    ));
                }
                TokenKind::Leakproof => {
                    self.advance();
                    options.push(make_def_elem(
                        "leakproof",
                        Some(Node::Boolean(Boolean::new(true))),
                        location,
                    ));
                }
                TokenKind::Not => {
                    self.advance();
                    self.expect(TokenKind::Leakproof)?;
                    options.push(make_def_elem(
                        "leakproof",
                        Some(Node::Boolean(Boolean::new(false))),
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
                        Some(Node::Boolean(Boolean::new(true))),
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
        Ok(Node::CreateFunctionStmt(CreateFunctionStmt {
            node_tag: NodeTag::CreateFunctionStmt,
            is_procedure,
            replace,
            funcname,
            parameters,
            return_type,
            options,
            sql_body,
        }))
    }

    fn parse_function_parameters(&mut self) -> PResult<NodeList> {
        self.expect(TokenKind::Char('('))?;
        let mut parameters = Vec::new();
        while !self.at(TokenKind::Char(')')) {
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            parameters.push(Node::FunctionParameter(function_parameter_from_tokens(
                tokens,
            )?));
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
            let tokens = self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            let location = tokens
                .first()
                .map_or(self.location(), |token| token.location);
            let name = tokens
                .first()
                .and_then(|token| {
                    token_name_in_categories(
                        token,
                        &[KeywordCategory::Unreserved, KeywordCategory::TypeFuncName],
                    )
                })
                .ok_or_else(|| {
                    ParseError::new(location, "expected a table function column name")
                })?;
            let arg_type = parse_func_type_tokens(tokens[1..].to_vec())
                .map(Box::new)
                .map_err(|_| ParseError::new(location, "expected a table function column type"))?;
            columns.push(Node::FunctionParameter(FunctionParameter {
                node_tag: NodeTag::FunctionParameter,
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
}
