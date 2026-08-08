//! Precedence-aware expression parsing.
//!
//! [`ExprParser`] coordinates expression modes, binary binding powers, nested
//! queries, and completion provenance; syntax-specific pieces live in sibling
//! expression modules.

use super::*;

const NEGATED_PREDICATE_TOKENS: &[TokenKind] = &[
    TokenKind::InP,
    TokenKind::Like,
    TokenKind::Ilike,
    TokenKind::Similar,
    TokenKind::Between,
];

// Binding powers for precedence-climbing expression parsing. The numeric
// values are arbitrary scores; only their relative order matters. An infix
// or postfix operator is folded into the left-hand side only while its
// binding power is at least the current `min_bp`, and parsing its operand
// at `binding_power + 1` yields left associativity. Prefix operators
// (`NOT`, unary `+`/`-`) capture everything up to their binding power.
// The ladder mirrors the `%left` / `%nonassoc` / `%right` declaration stack
// in PostgreSQL's `gram.y`; gaps between adjacent levels leave room to
// insert a new level later without renumbering.
const OR_BINDING_POWER: u8 = 10;
pub(super) const AND_BINDING_POWER: u8 = 20;
const IS_BINDING_POWER: u8 = 25;
const COMPARISON_BINDING_POWER: u8 = 30;
pub(super) const PREDICATE_BINDING_POWER: u8 = 35;
const GENERIC_OPERATOR_BINDING_POWER: u8 = 40;
const ADDITIVE_BINDING_POWER: u8 = 45;
const MULTIPLICATIVE_BINDING_POWER: u8 = 50;
const EXPONENTIATION_BINDING_POWER: u8 = 55;
const AT_TIME_ZONE_BINDING_POWER: u8 = 60;
const COLLATE_BINDING_POWER: u8 = 65;
pub(super) const UMINUS_BINDING_POWER: u8 = 70;
const TYPE_CAST_BINDING_POWER: u8 = 80;
const INDIRECTION_BINDING_POWER: u8 = 90;

#[derive(Default)]
struct InfixChainState {
    saw_is_predicate: bool,
    saw_comparison: bool,
    saw_special_predicate: bool,
}

#[derive(Clone, Copy)]
struct FixedBinaryOperator {
    name: &'static str,
    binding_power: u8,
    is_comparison: bool,
}

impl FixedBinaryOperator {
    fn for_token(kind: TokenKind) -> Option<Self> {
        if let Some(name) = comparison_operator(kind) {
            return Some(Self {
                name,
                binding_power: COMPARISON_BINDING_POWER,
                is_comparison: true,
            });
        }
        if let Some(name) = additive_operator(kind) {
            return Some(Self {
                name,
                binding_power: ADDITIVE_BINDING_POWER,
                is_comparison: false,
            });
        }
        if let Some(name) = multiplicative_operator(kind) {
            return Some(Self {
                name,
                binding_power: MULTIPLICATIVE_BINDING_POWER,
                is_comparison: false,
            });
        }
        (kind == TokenKind::Char('^')).then_some(Self {
            name: "^",
            binding_power: EXPONENTIATION_BINDING_POWER,
            is_comparison: false,
        })
    }
}

pub(super) struct ExprParser {
    pub(super) tokens: Vec<Token>,
    pub(super) pos: usize,
    pub(super) error: Option<ParserExit>,
    pub(super) completion: Option<completion::SharedCollector>,
}

impl ExprParser {
    pub(super) fn with_completion(
        mut tokens: Vec<Token>,
        completion: Option<completion::SharedCollector>,
    ) -> Self {
        let location = tokens.last().map_or(0, Token::end_location);
        tokens.push(Token::synthetic(TokenKind::Eof, location));
        Self {
            tokens,
            pos: 0,
            error: None,
            completion,
        }
    }

    pub(super) fn parse(self) -> PResult<Node> {
        self.parse_complete(false)
    }

    pub(super) fn parse_b(self) -> PResult<Node> {
        self.parse_complete(true)
    }

    pub(super) fn parse_c(mut self) -> PResult<Node> {
        let location = self.location();
        let node = self.parse_c_expr().ok_or_else(|| {
            self.error
                .take()
                .unwrap_or_else(|| ParseError::syntax_exit(location, "invalid common expression"))
        })?;
        if !self.at(TokenKind::Eof) {
            return Err(self.error_here("unexpected token after common expression"));
        }
        Ok(node)
    }

    pub(super) fn parse_complete(mut self, restricted: bool) -> PResult<Node> {
        let location = self.location();
        let node = self.parse_expr_mode(0, restricted).ok_or_else(|| {
            self.error.take().unwrap_or_else(|| {
                ParseError::syntax_exit(location, "invalid or unsupported expression")
            })
        })?;
        if !self.at(TokenKind::Eof) {
            return Err(self.error_here("unexpected token after expression"));
        }
        Ok(node)
    }

    pub(super) fn parse_nested_select(&mut self, mut tokens: Vec<Token>) -> Option<Node> {
        if self.at_completion() {
            tokens.push(self.peek().clone());
        }
        match parse_select_statement_tokens_with_completion(tokens, self.completion.clone()) {
            Ok(node) => Some(node),
            Err(error) => self.fail_with(error),
        }
    }

    pub(super) fn parse_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, false)
    }

    pub(super) fn parse_b_expr(&mut self, min_bp: u8) -> Option<Node> {
        self.parse_expr_mode(min_bp, true)
    }

    pub(super) fn parse_expr_mode(&mut self, min_bp: u8, restricted: bool) -> Option<Node> {
        let expression_start = self.location();
        let mut lhs = self.parse_prefix(restricted)?;
        let mut chain = InfixChainState::default();

        loop {
            if !restricted
                && min_bp <= PREDICATE_BINDING_POWER
                && !chain.saw_special_predicate
                && self.at(TokenKind::Not)
                && self.peek_kind_n(1) == TokenKind::Completion
            {
                self.advance();
                self.record_completion_expression_continuation_tokens(NEGATED_PREDICATE_TOKENS);
                return self.fail("NOT requires IN, LIKE, ILIKE, SIMILAR, or BETWEEN");
            }
            if self.at_completion() {
                self.record_completion_infix(min_bp, restricted, &chain);
                break;
            }

            if let Some(operator) = FixedBinaryOperator::for_token(self.peek_kind()) {
                if operator.binding_power < min_bp {
                    break;
                }
                // A quantified comparison has the precedence of `Op` in
                // PostgreSQL and may be followed by another comparison
                // operator (for example, `1 = ANY (SELECT 1) = 2`), but it
                // may not follow one (`a = b = ANY (...)` is still invalid).
                let quantified = !restricted
                    && matches!(
                        self.peek_kind_n(1),
                        TokenKind::Any | TokenKind::Some | TokenKind::All
                    );
                if operator.is_comparison {
                    if chain.saw_comparison {
                        return self.fail("cannot chain comparison operators");
                    }
                    if !quantified {
                        chain.saw_comparison = true;
                    }
                }
                let location = self.advance().location();
                lhs = self.parse_binary_operator_rhs(
                    lhs,
                    vec![make_string_node(operator.name)],
                    operator.binding_power + 1,
                    restricted,
                    location,
                )?;
                continue;
            }

            lhs = match self.peek_kind() {
                TokenKind::Char('[') => {
                    if INDIRECTION_BINDING_POWER < min_bp {
                        break;
                    }
                    let index = self.parse_indirection_index()?;
                    append_indirection(lhs, index)
                }
                TokenKind::Char('.') => {
                    if INDIRECTION_BINDING_POWER < min_bp {
                        break;
                    }
                    self.advance();
                    let item = if self.consume(TokenKind::Char('*')) {
                        Node::AStar(AStar {
                            node_tag: NodeTag::AStar,
                        })
                    } else {
                        let name = self
                            .consume_column_label()
                            .or_else(|| self.fail("expected a field name after '.'"))?;
                        make_string_node(name)
                    };
                    append_indirection(lhs, item)
                }
                TokenKind::TypeCast => {
                    if TYPE_CAST_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let type_name = Some(Box::new(self.parse_cast_type_name()?));
                    Node::TypeCast(TypeCast {
                        node_tag: NodeTag::TypeCast,
                        arg: Some(Box::new(lhs)),
                        type_name,
                        location: location as ParseLoc,
                    })
                }
                TokenKind::Collate => {
                    if restricted || COLLATE_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    self.record_completion_slot(completion::GrammarSlot::Collation);
                    let collname = self.parse_name_nodes_with_slots(
                        &[completion::GrammarSlot::Collation],
                        false,
                    )?;
                    Node::CollateClause(CollateClause {
                        node_tag: NodeTag::CollateClause,
                        arg: Some(Box::new(lhs)),
                        collname,
                        location: location as ParseLoc,
                    })
                }
                kind @ (TokenKind::Isnull | TokenKind::Notnull) => {
                    if restricted || IS_BINDING_POWER < min_bp {
                        break;
                    }
                    if chain.saw_is_predicate {
                        return self.fail("cannot chain IS predicates");
                    }
                    chain.saw_is_predicate = true;
                    let location = self.advance().location();
                    Node::NullTest(NullTest {
                        xpr: Expr::new(NodeTag::NullTest),
                        arg: Some(Box::new(lhs)),
                        nulltesttype: if kind == TokenKind::Isnull {
                            NullTestType::Null
                        } else {
                            NullTestType::NotNull
                        },
                        location: location as ParseLoc,
                        ..NullTest::default()
                    })
                }
                TokenKind::At => {
                    if restricted || AT_TIME_ZONE_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let (args, call_location) = if self.consume(TokenKind::Time) {
                        self.expect(TokenKind::Zone)?;
                        let zone =
                            self.parse_expr_mode(AT_TIME_ZONE_BINDING_POWER + 1, restricted)?;
                        (vec![zone, lhs], location as ParseLoc)
                    } else {
                        self.expect(TokenKind::Local)?;
                        (vec![lhs], -1)
                    };
                    Node::FuncCall(FuncCall {
                        node_tag: NodeTag::FuncCall,
                        funcname: system_type_names("timezone"),
                        args,
                        funcformat: CoercionForm::SqlSyntax,
                        location: call_location,
                        ..FuncCall::default()
                    })
                }
                TokenKind::Or => {
                    if restricted || OR_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let rhs = self.parse_expr_mode(OR_BINDING_POWER + 1, restricted)?;
                    make_bool_expr(BoolExprType::OrExpr, lhs, rhs, location)
                }
                TokenKind::And => {
                    if restricted || AND_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.advance().location();
                    let rhs = self.parse_expr_mode(AND_BINDING_POWER + 1, restricted)?;
                    make_bool_expr(BoolExprType::AndExpr, lhs, rhs, location)
                }
                TokenKind::Not if NEGATED_PREDICATE_TOKENS.contains(&self.peek_kind_n(1)) => {
                    if restricted || PREDICATE_BINDING_POWER < min_bp {
                        break;
                    }
                    if chain.saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    chain.saw_special_predicate = true;
                    let location = self.advance().location();
                    let op = self.advance().kind;
                    self.parse_special_infix(lhs, op, true, location)?
                }
                TokenKind::InP
                | TokenKind::Like
                | TokenKind::Ilike
                | TokenKind::Similar
                | TokenKind::Between => {
                    if restricted || PREDICATE_BINDING_POWER < min_bp {
                        break;
                    }
                    if chain.saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    chain.saw_special_predicate = true;
                    let token = self.advance().clone();
                    self.parse_special_infix(lhs, token.kind, false, token.location())?
                }
                TokenKind::Is => {
                    if IS_BINDING_POWER < min_bp {
                        break;
                    }
                    if chain.saw_is_predicate {
                        return self.fail("cannot chain IS predicates");
                    }
                    chain.saw_is_predicate = true;
                    let location = self.advance().location();
                    self.parse_is_expr(lhs, location, expression_start, restricted)?
                }
                TokenKind::Overlaps => {
                    if restricted || PREDICATE_BINDING_POWER < min_bp {
                        break;
                    }
                    if chain.saw_special_predicate {
                        return self.fail("cannot chain non-associative predicates");
                    }
                    chain.saw_special_predicate = true;
                    let location = self.advance().location();
                    self.parse_overlaps(lhs, restricted, location)?
                }
                TokenKind::RightArrow | TokenKind::Char('|') | TokenKind::Op => {
                    if GENERIC_OPERATOR_BINDING_POWER < min_bp {
                        break;
                    }
                    let token = self.advance().clone();
                    let operator = token_name(&token).unwrap_or_else(|| token_text(&token));
                    self.parse_binary_operator_rhs(
                        lhs,
                        vec![make_string_node(operator)],
                        GENERIC_OPERATOR_BINDING_POWER + 1,
                        restricted,
                        token.location(),
                    )?
                }
                TokenKind::Operator => {
                    if GENERIC_OPERATOR_BINDING_POWER < min_bp {
                        break;
                    }
                    let location = self.location();
                    let name = self.parse_explicit_operator_name()?;
                    self.parse_binary_operator_rhs(
                        lhs,
                        name,
                        GENERIC_OPERATOR_BINDING_POWER + 1,
                        restricted,
                        location,
                    )?
                }
                _ => break,
            };
        }

        Some(lhs)
    }

    fn record_completion_infix(&self, min_bp: u8, restricted: bool, chain: &InfixChainState) {
        if min_bp <= INDIRECTION_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Char('['),
                TokenKind::Char('.'),
            ]);
        }
        if min_bp <= TYPE_CAST_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[TokenKind::TypeCast]);
            if !restricted && min_bp <= COLLATE_BINDING_POWER {
                self.record_completion_expression_continuation_tokens(&[TokenKind::Collate]);
            }
        }
        if !restricted && min_bp <= IS_BINDING_POWER && !chain.saw_is_predicate {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Isnull,
                TokenKind::Notnull,
            ]);
        }
        if !restricted && min_bp <= AT_TIME_ZONE_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[TokenKind::At]);
        }
        if min_bp <= EXPONENTIATION_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[TokenKind::Char('^')]);
        }
        if min_bp <= MULTIPLICATIVE_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Char('*'),
                TokenKind::Char('/'),
                TokenKind::Char('%'),
            ]);
        }
        if min_bp <= ADDITIVE_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Char('+'),
                TokenKind::Char('-'),
            ]);
        }
        if min_bp <= GENERIC_OPERATOR_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::RightArrow,
                TokenKind::Char('|'),
                TokenKind::Op,
                TokenKind::Operator,
            ]);
        }
        if !restricted && min_bp <= PREDICATE_BINDING_POWER && !chain.saw_special_predicate {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Not,
                TokenKind::Overlaps,
            ]);
            self.record_completion_expression_continuation_tokens(NEGATED_PREDICATE_TOKENS);
        }
        if min_bp <= COMPARISON_BINDING_POWER && !chain.saw_comparison {
            self.record_completion_expression_continuation_tokens(&[
                TokenKind::Char('='),
                TokenKind::Char('<'),
                TokenKind::Char('>'),
                TokenKind::LessEquals,
                TokenKind::GreaterEquals,
                TokenKind::NotEquals,
            ]);
        }
        if min_bp <= IS_BINDING_POWER && !chain.saw_is_predicate {
            self.record_completion_expression_continuation_tokens(&[TokenKind::Is]);
        }
        if !restricted && min_bp <= AND_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[TokenKind::And]);
        }
        if !restricted && min_bp <= OR_BINDING_POWER {
            self.record_completion_expression_continuation_tokens(&[TokenKind::Or]);
        }
    }

    fn parse_binary_operator_rhs(
        &mut self,
        lhs: Node,
        operator_name: NodeList,
        right_binding_power: u8,
        restricted: bool,
        location: usize,
    ) -> Option<Node> {
        if !restricted && self.quantified_sub_link_type().is_some() {
            self.parse_quantified_comparison_with_name(lhs, operator_name, location)
        } else {
            let rhs = self.parse_expr_mode(right_binding_power, restricted)?;
            Some(make_aexpr_with_name(
                AExprKind::Op,
                operator_name,
                Some(lhs),
                Some(rhs),
                location,
            ))
        }
    }
}
