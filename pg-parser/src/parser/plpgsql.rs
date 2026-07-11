use super::*;

pub(super) fn parse_assignment(sql: &str, nnames: i32) -> PResult<RawStmt> {
    if !(1..=3).contains(&nnames) {
        return Err(ParseError::new(
            0,
            "PL/pgSQL assignment name count must be between 1 and 3",
        ));
    }

    let mut parser = Parser::new(sql)?;
    let location = parser.location();
    let name = if parser.at(TokenKind::Param) {
        let token = parser.advance().clone();
        match token.value {
            Some(TokenValue::Integer(number)) => format!("${number}"),
            _ => return Err(ParseError::new(token.location, "invalid parameter target")),
        }
    } else {
        parser
            .consume_col_id()
            .ok_or_else(|| parser.error_here("expected a PL/pgSQL assignment target"))?
    };
    let indirection = parser.parse_assignment_indirection()?;
    if !parser.consume(TokenKind::ColonEquals) {
        parser.expect(TokenKind::Char('='))?;
    }

    let val = Some(Box::new(parse_expression_select(&mut parser)?));

    Ok(RawStmt {
        node_tag: NodeTag::RawStmt,
        stmt: Some(Box::new(Node::PlAssignStmt(PlAssignStmt {
            node_tag: NodeTag::PlAssignStmt,
            name: Some(name),
            indirection,
            nnames,
            val,
            location: location as ParseLoc,
        }))),
        stmt_location: location as ParseLoc,
        stmt_len: 0,
    })
}

pub(super) fn parse_expression(sql: &str) -> PResult<RawStmt> {
    let mut parser = Parser::new(sql)?;
    let location = parser.location();
    let select = parse_expression_select(&mut parser)?;
    Ok(RawStmt {
        node_tag: NodeTag::RawStmt,
        stmt: Some(Box::new(Node::SelectStmt(select))),
        stmt_location: location as ParseLoc,
        stmt_len: 0,
    })
}

fn parse_expression_select(parser: &mut Parser) -> PResult<SelectStmt> {
    let mut tokens = parser.take_until_top_level(&[TokenKind::Eof]);
    let location = tokens
        .first()
        .map_or_else(|| parser.location(), |token| token.location);
    tokens.insert(
        0,
        Token {
            kind: TokenKind::Select,
            location,
            value: None,
        },
    );
    match parse_statement_node_tokens(tokens)? {
        Node::SelectStmt(select) => Ok(select),
        _ => unreachable!("synthetic SELECT must produce SelectStmt"),
    }
}
