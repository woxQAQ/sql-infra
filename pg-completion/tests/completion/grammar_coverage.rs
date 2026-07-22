use std::collections::HashSet;

use pg_completion::CompletionKind;

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
    JoinUsingColumn,
    Type,
    Value,
    Declaration,
    FunctionReference,
    FromItem,
}

const COVERAGE: &[(&str, CoverageStatus)] = &[
    (
        "StatementStart",
        CoverageStatus::Covered {
            marked: "|",
            contract: Contract::Keyword("SELECT"),
        },
    ),
    (
        "CreateObjectKind",
        CoverageStatus::Covered {
            marked: "CREATE |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        "AlterObjectKind",
        CoverageStatus::Covered {
            marked: "ALTER |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        "DropObjectKind",
        CoverageStatus::Covered {
            marked: "DROP |",
            contract: Contract::Keyword("TABLE"),
        },
    ),
    (
        "FromItem",
        CoverageStatus::Covered {
            marked: "SELECT * FROM |",
            contract: Contract::FromItem,
        },
    ),
    (
        "SelectTarget",
        CoverageStatus::Covered {
            marked: "SELECT | FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SelectTargetAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT id, | FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SelectDistinctOn",
        CoverageStatus::Covered {
            marked: "SELECT DISTINCT ON (|) id FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SelectDistinctOnAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT DISTINCT ON (id, |) id FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SelectWhere",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectGroupBy",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users GROUP BY |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectGroupByAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users GROUP BY id, |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectHaving",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users HAVING |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectOrderBy",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users ORDER BY |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectOrderByAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users ORDER BY id, |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectLimit",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users LIMIT |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectOffset",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users OFFSET |",
            contract: Contract::Column,
        },
    ),
    (
        "SelectFetchCount",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users FETCH FIRST | ROWS ONLY",
            contract: Contract::Column,
        },
    ),
    (
        "ValuesExpression",
        CoverageStatus::Covered {
            marked: "VALUES (|)",
            contract: Contract::Value,
        },
    ),
    (
        "ValuesExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "VALUES (1, |)",
            contract: Contract::Value,
        },
    ),
    (
        "WindowPartitionExpression",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (PARTITION BY |)",
            contract: Contract::Column,
        },
    ),
    (
        "WindowPartitionExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (PARTITION BY id, |)",
            contract: Contract::Column,
        },
    ),
    (
        "WindowOrderExpression",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY |)",
            contract: Contract::Column,
        },
    ),
    (
        "WindowOrderExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id, |)",
            contract: Contract::Column,
        },
    ),
    (
        "WindowFrameStartOffset",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS | PRECEDING)",
            contract: Contract::Column,
        },
    ),
    (
        "WindowFrameEndOffset",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS BETWEEN 1 PRECEDING AND | FOLLOWING)",
            contract: Contract::Column,
        },
    ),
    (
        "TableSampleArgument",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(|)",
            contract: Contract::Column,
        },
    ),
    (
        "TableSampleArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(1, |)",
            contract: Contract::Column,
        },
    ),
    (
        "TableSampleRepeatable",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users TABLESAMPLE system(1) REPEATABLE (|)",
            contract: Contract::Column,
        },
    ),
    (
        "RowsFromFunction",
        CoverageStatus::Covered {
            marked: "SELECT * FROM ROWS FROM (|)",
            contract: Contract::Value,
        },
    ),
    (
        "RowsFromFunctionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM ROWS FROM (count(1), |)",
            contract: Contract::Value,
        },
    ),
    (
        "JoinOn",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users u JOIN orders o ON |",
            contract: Contract::Column,
        },
    ),
    (
        "XmlTableNamespace",
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        "XmlTableNamespaceAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(XMLNAMESPACES('/a' AS a, DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        "XmlTableRowExpression",
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE(| PASSING '<x/>' COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        "XmlTableDocumentExpression",
        CoverageStatus::Covered {
            marked: "SELECT * FROM XMLTABLE('/x' PASSING | COLUMNS id integer)",
            contract: Contract::Value,
        },
    ),
    (
        "FunctionArgument",
        CoverageStatus::Covered {
            marked: "SELECT count(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "FunctionArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "FunctionOrderBy",
        CoverageStatus::Covered {
            marked: "SELECT count(id ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "FunctionOrderByAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT count(id ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "WithinGroupOrderBy",
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1) WITHIN GROUP (ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "WithinGroupOrderByAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT calculate_total(1) WITHIN GROUP (ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "FunctionFilter",
        CoverageStatus::Covered {
            marked: "SELECT count(id) FILTER (WHERE |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ArrayElement",
        CoverageStatus::Covered {
            marked: "SELECT ARRAY[|] FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ArrayElementAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT ARRAY[id, |] FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ParenthesizedExpression",
        CoverageStatus::Covered {
            marked: "SELECT (|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ParenthesizedExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT (id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CoalesceArgument",
        CoverageStatus::Covered {
            marked: "SELECT COALESCE(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CoalesceArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT COALESCE(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "MinmaxArgument",
        CoverageStatus::Covered {
            marked: "SELECT GREATEST(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "MinmaxArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT LEAST(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "NullifArgument",
        CoverageStatus::Covered {
            marked: "SELECT NULLIF(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "NullifArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT NULLIF(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "InListExpression",
        CoverageStatus::Covered {
            marked: "SELECT id IN (|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "InListExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT id IN (1, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "GroupingArgument",
        CoverageStatus::Covered {
            marked: "SELECT GROUPING(|) FROM users GROUP BY id",
            contract: Contract::Column,
        },
    ),
    (
        "GroupingArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT GROUPING(id, |) FROM users GROUP BY id",
            contract: Contract::Column,
        },
    ),
    (
        "CaseOperand",
        CoverageStatus::Covered {
            marked: "SELECT CASE | WHEN 1 THEN 2 END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CaseWhenCondition",
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN | THEN 1 END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CaseThenResult",
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN true THEN | END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CaseElseResult",
        CoverageStatus::Covered {
            marked: "SELECT CASE WHEN true THEN 1 ELSE | END FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CastArgument",
        CoverageStatus::Covered {
            marked: "SELECT CAST(| AS integer) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ExtractArgument",
        CoverageStatus::Covered {
            marked: "SELECT EXTRACT(YEAR FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "NormalizeArgument",
        CoverageStatus::Covered {
            marked: "SELECT NORMALIZE(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "PositionNeedle",
        CoverageStatus::Covered {
            marked: "SELECT POSITION(| IN name) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "PositionHaystack",
        CoverageStatus::Covered {
            marked: "SELECT POSITION('x' IN |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "OverlaySource",
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(| PLACING 'x' FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "OverlayReplacement",
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING | FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "OverlayStart",
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING 'x' FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "OverlayCount",
        CoverageStatus::Covered {
            marked: "SELECT OVERLAY(name PLACING 'x' FROM 1 FOR |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SubstringSource",
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(| FROM 1) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SubstringStart",
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SubstringCount",
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name FROM 1 FOR |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SubstringPattern",
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name SIMILAR | ESCAPE '#') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "SubstringEscape",
        CoverageStatus::Covered {
            marked: "SELECT SUBSTRING(name SIMILAR '%#\"o#\"%' ESCAPE |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "TrimArgument",
        CoverageStatus::Covered {
            marked: "SELECT TRIM(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "TrimArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT TRIM(name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "TrimSource",
        CoverageStatus::Covered {
            marked: "SELECT TRIM('x' FROM |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "TrimSourceAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT TRIM('x' FROM name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlExistsXpath",
        CoverageStatus::Covered {
            marked: "SELECT XMLEXISTS(| PASSING '<x/>') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlExistsDocument",
        CoverageStatus::Covered {
            marked: "SELECT XMLEXISTS('/x' PASSING |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "RowElement",
        CoverageStatus::Covered {
            marked: "SELECT ROW(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "RowElementAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT ROW(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlConcatArgument",
        CoverageStatus::Covered {
            marked: "SELECT XMLCONCAT(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlConcatArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT XMLCONCAT(name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlElementContent",
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlElementContentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlAttributeExpression",
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(|)) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlAttributeExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(id, |)) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlForestExpression",
        CoverageStatus::Covered {
            marked: "SELECT XMLFOREST(|) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlForestExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT XMLFOREST(id, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlParseValue",
        CoverageStatus::Covered {
            marked: "SELECT XMLPARSE(DOCUMENT |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlPiValue",
        CoverageStatus::Covered {
            marked: "SELECT XMLPI(NAME item, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlRootDocument",
        CoverageStatus::Covered {
            marked: "SELECT XMLROOT(|, VERSION '1.0') FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlRootVersion",
        CoverageStatus::Covered {
            marked: "SELECT XMLROOT(name, VERSION |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "XmlSerializeValue",
        CoverageStatus::Covered {
            marked: "SELECT XMLSERIALIZE(CONTENT | AS text) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "ExecuteParameter",
        CoverageStatus::Covered {
            marked: "EXECUTE prepared_statement(|)",
            contract: Contract::Value,
        },
    ),
    (
        "ExecuteParameterAfterComma",
        CoverageStatus::Covered {
            marked: "EXECUTE prepared_statement(1, |)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionListValue",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (|)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionListValueAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (1, |)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionRangeFromValue",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (|) TO (10)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionRangeFromValueAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, |) TO (10, 20)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionRangeToValue",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1) TO (|)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionRangeToValueAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, 2) TO (10, |)",
            contract: Contract::Value,
        },
    ),
    (
        "MergeInsertValue",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (|)",
            contract: Contract::Column,
        },
    ),
    (
        "MergeInsertValueAfterComma",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (u.id, |)",
            contract: Contract::Column,
        },
    ),
    (
        "ReturningExpression",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' RETURNING |",
            contract: Contract::Column,
        },
    ),
    (
        "ReturningExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' RETURNING id, |",
            contract: Contract::Column,
        },
    ),
    (
        "GraphTableColumnExpression",
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (|))",
            contract: Contract::Value,
        },
    ),
    (
        "GraphTableColumnExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (p.id, |))",
            contract: Contract::Value,
        },
    ),
    (
        "PropertyGraphPropertyExpression",
        CoverageStatus::Covered {
            marked: "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (|))",
            contract: Contract::Value,
        },
    ),
    (
        "PropertyGraphPropertyExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (id, |))",
            contract: Contract::Value,
        },
    ),
    (
        "JsonArrayAggOrderBy",
        CoverageStatus::Covered {
            marked: "SELECT JSON_ARRAYAGG(id ORDER BY |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "JsonArrayAggOrderByAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT JSON_ARRAYAGG(id ORDER BY name, |) FROM users",
            contract: Contract::Column,
        },
    ),
    (
        "CteName",
        CoverageStatus::Covered {
            marked: "WITH | AS (SELECT 1) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        "CteAliasColumn",
        CoverageStatus::Covered {
            marked: "WITH recent(|) AS (SELECT 1) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        "CteAliasColumnAfterComma",
        CoverageStatus::Covered {
            marked: "WITH recent(id, |) AS (SELECT 1, 2) SELECT 1",
            contract: Contract::Declaration,
        },
    ),
    (
        "CteContinuation",
        CoverageStatus::Covered {
            marked: "WITH recent AS (SELECT 1) |",
            contract: Contract::KeywordSet(&[
                "CYCLE", "DELETE", "INSERT", "MERGE", "SEARCH", "SELECT", "TABLE", "UPDATE",
                "VALUES",
            ]),
        },
    ),
    (
        "InsertTargetRelation",
        CoverageStatus::Covered {
            marked: "INSERT INTO |",
            contract: Contract::Relation,
        },
    ),
    (
        "UpdateTargetRelation",
        CoverageStatus::Covered {
            marked: "UPDATE |",
            contract: Contract::Relation,
        },
    ),
    (
        "DeleteTargetRelation",
        CoverageStatus::Covered {
            marked: "DELETE FROM |",
            contract: Contract::Relation,
        },
    ),
    (
        "IndexRelation",
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON |",
            contract: Contract::Relation,
        },
    ),
    (
        "AlterTableRelation",
        CoverageStatus::Covered {
            marked: "ALTER TABLE |",
            contract: Contract::Relation,
        },
    ),
    (
        "UpdateSetTarget",
        CoverageStatus::Covered {
            marked: "UPDATE users SET |",
            contract: Contract::Column,
        },
    ),
    (
        "UpdateSetTargetAfterComma",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        "UpdateSetValue",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        "UpdateWhere",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name = 'x' WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "DeleteWhere",
        CoverageStatus::Covered {
            marked: "DELETE FROM users WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "ForPortionTarget",
        CoverageStatus::Covered {
            marked: "DELETE FROM users FOR PORTION OF valid_time (|)",
            contract: Contract::Column,
        },
    ),
    (
        "ForPortionStart",
        CoverageStatus::Covered {
            marked: "UPDATE users FOR PORTION OF valid_time FROM | TO 10 SET name = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        "ForPortionEnd",
        CoverageStatus::Covered {
            marked: "UPDATE users FOR PORTION OF valid_time FROM 1 TO | SET name = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictInferenceWhere",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id) WHERE | DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictSetTarget",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET |",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictSetTargetAfterComma",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictSetValue",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictUpdateWhere",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x' WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictSelectWhere",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO SELECT WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "MergeJoinCondition",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON | WHEN MATCHED THEN DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "MergeWhenCondition",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED AND | THEN DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "MergeSetTarget",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET |",
            contract: Contract::Column,
        },
    ),
    (
        "MergeSetTargetAfterComma",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = 'x', |",
            contract: Contract::Column,
        },
    ),
    (
        "MergeSetValue",
        CoverageStatus::Covered {
            marked: "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = |",
            contract: Contract::Column,
        },
    ),
    (
        "AssignmentSubscriptLowerOrIndex",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name[|] = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        "AssignmentSliceUpper",
        CoverageStatus::Covered {
            marked: "UPDATE users SET name[1:|] = 'x'",
            contract: Contract::Column,
        },
    ),
    (
        "AlterColumnUsing",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name TYPE text USING |",
            contract: Contract::Column,
        },
    ),
    (
        "AlterColumnDefault",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name SET DEFAULT |",
            contract: Contract::Column,
        },
    ),
    (
        "AlterColumnExpression",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ALTER COLUMN name SET EXPRESSION AS (|)",
            contract: Contract::Column,
        },
    ),
    (
        "PublicationRowFilter",
        CoverageStatus::Covered {
            marked: "CREATE PUBLICATION p FOR TABLE users WHERE (|)",
            contract: Contract::Column,
        },
    ),
    (
        "RuleWhere",
        CoverageStatus::Covered {
            marked: "CREATE RULE r AS ON UPDATE TO users WHERE | DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "ColumnDefault",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        "ColumnCheck",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        "ColumnGenerated",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD COLUMN value integer GENERATED ALWAYS AS (|) STORED",
            contract: Contract::Column,
        },
    ),
    (
        "TableCheck",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        "ExclusionWhere",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =) WHERE (|)",
            contract: Contract::Column,
        },
    ),
    (
        "TriggerWhen",
        CoverageStatus::Covered {
            marked: "CREATE TRIGGER users_trigger BEFORE UPDATE ON users FOR EACH ROW WHEN (|) EXECUTE FUNCTION calculate_total()",
            contract: Contract::Column,
        },
    ),
    (
        "IndexPredicate",
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (id) WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "DomainDefault",
        CoverageStatus::Covered {
            marked: "CREATE DOMAIN positive_integer AS integer DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        "DomainCheck",
        CoverageStatus::Covered {
            marked: "CREATE DOMAIN positive_integer AS integer CHECK (|)",
            contract: Contract::Value,
        },
    ),
    (
        "AlterDomainDefault",
        CoverageStatus::Covered {
            marked: "ALTER DOMAIN positive_integer SET DEFAULT |",
            contract: Contract::Value,
        },
    ),
    (
        "AlterDomainCheck",
        CoverageStatus::Covered {
            marked: "ALTER DOMAIN positive_integer ADD CHECK (|)",
            contract: Contract::Value,
        },
    ),
    (
        "CopyWhere",
        CoverageStatus::Covered {
            marked: "COPY users FROM STDIN WHERE |",
            contract: Contract::Column,
        },
    ),
    (
        "CreatePolicyUsing",
        CoverageStatus::Covered {
            marked: "CREATE POLICY users_policy ON users USING (|)",
            contract: Contract::Column,
        },
    ),
    (
        "CreatePolicyCheck",
        CoverageStatus::Covered {
            marked: "CREATE POLICY users_policy ON users WITH CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        "AlterPolicyUsing",
        CoverageStatus::Covered {
            marked: "ALTER POLICY users_policy ON users USING (|)",
            contract: Contract::Column,
        },
    ),
    (
        "AlterPolicyCheck",
        CoverageStatus::Covered {
            marked: "ALTER POLICY users_policy ON users WITH CHECK (|)",
            contract: Contract::Column,
        },
    ),
    (
        "ReturnExpression",
        CoverageStatus::Covered {
            marked: "CREATE FUNCTION f() RETURNS integer BEGIN ATOMIC RETURN |; END",
            contract: Contract::Value,
        },
    ),
    (
        "GraphTableWhere",
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p) WHERE | COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        "GraphPathWhere",
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH ((p) WHERE |) COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        "GraphElementWhere",
        CoverageStatus::Covered {
            marked: "SELECT * FROM GRAPH_TABLE(social MATCH (p WHERE |) COLUMNS (p.id))",
            contract: Contract::Value,
        },
    ),
    (
        "JsonTableContext",
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE(|, '$' COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        "JsonTablePassingArgument",
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' PASSING | AS value COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        "JsonTablePassingArgumentAfterComma",
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' PASSING 1 AS first, | AS second COLUMNS (id integer))",
            contract: Contract::Value,
        },
    ),
    (
        "StatisticsExpression",
        CoverageStatus::Covered {
            marked: "CREATE STATISTICS s ON | FROM users",
            contract: Contract::Value,
        },
    ),
    (
        "StatisticsExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE STATISTICS s ON id, | FROM users",
            contract: Contract::Value,
        },
    ),
    (
        "CreateIndexElement",
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (|)",
            contract: Contract::Column,
        },
    ),
    (
        "CreateIndexElementAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE INDEX users_idx ON users (id, |)",
            contract: Contract::Column,
        },
    ),
    (
        "ExclusionElement",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (| WITH =)",
            contract: Contract::Column,
        },
    ),
    (
        "ExclusionElementAfterComma",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =, | WITH =)",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictInferenceElement",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (|) DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "OnConflictInferenceElementAfterComma",
        CoverageStatus::Covered {
            marked: "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id, |) DO NOTHING",
            contract: Contract::Column,
        },
    ),
    (
        "PartitionKeyExpression",
        CoverageStatus::Covered {
            marked: "CREATE TABLE partitioned (id integer) PARTITION BY RANGE (|)",
            contract: Contract::Value,
        },
    ),
    (
        "PartitionKeyExpressionAfterComma",
        CoverageStatus::Covered {
            marked: "CREATE TABLE partitioned (id integer, name text) PARTITION BY RANGE (id, |)",
            contract: Contract::Value,
        },
    ),
    (
        "JsonTableDefaultBehavior",
        CoverageStatus::Covered {
            marked: "SELECT * FROM JSON_TABLE('{}', '$' COLUMNS (id integer PATH '$.id' DEFAULT | ON EMPTY))",
            contract: Contract::Value,
        },
    ),
    (
        "CallRoutine",
        CoverageStatus::Covered {
            marked: "CALL |",
            contract: Contract::FunctionReference,
        },
    ),
    (
        "InsertColumn",
        CoverageStatus::Covered {
            marked: "INSERT INTO users(|) VALUES ('x')",
            contract: Contract::Column,
        },
    ),
    (
        "SelectContinuation",
        CoverageStatus::Covered {
            marked: "SELECT id |",
            contract: Contract::Keyword("FROM"),
        },
    ),
    (
        "JoinUsingColumn",
        CoverageStatus::Covered {
            marked: "SELECT * FROM users u JOIN orders o USING (|)",
            contract: Contract::JoinUsingColumn,
        },
    ),
    (
        "TypeName",
        CoverageStatus::Covered {
            marked: "SELECT id::| FROM users",
            contract: Contract::Type,
        },
    ),
    (
        "AlterTableColumnName",
        CoverageStatus::Covered {
            marked: "ALTER TABLE users RENAME COLUMN |",
            contract: Contract::Column,
        },
    ),
    (
        "DropRelation",
        CoverageStatus::Covered {
            marked: "DROP TABLE |",
            contract: Contract::Relation,
        },
    ),
    (
        "ObjectColumnName",
        CoverageStatus::Covered {
            marked: "COMMENT ON COLUMN users.| IS 'column comment'",
            contract: Contract::Column,
        },
    ),
];

#[test]
fn every_completion_scenario_is_named_once() {
    let mut classified = HashSet::new();
    for (name, status) in COVERAGE {
        assert!(
            classified.insert(*name),
            "duplicate completion scenario {name:?}"
        );
        if let CoverageStatus::Pending { phase, reason } = status {
            panic!(
                "completion scenario {name:?} is pending ({phase}: {reason}); CI requires pending = 0"
            );
        }
    }
}

#[test]
fn completion_scenarios_reach_their_public_contract() {
    let fixture = Fixture::default();
    for (_name, status) in COVERAGE {
        let CoverageStatus::Covered { marked, contract } = status else {
            continue;
        };
        let completed = fixture.complete(marked);
        completed
            .assert_replacement_contract()
            .assert_no_duplicate_items();
        match contract {
            Contract::Keyword(keyword) => {
                completed.assert_has(keyword, CompletionKind::Keyword);
            }
            Contract::KeywordSet(expected) => {
                completed.assert_kind_label_set(CompletionKind::Keyword, expected);
            }
            Contract::Relation => {
                completed.assert_has("users", CompletionKind::Table);
            }
            Contract::Column => {
                completed.assert_has("name", CompletionKind::Column);
            }
            Contract::JoinUsingColumn => {
                completed.assert_has("id", CompletionKind::Column);
            }
            Contract::Type => {
                completed.assert_has("integer", CompletionKind::Type);
            }
            Contract::Value => {
                completed
                    .assert_has("count", CompletionKind::Function)
                    .assert_has("NULL", CompletionKind::Keyword);
            }
            Contract::Declaration => {
                completed
                    .assert_all_items(|item| !matches!(item.kind, CompletionKind::Catalog(_)))
                    .assert_incomplete(false);
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
fn partial_expressions_preserve_their_outer_semantics() {
    let marked = "SELECT * FROM users WHERE NOT |";
    Fixture::default()
        .complete(marked)
        .assert_has("name", CompletionKind::Column);
}
