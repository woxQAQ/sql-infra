use super::*;

pub(super) fn is_function_expression_node(node: &Node) -> bool {
    matches!(
        node,
        Node::FuncCall(_)
            | Node::TypeCast(_)
            | Node::GroupingFunc(_)
            | Node::MergeSupportFunc(_)
            | Node::CoalesceExpr(_)
            | Node::MinMaxExpr(_)
            | Node::SqlValueFunction(_)
            | Node::XmlExpr(_)
            | Node::XmlSerialize(_)
            | Node::JsonFuncExpr(_)
            | Node::JsonParseExpr(_)
            | Node::JsonScalarExpr(_)
            | Node::JsonSerializeExpr(_)
            | Node::JsonObjectConstructor(_)
            | Node::JsonArrayConstructor(_)
            | Node::JsonArrayQueryConstructor(_)
            | Node::JsonObjectAgg(_)
            | Node::JsonArrayAgg(_)
            | Node::AExpr(AExpr {
                kind: AExprKind::Nullif,
                ..
            })
    )
}

pub(super) fn is_windowless_function_expression_node(node: &Node, starts_with_cast: bool) -> bool {
    is_function_expression_node(node)
        && !matches!(node, Node::FuncCall(call) if call.over.is_some())
        && (!matches!(node, Node::TypeCast(_)) || starts_with_cast)
}

pub(super) fn make_aexpr<I, S>(
    kind: AExprKind,
    name: I,
    lexpr: Option<Node>,
    rexpr: Option<Node>,
    location: usize,
) -> Node
where
    I: IntoIterator<Item = S>,
    S: Into<std::string::String>,
{
    make_aexpr_with_name(
        kind,
        name.into_iter().map(make_string_node).collect(),
        lexpr,
        rexpr,
        location,
    )
}

pub(super) fn make_aexpr_with_name(
    kind: AExprKind,
    name: NodeList,
    lexpr: Option<Node>,
    rexpr: Option<Node>,
    location: usize,
) -> Node {
    Node::AExpr(AExpr {
        node_tag: NodeTag::AExpr,
        kind,
        name,
        lexpr: lexpr.map(Box::new),
        rexpr: rexpr.map(Box::new),
        location: location as ParseLoc,
        ..AExpr::default()
    })
}

pub(super) fn make_bool_expr(kind: BoolExprType, lhs: Node, rhs: Node, location: usize) -> Node {
    match lhs {
        Node::BoolExpr(mut expression) if expression.boolop == kind => {
            expression.args.push(rhs);
            Node::BoolExpr(expression)
        }
        lhs => Node::BoolExpr(BoolExpr {
            xpr: Expr::new(NodeTag::BoolExpr),
            boolop: kind,
            args: vec![lhs, rhs],
            location: location as ParseLoc,
        }),
    }
}

pub(super) fn make_not_expr(arg: Node, location: usize) -> Node {
    Node::BoolExpr(BoolExpr {
        xpr: Expr::new(NodeTag::BoolExpr),
        boolop: BoolExprType::NotExpr,
        args: vec![arg],
        location: location as ParseLoc,
    })
}

pub(super) fn negate_node(node: Node, location: usize) -> Node {
    match node {
        Node::AConst(mut constant) => {
            constant.location = location as ParseLoc;
            match &mut constant.val {
                ValUnion::Integer(value) => {
                    value.ival = -value.ival;
                    Node::AConst(constant)
                }
                ValUnion::Float(value) => {
                    if let Some(number) = &mut value.fval {
                        if let Some(stripped) = number.strip_prefix('+') {
                            *number = stripped.to_owned();
                        } else if let Some(stripped) = number.strip_prefix('-') {
                            *number = stripped.to_owned();
                        } else {
                            number.insert(0, '-');
                        }
                    }
                    Node::AConst(constant)
                }
                _ => make_aexpr(
                    AExprKind::Op,
                    vec!["-"],
                    None,
                    Some(Node::AConst(constant)),
                    location,
                ),
            }
        }
        node => make_aexpr(AExprKind::Op, vec!["-"], None, Some(node), location),
    }
}

pub(super) fn append_indirection(arg: Node, item: Node) -> Node {
    match arg {
        Node::AIndirection(mut indirection) => {
            indirection.indirection.push(item);
            Node::AIndirection(indirection)
        }
        arg => Node::AIndirection(AIndirection {
            node_tag: NodeTag::AIndirection,
            arg: Some(Box::new(arg)),
            indirection: vec![item],
        }),
    }
}

pub(super) fn comparison_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('=') => Some("="),
        TokenKind::Char('<') => Some("<"),
        TokenKind::Char('>') => Some(">"),
        TokenKind::LessEquals => Some("<="),
        TokenKind::GreaterEquals => Some(">="),
        TokenKind::NotEquals => Some("<>"),
        _ => None,
    }
}

pub(super) fn additive_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('+') => Some("+"),
        TokenKind::Char('-') => Some("-"),
        _ => None,
    }
}

pub(super) fn multiplicative_operator(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Char('*') => Some("*"),
        TokenKind::Char('/') => Some("/"),
        TokenKind::Char('%') => Some("%"),
        _ => None,
    }
}

pub(super) fn expression_boundary(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Eof
            | TokenKind::Char(',')
            | TokenKind::Char(')')
            | TokenKind::Char(']')
            | TokenKind::Char('+')
            | TokenKind::Char('-')
            | TokenKind::Char('*')
            | TokenKind::Char('/')
            | TokenKind::Char('%')
            | TokenKind::Char('^')
            | TokenKind::Char('|')
            | TokenKind::Char('=')
            | TokenKind::Char('<')
            | TokenKind::Char('>')
            | TokenKind::LessEquals
            | TokenKind::GreaterEquals
            | TokenKind::NotEquals
            | TokenKind::Op
            | TokenKind::RightArrow
            | TokenKind::At
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::InP
            | TokenKind::Is
            | TokenKind::Like
            | TokenKind::Ilike
            | TokenKind::Similar
            | TokenKind::Between
            | TokenKind::Overlaps
            | TokenKind::Not
    )
}

pub(super) fn parse_expression_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::new(location, "expected an expression"));
    }
    ExprParser::from_owned_tokens(tokens)
        .parse()
        .map_err(|mut error| {
            if error.location() == 0 {
                error.reanchor(location);
            }
            error
        })
}

pub(super) fn parse_b_expression_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.is_empty() {
        return Err(ParseError::new(
            location,
            "expected a restricted expression",
        ));
    }
    ExprParser::from_owned_tokens(tokens)
        .parse_b()
        .map_err(|mut error| {
            if error.location() == 0 {
                error.reanchor(location);
            }
            error
        })
}

pub(super) fn parse_aexpr_const_tokens(tokens: Vec<Token>) -> PResult<Node> {
    let location = tokens.first().map_or(0, |token| token.location());
    if tokens.len() == 1
        && let Some(node) = token_to_leaf(&tokens[0])
        && matches!(node, Node::AConst(_))
    {
        return Ok(node);
    }

    let string_indexes = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (token.kind == TokenKind::SConst).then_some(index))
        .collect::<Vec<_>>();
    if let [string_index] = string_indexes.as_slice() {
        let string_token = &tokens[*string_index];
        let mut type_tokens = tokens[..*string_index].to_vec();
        type_tokens.extend_from_slice(&tokens[*string_index + 1..]);
        let type_name = parse_const_type_name_tokens(type_tokens)
            .map_err(|_| ParseError::new(location, "invalid typed constant"))?;
        let value = token_name(string_token)
            .ok_or_else(|| ParseError::ranged(string_token.range, "invalid string constant"))?;
        return Ok(Node::TypeCast(TypeCast {
            node_tag: NodeTag::TypeCast,
            arg: Some(Box::new(Node::AConst(AConst::string(
                value,
                string_token.location() as ParseLoc,
            )))),
            type_name: Some(Box::new(type_name)),
            location: location as ParseLoc,
        }));
    }

    Err(ParseError::new(location, "expected a constant"))
}
