//! Index declarations and index-element parsing.
//!
//! Expressions, columns, collations, operator classes, ordering, null treatment,
//! and completion-aware element fragments converge here.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createindex.html
    // CREATE [ UNIQUE ] INDEX [ CONCURRENTLY ] [ [ IF NOT EXISTS ] name ] ON [ ONLY ] table_name [
    // USING method ]     ( { column_name | ( expression ) } [ COLLATE collation ] [ opclass [ (
    // opclass_parameter = value [, ... ] ) ] ] [ ASC | DESC ] [ NULLS { FIRST | LAST } ] [, ...] )
    //     [ INCLUDE ( column_name [, ...] ) ]
    //     [ NULLS [ NOT ] DISTINCT ]
    //     [ WITH ( storage_parameter [= value] [, ... ] ) ]
    //     [ TABLESPACE tablespace_name ]
    //     [ WHERE predicate ]
    pub(super) fn parse_index(&mut self) -> PResult<Node> {
        let unique = self.consume(TokenKind::Unique);
        self.expect(TokenKind::Index)?;
        let concurrent = self.consume(TokenKind::Concurrently);
        let if_not_exists = self.consume_if_not_exists()?;
        self.record_completion_slot(completion::GrammarSlot::Index);
        let idxname = if self.peek_kind() != TokenKind::On {
            self.consume_col_id()
        } else {
            None
        };
        if if_not_exists && idxname.is_none() {
            return Err(self.error_here("CREATE INDEX IF NOT EXISTS requires an index name"));
        }
        self.expect(TokenKind::On)?;
        self.record_completion_slot(completion::GrammarSlot::MaterializedView);
        let owner_start = self.pos;
        let relation = Some(Box::new(
            self.parse_relation_expr_with_slot(completion::GrammarSlot::Table)?,
        ));
        let owner_end = self.pos;
        self.push_completion_membership_owner_from_tokens(
            &[completion::GrammarSlot::Column],
            &[
                ObjectType::Table,
                ObjectType::View,
                ObjectType::Matview,
                ObjectType::ForeignTable,
            ],
            owner_start,
            owner_end,
        );
        let access_method = if self.consume(TokenKind::Using) {
            self.record_completion_slot(completion::GrammarSlot::AccessMethod);
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("USING requires an access method"))?,
            )
        } else {
            Some("btree".to_owned())
        };
        self.expect(TokenKind::Char('('))?;
        let index_params = self.parse_index_elem_list_body()?;
        self.expect(TokenKind::Char(')'))?;

        let index_including_params = if self.consume(TokenKind::Include) {
            self.expect(TokenKind::Char('('))?;
            let columns = self.parse_index_elem_list_body()?;
            self.expect(TokenKind::Char(')'))?;
            columns
        } else {
            Vec::new()
        };
        let nulls_not_distinct = if self.consume(TokenKind::NullsP) {
            let not_distinct = self.consume(TokenKind::Not);
            self.expect(TokenKind::Distinct)?;
            not_distinct
        } else {
            false
        };
        let options = if self.consume(TokenKind::With) {
            let options = self.parse_parenthesized_reloptions()?;
            if options.is_empty() {
                return Err(self.error_here("CREATE INDEX WITH requires an option list"));
            }
            options
        } else {
            Vec::new()
        };
        let table_space = if self.consume(TokenKind::Tablespace) {
            self.record_completion_slot(completion::GrammarSlot::Tablespace);
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("TABLESPACE requires a name"))?,
            )
        } else {
            None
        };
        let where_clause = if self.consume(TokenKind::Where) {
            Some(self.parse_expr_box_strict_until(&[TokenKind::Char(';'), TokenKind::Eof])?)
        } else {
            None
        };
        Ok(node!(IndexStmt {
            idxname,
            relation,
            access_method,
            table_space,
            index_params,
            index_including_params,
            options,
            where_clause,
            unique,
            nulls_not_distinct,
            concurrent,
            if_not_exists,
            ..IndexStmt::default()
        }))
    }

    fn parse_index_elem_list_body(&mut self) -> PResult<NodeList> {
        if self.at(TokenKind::Char(')')) {
            return Err(self.error_here("index element list cannot be empty"));
        }
        let mut elements = Vec::new();
        loop {
            let mut tokens =
                self.take_until_top_level(&[TokenKind::Char(','), TokenKind::Char(')')]);
            if tokens_end_at_top_level(&tokens)
                && parse_index_elem_tokens_with_completion(tokens.clone(), None).is_ok()
            {
                self.record_completion_tokens(&[TokenKind::Char(','), TokenKind::Char(')')]);
            }
            self.append_completion_marker(&mut tokens);
            let location = tokens
                .first()
                .map_or(self.location(), |token| token.location());
            let starts_parenthesized =
                tokens.first().map(|token| token.kind) == Some(TokenKind::Char('('));
            let starts_with_cast = tokens.first().map(|token| token.kind) == Some(TokenKind::Cast);
            let element = parse_index_elem_tokens_with_completion(tokens, self.completion.clone())?;
            if let Some(expression) = element.expr.as_deref()
                && !starts_parenthesized
                && !is_windowless_function_expression_node(expression, starts_with_cast)
            {
                return Err(ParseError::syntax_exit(
                    location,
                    "index expressions must be parenthesized unless they are function calls",
                ));
            }
            elements.push(Node::IndexElem(element));
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected an index element after ','"));
            }
        }
        Ok(elements)
    }
}
pub(super) fn node_to_index_elem(node: Node) -> Node {
    match node {
        node!(ColumnRef {
            fields,
            location,
            ..
        }) if fields.len() == 1 => node!(IndexElem {
            name: fields.first().and_then(|field| match field {
                Node::String(value) => value.sval.clone(),
                _ => None,
            }),
            location,
            ..IndexElem::default()
        }),
        expr => node!(IndexElem {
            expr: Some(Box::new(expr)),
            ..IndexElem::default()
        }),
    }
}

pub(super) fn parse_index_elem_tokens_with_completion(
    tokens: Vec<Token>,
    completion: Option<completion::SharedCollector>,
) -> PResult<IndexElem> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::syntax_exit(
            location,
            "expected an index element",
        ));
    }
    let collate_index = find_top_level_token(&tokens, TokenKind::Collate);
    let (expression, suffix_start) = if let Some(index) = collate_index {
        if index == 0 {
            return Err(ParseError::syntax_exit(
                location,
                "COLLATE requires an index expression",
            ));
        }
        (
            parse_expression_tokens_with_completion(tokens[..index].to_vec(), completion.clone())?,
            index,
        )
    } else {
        let mut parser = ExprParser::with_completion(tokens.clone(), completion.clone());
        let expression = parser.parse_expr(0).ok_or_else(|| {
            parser
                .error
                .take()
                .unwrap_or_else(|| ParseError::syntax_exit(location, "invalid index expression"))
        })?;
        (expression, parser.pos)
    };
    let Node::IndexElem(mut element) = node_to_index_elem(expression) else {
        unreachable!("node_to_index_elem always returns IndexElem");
    };
    element.location = location as ParseLoc;

    let mut suffix_tokens = tokens[suffix_start..].to_vec();
    let end_location = suffix_tokens.last().map_or(location, Token::end_location);
    suffix_tokens.push(Token::synthetic(TokenKind::Eof, end_location));
    let mut suffix = Parser {
        tokens: suffix_tokens,
        pos: 0,
        completion,
    };
    if suffix.consume(TokenKind::Collate) {
        suffix.record_completion_slot(completion::GrammarSlot::Collation);
        element.collation = suffix.parse_name_list();
        if element.collation.is_empty() {
            return Err(suffix.error_here("COLLATE requires a collation name"));
        }
    }
    if !suffix.at_any(&[
        TokenKind::Asc,
        TokenKind::Desc,
        TokenKind::NullsP,
        TokenKind::Eof,
    ]) {
        suffix.record_completion_slot(completion::GrammarSlot::OperatorClass);
        element.opclass = suffix.parse_name_list();
        if element.opclass.is_empty() {
            return Err(suffix.error_here("expected an operator class name"));
        }
        suffix.record_completion_follow_tokens(&[TokenKind::Char('(')]);
        if suffix.at(TokenKind::Char('(')) {
            element.opclassopts = suffix.parse_parenthesized_reloptions()?;
        }
    }
    element.ordering = if suffix.consume(TokenKind::Asc) {
        SortByDir::Asc
    } else if suffix.consume(TokenKind::Desc) {
        SortByDir::Desc
    } else {
        SortByDir::Default
    };
    if suffix.consume(TokenKind::NullsP) {
        element.nulls_ordering = if suffix.consume(TokenKind::FirstP) {
            SortByNulls::First
        } else if suffix.consume(TokenKind::LastP) {
            SortByNulls::Last
        } else {
            return Err(suffix.error_here("NULLS requires FIRST or LAST"));
        };
    }
    if !suffix.at(TokenKind::Eof) {
        return Err(suffix.error_here("unexpected token after index element options"));
    }
    Ok(element)
}
