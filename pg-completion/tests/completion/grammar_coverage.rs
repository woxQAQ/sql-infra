use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use pg_completion::CompletionKind;
use pg_parser::{CompletionSlot, TextSize, collect_completion};

use super::support::Fixture;

#[derive(Clone, Copy)]
#[expect(
    dead_code,
    reason = "the rejected Pending variant keeps the zero-pending CI invariant explicit"
)]
enum CoverageStatus {
    Covered {
        marked: &'static str,
        contract: Contract,
    },
    Pending {
        phase: &'static str,
        reason: &'static str,
    },
}

#[derive(Clone, Copy)]
enum Contract {
    Keyword(&'static str),
    KeywordSet(&'static [&'static str]),
    Relation,
    Column,
    Value,
    Declaration,
    FunctionReference,
    FromItem,
}

const COVERAGE: &[(CompletionSlot, CoverageStatus)] = &[
    (
        CompletionSlot::StatementStart,
        CoverageStatus::Covered {
            marked: "|",
            contract: Contract::Keyword("SELECT"),
        },
    ),
    (
        CompletionSlot::CreateObjectKind,
        CoverageStatus::Covered {
            marked: "CREATE |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        CompletionSlot::AlterObjectKind,
        CoverageStatus::Covered {
            marked: "ALTER |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        CompletionSlot::DropObjectKind,
        CoverageStatus::Covered {
            marked: "DROP |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        CompletionSlot::FromItem,
        CoverageStatus::Covered {
            marked: "SELECT * FROM |",
            contract: Contract::FromItem,
        },
    ),
    (
        CompletionSlot::SelectTarget,
        CoverageStatus::Covered {
            marked: "SELECT | FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectTargetAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT id, | FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectDistinctOn,
        CoverageStatus::Covered {
            marked: "SELECT DISTINCT ON (|) id FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectDistinctOnAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT DISTINCT ON (id, |) id FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectWhere,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectGroupBy,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users GROUP BY |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectGroupByAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users GROUP BY id, |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectHaving,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users HAVING |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectOrderBy,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users ORDER BY |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectOrderByAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users ORDER BY id, |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectLimit,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users LIMIT |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectOffset,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users OFFSET |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SelectFetchCount,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users FETCH FIRST | ROWS ONLY",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ValuesExpression,
        CoverageStatus::Covered {
            marked: "VALUES (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::ValuesExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "VALUES (1, |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::WindowPartitionExpression,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (PARTITION BY |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WindowPartitionExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (PARTITION BY id, |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WindowOrderExpression,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WindowOrderExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id, |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WindowFrameStartOffset,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS | PRECEDING)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WindowFrameEndOffset,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS BETWEEN 1 PRECEDING AND | FOLLOWING)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TableSampleArgument,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TableSampleArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(1, |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TableSampleRepeatable,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(1) REPEATABLE (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::RowsFromFunction,
        CoverageStatus::Covered {
            marked: "SELECT * FROM ROWS FROM (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::RowsFromFunctionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM ROWS FROM (count(1), |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JoinOn,
        CoverageStatus::Covered {
            marked: "SELECT * FROM users u JOIN orders o ON |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlTableNamespace,
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::XmlTableNamespaceAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(XMLNAMESPACES('/a' AS a, DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::XmlTableRowExpression,
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(| PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::XmlTableDocumentExpression,
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE('/x' PASSING | COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::FunctionArgument,
        CoverageStatus::Covered {
            marked: "SELECT count(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::FunctionArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::FunctionOrderBy,
        CoverageStatus::Covered {
            marked: "SELECT count(id ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::FunctionOrderByAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT count(id ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WithinGroupOrderBy,
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1) WITHIN GROUP (ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::WithinGroupOrderByAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1) WITHIN GROUP (ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::FunctionFilter,
        CoverageStatus::Covered {
            marked: "SELECT count(id) FILTER (WHERE |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ArrayElement,
        CoverageStatus::Covered {
            marked: "SELECT ARRAY[|] FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ArrayElementAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT ARRAY[id, |] FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ParenthesizedExpression,
        CoverageStatus::Covered {
            marked: "SELECT (|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ParenthesizedExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT (id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CoalesceArgument,
        CoverageStatus::Covered {
            marked: "SELECT COALESCE(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CoalesceArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT COALESCE(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MinmaxArgument,
        CoverageStatus::Covered {
            marked: "SELECT GREATEST(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MinmaxArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT LEAST(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::NullifArgument,
        CoverageStatus::Covered {
            marked: "SELECT NULLIF(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::NullifArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT NULLIF(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::InListExpression,
        CoverageStatus::Covered {
            marked: "SELECT id IN (|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::InListExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT id IN (1, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::GroupingArgument,
        CoverageStatus::Covered {
            marked: "SELECT GROUPING(|) FROM users GROUP BY id",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::GroupingArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT GROUPING(id, |) FROM users GROUP BY id",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CaseOperand,
        CoverageStatus::Covered {
            marked: "SELECT CASE | WHEN 1 THEN 2 END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CaseWhenCondition,
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN | THEN 1 END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CaseThenResult,
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN true THEN | END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CaseElseResult,
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN true THEN 1 ELSE | END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CastArgument,
        CoverageStatus::Covered {
            marked: "SELECT CAST(| AS integer) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ExtractArgument,
        CoverageStatus::Covered {
            marked: "SELECT EXTRACT(YEAR FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::NormalizeArgument,
        CoverageStatus::Covered {
            marked: "SELECT NORMALIZE(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::PositionNeedle,
        CoverageStatus::Covered {
            marked: "SELECT POSITION(| IN name) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::PositionHaystack,
        CoverageStatus::Covered {
            marked: "SELECT POSITION('x' IN |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OverlaySource,
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(| PLACING 'x' FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OverlayReplacement,
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING | FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OverlayStart,
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING 'x' FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OverlayCount,
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING 'x' FROM 1 FOR |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SubstringSource,
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(| FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SubstringStart,
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SubstringCount,
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name FROM 1 FOR |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SubstringPattern,
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name SIMILAR | ESCAPE '#') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::SubstringEscape,
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name SIMILAR '%#\"o#\"%' ESCAPE |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TrimArgument,
        CoverageStatus::Covered {
            marked: "SELECT TRIM(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TrimArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT TRIM(name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TrimSource,
        CoverageStatus::Covered {
            marked: "SELECT TRIM('x' FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TrimSourceAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT TRIM('x' FROM name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlExistsXpath,
        CoverageStatus::Covered {
            marked: "SELECT XMLEXISTS(| PASSING '<x/>') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlExistsDocument,
        CoverageStatus::Covered {
            marked: "SELECT XMLEXISTS('/x' PASSING |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::RowElement,
        CoverageStatus::Covered {
            marked: "SELECT ROW(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::RowElementAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT ROW(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlConcatArgument,
        CoverageStatus::Covered {
            marked: "SELECT XMLCONCAT(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlConcatArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT XMLCONCAT(name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlElementContent,
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlElementContentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlAttributeExpression,
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(|)) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlAttributeExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(id, |)) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlForestExpression,
        CoverageStatus::Covered {
            marked: "SELECT XMLFOREST(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlForestExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT XMLFOREST(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlParseValue,
        CoverageStatus::Covered {
            marked: "SELECT XMLPARSE(DOCUMENT |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlPiValue,
        CoverageStatus::Covered {
            marked: "SELECT XMLPI(NAME item, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlRootDocument,
        CoverageStatus::Covered {
            marked: "SELECT XMLROOT(|, VERSION '1.0') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlRootVersion,
        CoverageStatus::Covered {
            marked: "SELECT XMLROOT(name, VERSION |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::XmlSerializeValue,
        CoverageStatus::Covered {
            marked: "SELECT XMLSERIALIZE(CONTENT | AS text) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ExecuteParameter,
        CoverageStatus::Covered {
            marked: "EXECUTE prepared_statement(|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::ExecuteParameterAfterComma,
        CoverageStatus::Covered {
            marked: "EXECUTE prepared_statement(1, |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionListValue,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionListValueAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (1, |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionRangeFromValue,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (|) TO (10)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionRangeFromValueAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, |) TO (10, 20)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionRangeToValue,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1) TO (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionRangeToValueAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, 2) TO (10, |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::MergeInsertValue,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeInsertValueAfterComma,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (u.id, |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ReturningExpression,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' RETURNING |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ReturningExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' RETURNING id, |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::GraphTableColumnExpression,
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (|))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::GraphTableColumnExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (p.id, |))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PropertyGraphPropertyExpression,
        CoverageStatus::Covered {
            marked: "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (|))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PropertyGraphPropertyExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (id, |))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JsonArrayAggOrderBy,
        CoverageStatus::Covered {
            marked: "SELECT JSON_ARRAYAGG(id ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::JsonArrayAggOrderByAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT JSON_ARRAYAGG(id ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CteName,
        CoverageStatus::Covered {
            marked: "WITH | AS (SELECT 1) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        CompletionSlot::CteAliasColumn,
        CoverageStatus::Covered {
            marked: "WITH recent(|) AS (SELECT 1) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        CompletionSlot::CteAliasColumnAfterComma,
        CoverageStatus::Covered {
            marked: "WITH recent(id, |) AS (SELECT 1, 2) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        CompletionSlot::CteContinuation,
        CoverageStatus::Covered {
            marked: "WITH recent AS (SELECT 1) |",
            contract: Contract::KeywordSet(&[
                "CYCLE", "DELETE", "INSERT", "MERGE", "SEARCH", "SELECT", "TABLE", "UPDATE",
                "VALUES",
            ]),
        },
    ),
    (
        CompletionSlot::InsertTargetRelation,
        CoverageStatus::Covered {
            marked: "INSERT INTO |",
            contract: Contract::Relation,
        },
    ),
    (
        CompletionSlot::UpdateTargetRelation,
        CoverageStatus::Covered {
            marked: "UPDATE |",
            contract: Contract::Relation,
        },
    ),
    (
        CompletionSlot::DeleteTargetRelation,
        CoverageStatus::Covered {
            marked: "DELETE FROM |",
            contract: Contract::Relation,
        },
    ),
    (
        CompletionSlot::IndexRelation,
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON |",
            contract: Contract::Relation,
        },
    ),
    (
        CompletionSlot::AlterTableRelation,
        CoverageStatus::Covered {
            marked: "ALTER TABLE |",
            contract: Contract::Relation,
        },
    ),
    (
        CompletionSlot::UpdateSetTarget,
        CoverageStatus::Covered {
            marked: "UPDATE users SET |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::UpdateSetTargetAfterComma,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::UpdateSetValue,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::UpdateWhere,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::DeleteWhere,
        CoverageStatus::Covered {
            marked: "DELETE FROM users WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ForPortionTarget,
        CoverageStatus::Covered {
            marked: "DELETE FROM users FOR PORTION OF valid_time (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ForPortionStart,
        CoverageStatus::Covered {
            marked: "UPDATE users FOR PORTION OF valid_time FROM | TO 10 SET name = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ForPortionEnd,
        CoverageStatus::Covered {
            marked: "UPDATE users FOR PORTION OF valid_time FROM 1 TO | SET name = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictInferenceWhere,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id) WHERE | DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictSetTarget,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictSetTargetAfterComma,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictSetValue,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictUpdateWhere,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x' WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictSelectWhere,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO SELECT WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeJoinCondition,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON | WHEN MATCHED THEN DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeWhenCondition,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED AND | THEN DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeSetTarget,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeSetTargetAfterComma,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::MergeSetValue,
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AssignmentSubscriptLowerOrIndex,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name[|] = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AssignmentSliceUpper,
        CoverageStatus::Covered {
            marked: "UPDATE users SET name[1:|] = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AlterColumnUsing,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name TYPE text USING |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AlterColumnDefault,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name SET DEFAULT |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AlterColumnExpression,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name SET EXPRESSION AS (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::PublicationRowFilter,
        CoverageStatus::Covered {
            marked: "CREATE PUBLICATION p FOR TABLE users WHERE (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::RuleWhere,
        CoverageStatus::Covered {
            marked: "CREATE RULE r AS ON UPDATE TO users WHERE | DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ColumnDefault,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::ColumnCheck,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ColumnGenerated,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer GENERATED ALWAYS AS (|) STORED",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TableCheck,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ExclusionWhere,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =) WHERE (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::TriggerWhen,
        CoverageStatus::Covered {
            marked: "CREATE TRIGGER users_trigger BEFORE UPDATE ON users FOR EACH ROW WHEN (|) EXECUTE FUNCTION calculate_total()",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::IndexPredicate,
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (id) WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::DomainDefault,
        CoverageStatus::Covered {
            marked: "CREATE DOMAIN positive_integer AS integer DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::DomainCheck,
        CoverageStatus::Covered {
            marked: "CREATE DOMAIN positive_integer AS integer CHECK (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::AlterDomainDefault,
        CoverageStatus::Covered {
            marked: "ALTER DOMAIN positive_integer SET DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::AlterDomainCheck,
        CoverageStatus::Covered {
            marked: "ALTER DOMAIN positive_integer ADD CHECK (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::CopyWhere,
        CoverageStatus::Covered {
            marked: "COPY users FROM STDIN WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CreatePolicyUsing,
        CoverageStatus::Covered {
            marked: "CREATE POLICY users_policy ON users USING (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CreatePolicyCheck,
        CoverageStatus::Covered {
            marked: "CREATE POLICY users_policy ON users WITH CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AlterPolicyUsing,
        CoverageStatus::Covered {
            marked: "ALTER POLICY users_policy ON users USING (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::AlterPolicyCheck,
        CoverageStatus::Covered {
            marked: "ALTER POLICY users_policy ON users WITH CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ReturnExpression,
        CoverageStatus::Covered {
            marked: "CREATE FUNCTION f() RETURNS integer BEGIN ATOMIC RETURN |; END",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::GraphTableWhere,
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) WHERE | COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::GraphPathWhere,
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH ((p) WHERE |) COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::GraphElementWhere,
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p WHERE |) COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JsonTableContext,
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE(|, '$' COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JsonTablePassingArgument,
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' PASSING | AS value COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JsonTablePassingArgumentAfterComma,
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' PASSING 1 AS first, | AS second COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::StatisticsExpression,
        CoverageStatus::Covered {
            marked: "CREATE STATISTICS s ON | FROM users",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::StatisticsExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE STATISTICS s ON id, | FROM users",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::CreateIndexElement,
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (|)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::CreateIndexElementAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (id, |)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ExclusionElement,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (| WITH =)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::ExclusionElementAfterComma,
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =, | WITH =)",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictInferenceElement,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (|) DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::OnConflictInferenceElementAfterComma,
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id, |) DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        CompletionSlot::PartitionKeyExpression,
        CoverageStatus::Covered {
            marked: "CREATE TABLE partitioned (id integer) PARTITION BY RANGE (|)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::PartitionKeyExpressionAfterComma,
        CoverageStatus::Covered {
            marked: "CREATE TABLE partitioned (id integer, name text) PARTITION BY RANGE (id, |)",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::JsonTableDefaultBehavior,
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' COLUMNS (id integer PATH '$.id' DEFAULT | ON EMPTY))",
            contract: Contract::Value,
        },
    ),
    (
        CompletionSlot::CallRoutine,
        CoverageStatus::Covered {
            marked: "CALL |",
            contract: Contract::FunctionReference,
        },
    ),
    (
        CompletionSlot::InsertColumn,
        CoverageStatus::Covered {
            marked: "INSERT INTO users(|) VALUES ('x')",
            contract: Contract::Column,
        },
    ),
];

#[test]
fn every_registered_completion_slot_is_classified_once() {
    let mut classified = HashSet::new();
    for (slot, status) in COVERAGE {
        assert!(
            classified.insert(*slot),
            "duplicate coverage entry for {slot:?}"
        );
        if let CoverageStatus::Pending { phase, reason } = status {
            panic!(
                "completion slot {slot:?} is pending ({phase}: {reason}); CI requires pending = 0"
            );
        }
    }
    assert_eq!(
        classified,
        CompletionSlot::ALL.iter().copied().collect(),
        "the coverage matrix must classify every registered completion slot"
    );
}

#[test]
fn covered_slots_reach_their_semantic_contract() {
    let fixture = Fixture::default();
    for (slot, status) in COVERAGE {
        let CoverageStatus::Covered { marked, contract } = status else {
            continue;
        };
        let (sql, cursor) = marked_sql(marked);
        let context = collect_completion(&sql, TextSize::try_from(cursor).unwrap()).unwrap();
        assert!(
            context.slots.contains(slot),
            "{marked:?} did not reach registered slot {slot:?}; got {:?}",
            context.slots
        );

        let completed = fixture.complete(marked);
        completed
            .assert_replacement_contract()
            .assert_no_duplicate_items();
        match contract {
            Contract::Keyword(keyword) => {
                completed.assert_has(keyword, CompletionKind::Keyword);
            }
            Contract::KeywordSet(expected) => {
                let actual = completed
                    .result
                    .items
                    .iter()
                    .filter(|item| item.kind == CompletionKind::Keyword)
                    .map(|item| item.label.as_str())
                    .collect::<HashSet<_>>();
                assert_eq!(
                    actual,
                    expected.iter().copied().collect(),
                    "keyword continuation set for {marked:?}"
                );
            }
            Contract::Relation => {
                completed.assert_has("users", CompletionKind::Table);
            }
            Contract::Column => {
                completed.assert_has("name", CompletionKind::Column);
            }
            Contract::Value => {
                completed
                    .assert_has("count", CompletionKind::Function)
                    .assert_has("NULL", CompletionKind::Keyword);
            }
            Contract::Declaration => {
                assert!(
                    completed
                        .result
                        .items
                        .iter()
                        .all(|item| !matches!(item.kind, CompletionKind::Catalog(_))),
                    "declaration slot leaked catalog candidates for {marked:?}"
                );
                completed.assert_incomplete(false);
            }
            Contract::FunctionReference => {
                completed
                    .assert_has("calculate_total", CompletionKind::Function)
                    .assert_lacks_kind(CompletionKind::Column)
                    .assert_lacks_kind(CompletionKind::Table);
            }
            Contract::FromItem => {
                completed
                    .assert_has("users", CompletionKind::Table)
                    .assert_has("count", CompletionKind::Function)
                    .assert_has("LATERAL", CompletionKind::Keyword);
            }
        }
    }
}

#[test]
fn implicit_completion_recording_is_forbidden() {
    let sources = parser_sources();
    for pattern in [
        "record_completion(",
        "record_expression_completion(",
        "record_restricted_expression_completion(",
        ".record_expression(",
        ".record_restricted_expression(",
    ] {
        let actual = sources.matches(pattern).count();
        assert_eq!(
            actual, 0,
            "implicit completion path {pattern:?} is forbidden; register a semantic CompletionSlot instead"
        );
    }
}

#[test]
fn partial_expressions_preserve_their_outer_semantic_slot() {
    let marked = "SELECT * FROM users WHERE NOT |";
    let (sql, cursor) = marked_sql(marked);
    let context = collect_completion(&sql, TextSize::try_from(cursor).unwrap()).unwrap();
    assert!(context.slots.contains(&CompletionSlot::SelectWhere));
    Fixture::default()
        .complete(marked)
        .assert_has("name", CompletionKind::Column);
}

fn marked_sql(marked: &str) -> (String, usize) {
    let cursor = marked
        .find('|')
        .expect("coverage case needs a cursor marker");
    assert_eq!(
        marked.rfind('|'),
        Some(cursor),
        "one cursor marker required"
    );
    (marked.replacen('|', "", 1), cursor)
}

fn parser_sources() -> String {
    let parser_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-parser/src");
    let mut files = vec![parser_root.join("parser.rs")];
    collect_rust_files(&parser_root.join("parser"), &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap_or_else(|error| panic!("{directory:?}: {error}")) {
        let path = entry.expect("parser source entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
