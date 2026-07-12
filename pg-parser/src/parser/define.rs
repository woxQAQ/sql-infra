use super::*;

fn operator_name_nodes(tokens: Vec<Token>) -> NodeList {
    let names = tokens_to_name_nodes(&tokens);
    if names.is_empty() {
        vec![make_string_node(tokens_to_text(&tokens))]
    } else {
        names
    }
}

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createaggregate.html
    // CREATE [ OR REPLACE ] AGGREGATE name ( [ argmode ] [ argname ] arg_data_type [ , ... ] ) (
    //     SFUNC = sfunc,
    //     STYPE = state_data_type
    //     [ , SSPACE = state_data_size ]
    //     [ , FINALFUNC = ffunc ]
    //     [ , FINALFUNC_EXTRA ]
    //     [ , FINALFUNC_MODIFY = { READ_ONLY | SHAREABLE | READ_WRITE } ]
    //     [ , COMBINEFUNC = combinefunc ]
    //     [ , SERIALFUNC = serialfunc ]
    //     [ , DESERIALFUNC = deserialfunc ]
    //     [ , INITCOND = initial_condition ]
    //     [ , MSFUNC = msfunc ]
    //     [ , MINVFUNC = minvfunc ]
    //     [ , MSTYPE = mstate_data_type ]
    //     [ , MSSPACE = mstate_data_size ]
    //     [ , MFINALFUNC = mffunc ]
    //     [ , MFINALFUNC_EXTRA ]
    //     [ , MFINALFUNC_MODIFY = { READ_ONLY | SHAREABLE | READ_WRITE } ]
    //     [ , MINITCOND = minitial_condition ]
    //     [ , SORTOP = sort_operator ]
    //     [ , PARALLEL = { SAFE | RESTRICTED | UNSAFE } ]
    // )
    //
    // CREATE [ OR REPLACE ] AGGREGATE name ( [ [ argmode ] [ argname ] arg_data_type [ , ... ] ]
    //                         ORDER BY [ argmode ] [ argname ] arg_data_type [ , ... ] ) (
    //     SFUNC = sfunc,
    //     STYPE = state_data_type
    //     [ , SSPACE = state_data_size ]
    //     [ , FINALFUNC = ffunc ]
    //     [ , FINALFUNC_EXTRA ]
    //     [ , FINALFUNC_MODIFY = { READ_ONLY | SHAREABLE | READ_WRITE } ]
    //     [ , INITCOND = initial_condition ]
    //     [ , PARALLEL = { SAFE | RESTRICTED | UNSAFE } ]
    //     [ , HYPOTHETICAL ]
    // )
    //
    // or the old syntax
    //
    // CREATE [ OR REPLACE ] AGGREGATE name (
    //     BASETYPE = base_type,
    //     SFUNC = sfunc,
    //     STYPE = state_data_type
    //     [ , SSPACE = state_data_size ]
    //     [ , FINALFUNC = ffunc ]
    //     [ , FINALFUNC_EXTRA ]
    //     [ , FINALFUNC_MODIFY = { READ_ONLY | SHAREABLE | READ_WRITE } ]
    //     [ , COMBINEFUNC = combinefunc ]
    //     [ , SERIALFUNC = serialfunc ]
    //     [ , DESERIALFUNC = deserialfunc ]
    //     [ , INITCOND = initial_condition ]
    //     [ , MSFUNC = msfunc ]
    //     [ , MINVFUNC = minvfunc ]
    //     [ , MSTYPE = mstate_data_type ]
    //     [ , MSSPACE = mstate_data_size ]
    //     [ , MFINALFUNC = mffunc ]
    //     [ , MFINALFUNC_EXTRA ]
    //     [ , MFINALFUNC_MODIFY = { READ_ONLY | SHAREABLE | READ_WRITE } ]
    //     [ , MINITCOND = minitial_condition ]
    //     [ , SORTOP = sort_operator ]
    // )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createoperator.html
    // CREATE OPERATOR name (
    //     {FUNCTION|PROCEDURE} = function_name
    //     [, LEFTARG = left_type ] [, RIGHTARG = right_type ]
    //     [, COMMUTATOR = com_op ] [, NEGATOR = neg_op ]
    //     [, RESTRICT = res_proc ] [, JOIN = join_proc ]
    //     [, HASHES ] [, MERGES ]
    // )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createcollation.html
    // CREATE COLLATION [ IF NOT EXISTS ] name (
    //     [ LOCALE = locale, ]
    //     [ LC_COLLATE = lc_collate, ]
    //     [ LC_CTYPE = lc_ctype, ]
    //     [ PROVIDER = provider, ]
    //     [ DETERMINISTIC = boolean, ]
    //     [ RULES = rules, ]
    //     [ VERSION = version ]
    // )
    // CREATE COLLATION [ IF NOT EXISTS ] name FROM existing_collation
    pub(super) fn parse_define(&mut self, kind: ObjectType, replace: bool) -> PResult<Node> {
        self.advance();
        if replace && kind != ObjectType::Aggregate {
            return Err(self.error_here("OR REPLACE is only supported for CREATE AGGREGATE here"));
        }
        let mut if_not_exists = false;
        if kind == ObjectType::Collation {
            if_not_exists = self.consume_if_not_exists()?;
        }
        let (defnames, args, definition, oldstyle) = if kind == ObjectType::Aggregate {
            let defnames = self.parse_name_list();
            if defnames.is_empty() {
                return Err(self.error_here("CREATE AGGREGATE requires a name"));
            }
            self.expect(TokenKind::Char('('))?;
            let first = self.take_until_top_level(&[TokenKind::Char(')')]);
            self.expect(TokenKind::Char(')'))?;
            if self.at(TokenKind::Char('(')) {
                (
                    defnames,
                    parse_aggregate_args(first)?,
                    self.parse_parenthesized_definition()?,
                    false,
                )
            } else {
                (
                    defnames,
                    Vec::new(),
                    parse_old_aggregate_definition(first)?,
                    true,
                )
            }
        } else if kind == ObjectType::Operator {
            let tokens = self.take_until_top_level(&[TokenKind::Char('(')]);
            if tokens.is_empty() {
                return Err(self.error_here("CREATE OPERATOR requires an operator name"));
            }
            let defnames = operator_name_nodes(tokens);
            (
                defnames,
                Vec::new(),
                self.parse_parenthesized_definition()?,
                false,
            )
        } else {
            let defnames = self.parse_name_list_until_keywords(&[
                TokenKind::Char('('),
                TokenKind::From,
                TokenKind::Char(';'),
                TokenKind::Eof,
            ]);
            if defnames.is_empty() {
                return Err(self.error_here("CREATE COLLATION requires a name"));
            }
            let definition = if self.consume(TokenKind::From) {
                let from_location = self.location();
                let from = self.parse_name_list();
                if from.is_empty() {
                    return Err(self.error_here("COLLATION FROM requires a source collation"));
                }
                vec![make_def_elem(
                    "from",
                    Some(name_list_node(from)),
                    from_location,
                )]
            } else {
                self.parse_parenthesized_definition()?
            };
            (defnames, Vec::new(), definition, false)
        };
        Ok(Node::DefineStmt(DefineStmt {
            node_tag: NodeTag::DefineStmt,
            kind,
            oldstyle,
            defnames,
            args,
            definition,
            if_not_exists,
            replace,
        }))
    }
}
