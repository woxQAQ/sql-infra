use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createcast.html
    // CREATE CAST (source_type AS target_type)
    //     WITH FUNCTION function_name [ (argument_type [, ...]) ]
    //     [ AS ASSIGNMENT | AS IMPLICIT ]
    //
    // CREATE CAST (source_type AS target_type)
    //     WITHOUT FUNCTION
    //     [ AS ASSIGNMENT | AS IMPLICIT ]
    //
    // CREATE CAST (source_type AS target_type)
    //     WITH INOUT
    //     [ AS ASSIGNMENT | AS IMPLICIT ]
    pub(super) fn parse_create_cast(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Cast)?;
        self.expect(TokenKind::Char('('))?;
        let sourcetype = self
            .parse_type_name_until(&[TokenKind::As, TokenKind::Char(')'), TokenKind::Eof])
            .map(Box::new)
            .ok_or_else(|| self.error_here("CREATE CAST requires a source type"))?;
        self.expect(TokenKind::As)?;
        let targettype = self
            .parse_type_name_until(&[TokenKind::Char(')'), TokenKind::Eof])
            .map(Box::new)
            .ok_or_else(|| self.error_here("CREATE CAST requires a target type"))?;
        self.expect(TokenKind::Char(')'))?;

        let mut func = None;
        let mut inout = false;
        if self.consume(TokenKind::With) {
            if self.consume(TokenKind::Function) {
                func = Some(Box::new(self.parse_object_with_args_until(&[
                    TokenKind::As,
                    TokenKind::Char(';'),
                    TokenKind::Eof,
                ])?));
            } else if self.consume(TokenKind::Inout) {
                inout = true;
            } else {
                return Err(self.error_here("expected FUNCTION or INOUT after WITH"));
            }
        } else {
            self.expect(TokenKind::Without)?;
            self.expect(TokenKind::Function)?;
        }
        let context = if self.consume(TokenKind::As) {
            if self.consume(TokenKind::ImplicitP) {
                CoercionContext::Implicit
            } else if self.consume(TokenKind::Assignment) {
                CoercionContext::Assignment
            } else {
                return Err(self.error_here("CAST context must be IMPLICIT or ASSIGNMENT"));
            }
        } else {
            CoercionContext::Explicit
        };
        Ok(Node::CreateCastStmt(CreateCastStmt {
            node_tag: NodeTag::CreateCastStmt,
            sourcetype: Some(sourcetype),
            targettype: Some(targettype),
            func,
            context,
            inout,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createconversion.html
    // CREATE [ DEFAULT ] CONVERSION name
    //     FOR source_encoding TO dest_encoding FROM function_name
    pub(super) fn parse_create_conversion(&mut self, def: bool) -> PResult<Node> {
        self.expect(TokenKind::ConversionP)?;
        let conversion_name = self.parse_name_list_until_keywords(&[
            TokenKind::For,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ]);
        if conversion_name.is_empty() {
            return Err(self.error_here("CREATE CONVERSION requires a name"));
        }
        self.expect(TokenKind::For)?;
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here("source encoding must be a string"));
        }
        let for_encoding_name = self.consume_string_like();
        self.expect(TokenKind::To)?;
        if !self.at(TokenKind::SConst) {
            return Err(self.error_here("target encoding must be a string"));
        }
        let to_encoding_name = self.consume_string_like();
        self.expect(TokenKind::From)?;
        let func_name =
            self.parse_name_list_until_keywords(&[TokenKind::Char(';'), TokenKind::Eof]);
        if func_name.is_empty() {
            return Err(self.error_here("CREATE CONVERSION requires a function"));
        }
        Ok(Node::CreateConversionStmt(CreateConversionStmt {
            node_tag: NodeTag::CreateConversionStmt,
            conversion_name,
            for_encoding_name,
            to_encoding_name,
            func_name,
            def,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createtransform.html
    // CREATE [ OR REPLACE ] TRANSFORM FOR type_name LANGUAGE lang_name (
    //     FROM SQL WITH FUNCTION from_sql_function_name [ (argument_type [, ...]) ],
    //     TO SQL WITH FUNCTION to_sql_function_name [ (argument_type [, ...]) ]
    // );
    pub(super) fn parse_create_transform(&mut self, replace: bool) -> PResult<Node> {
        self.expect(TokenKind::Transform)?;
        self.expect(TokenKind::For)?;
        let type_name = self
            .parse_type_name_until(&[TokenKind::Language, TokenKind::Char(';'), TokenKind::Eof])
            .map(Box::new)
            .ok_or_else(|| self.error_here("CREATE TRANSFORM requires a type"))?;
        self.expect(TokenKind::Language)?;
        let lang = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE TRANSFORM requires a language"))?,
        );
        let mut fromsql = None;
        let mut tosql = None;
        self.expect(TokenKind::Char('('))?;
        loop {
            if self.at(TokenKind::Char(')')) {
                break;
            }
            let is_from = self.consume(TokenKind::From);
            let is_to = if !is_from {
                self.consume(TokenKind::To)
            } else {
                false
            };
            if !is_from && !is_to {
                return Err(self.error_here("expected FROM SQL or TO SQL transform element"));
            }
            self.expect(TokenKind::SqlP)?;
            self.expect(TokenKind::With)?;
            self.expect(TokenKind::Function)?;
            let func = Some(Box::new(self.parse_object_with_args_until(&[
                TokenKind::Char(','),
                TokenKind::Char(')'),
                TokenKind::Eof,
            ])?));
            if is_from {
                if fromsql.is_some() {
                    return Err(self.error_here("duplicate FROM SQL transform"));
                }
                fromsql = func;
            } else if is_to {
                if tosql.is_some() {
                    return Err(self.error_here("duplicate TO SQL transform"));
                }
                tosql = func;
            }
            if !self.consume(TokenKind::Char(',')) {
                break;
            }
            if self.at(TokenKind::Char(')')) {
                return Err(self.error_here("expected a transform element after ','"));
            }
        }
        if fromsql.is_none() && tosql.is_none() {
            return Err(self.error_here("CREATE TRANSFORM requires at least one transform element"));
        }
        self.expect(TokenKind::Char(')'))?;
        Ok(Node::CreateTransformStmt(CreateTransformStmt {
            node_tag: NodeTag::CreateTransformStmt,
            replace,
            type_name: Some(type_name),
            lang,
            fromsql,
            tosql,
        }))
    }
}
