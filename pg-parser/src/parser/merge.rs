//! `MERGE` statement parsing.
//!
//! Target/source relations, join conditions, aliases, and ordered actions combine
//! with the shared DML action grammar in `dml_grammar`.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-merge.html
    // [ WITH with_query [, ...] ]
    // MERGE INTO [ ONLY ] target_table_name [ * ] [ [ AS ] target_alias ]
    //     USING data_source ON join_condition
    //     when_clause [...]
    //     [ RETURNING [ WITH ( { OLD | NEW } AS output_alias [, ...] ) ]
    //                 { * | output_expression [ [ AS ] output_name ] } [, ...] ]
    //
    // where data_source is:
    //
    //     { [ ONLY ] source_table_name [ * ] | ( source_query ) } [ [ AS ] source_alias ]
    //
    // and when_clause is:
    //
    //     { WHEN MATCHED [ AND condition ] THEN { merge_update | merge_delete | DO NOTHING } |
    //       WHEN NOT MATCHED BY SOURCE [ AND condition ] THEN { merge_update | merge_delete | DO
    // NOTHING } |       WHEN NOT MATCHED [ BY TARGET ] [ AND condition ] THEN { merge_insert |
    // DO NOTHING } }
    //
    // and merge_insert is:
    //
    //     INSERT [( column_name [, ...] )]
    //         [ OVERRIDING { SYSTEM | USER } VALUE ]
    //         { VALUES ( { expression | DEFAULT } [, ...] ) | DEFAULT VALUES }
    //
    // and merge_update is:
    //
    //     UPDATE SET { column_name = { expression | DEFAULT } |
    //                  ( column_name [, ...] ) = [ ROW ] ( { expression | DEFAULT } [, ...] ) |
    //                  ( column_name [, ...] ) = ( sub-SELECT )
    //                } [, ...]
    //
    // and merge_delete is:
    //
    //     DELETE
    pub(super) fn parse_merge(&mut self, with_clause: Option<WithClause>) -> PResult<Node> {
        self.expect(TokenKind::Merge)?;
        self.expect(TokenKind::Into)?;
        let relation = Some(Box::new(
            self.try_parse_range_var_with_slot(true, completion::GrammarSlot::Table)?
                .ok_or_else(|| self.error_here("MERGE INTO requires a relation name"))?,
        ));
        self.expect(TokenKind::Using)?;
        let source_relation = Some(Box::new(self.parse_from_item(&[TokenKind::On])?));
        self.expect(TokenKind::On)?;
        let join_condition = self.parse_expr_box_strict_until(&[
            TokenKind::When,
            TokenKind::Returning,
            TokenKind::Char(';'),
            TokenKind::Eof,
        ])?;
        let merge_when_clauses = self.parse_merge_when_clauses()?;
        if merge_when_clauses.is_empty() {
            return Err(self.error_here("MERGE requires at least one WHEN clause"));
        }
        let returning_clause = self.parse_returning_clause()?;
        Ok(node!(MergeStmt {
            relation,
            source_relation,
            join_condition: Some(join_condition),
            merge_when_clauses,
            returning_clause,
            with_clause: with_clause.map(Box::new),
        }))
    }
}
