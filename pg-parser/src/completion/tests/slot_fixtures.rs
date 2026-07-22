use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SlotContract {
    Keyword,
    Relation,
    Column,
    JoinUsingColumn,
    Type,
    Value,
    Declaration,
    FunctionReference,
    FromItem,
}

pub(super) const SLOT_FIXTURES: &[(CompletionSlot, &str, SlotContract)] = &[
    (CompletionSlot::StatementStart, "|", SlotContract::Keyword),
    (
        CompletionSlot::CreateObjectKind,
        "CREATE |",
        SlotContract::Keyword,
    ),
    (
        CompletionSlot::AlterObjectKind,
        "ALTER |",
        SlotContract::Keyword,
    ),
    (
        CompletionSlot::DropObjectKind,
        "DROP |",
        SlotContract::Keyword,
    ),
    (
        CompletionSlot::FromItem,
        "SELECT * FROM |",
        SlotContract::FromItem,
    ),
    (
        CompletionSlot::SelectTarget,
        "SELECT | FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectTargetAfterComma,
        "SELECT id, | FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectDistinctOn,
        "SELECT DISTINCT ON (|) id FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectDistinctOnAfterComma,
        "SELECT DISTINCT ON (id, |) id FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectWhere,
        "SELECT * FROM users WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectGroupBy,
        "SELECT * FROM users GROUP BY |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectGroupByAfterComma,
        "SELECT * FROM users GROUP BY id, |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectHaving,
        "SELECT * FROM users HAVING |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectOrderBy,
        "SELECT * FROM users ORDER BY |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectOrderByAfterComma,
        "SELECT * FROM users ORDER BY id, |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectLimit,
        "SELECT * FROM users LIMIT |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectOffset,
        "SELECT * FROM users OFFSET |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectFetchCount,
        "SELECT * FROM users FETCH FIRST | ROWS ONLY",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ValuesExpression,
        "VALUES (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::ValuesExpressionAfterComma,
        "VALUES (1, |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::WindowPartitionExpression,
        "SELECT count(id) FROM users WINDOW w AS (PARTITION BY |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WindowPartitionExpressionAfterComma,
        "SELECT count(id) FROM users WINDOW w AS (PARTITION BY id, |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WindowOrderExpression,
        "SELECT count(id) FROM users WINDOW w AS (ORDER BY |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WindowOrderExpressionAfterComma,
        "SELECT count(id) FROM users WINDOW w AS (ORDER BY id, |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WindowFrameStartOffset,
        "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS | PRECEDING)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WindowFrameEndOffset,
        "SELECT count(id) FROM users WINDOW w AS (ORDER BY id ROWS BETWEEN 1 PRECEDING AND | FOLLOWING)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TableSampleArgument,
        "SELECT * FROM users TABLESAMPLE system(|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TableSampleArgumentAfterComma,
        "SELECT * FROM users TABLESAMPLE system(1, |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TableSampleRepeatable,
        "SELECT * FROM users TABLESAMPLE system(1) REPEATABLE (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::RowsFromFunction,
        "SELECT * FROM ROWS FROM (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::RowsFromFunctionAfterComma,
        "SELECT * FROM ROWS FROM (count(1), |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JoinOn,
        "SELECT * FROM users u JOIN orders o ON |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlTableNamespace,
        "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::XmlTableNamespaceAfterComma,
        "SELECT * FROM XMLTABLE(XMLNAMESPACES('/a' AS a, DEFAULT |), '/x' PASSING '<x/>' COLUMNS id integer)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::XmlTableRowExpression,
        "SELECT * FROM XMLTABLE(| PASSING '<x/>' COLUMNS id integer)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::XmlTableDocumentExpression,
        "SELECT * FROM XMLTABLE('/x' PASSING | COLUMNS id integer)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::FunctionArgument,
        "SELECT count(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::FunctionArgumentAfterComma,
        "SELECT calculate_total(1, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::FunctionOrderBy,
        "SELECT count(id ORDER BY |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::FunctionOrderByAfterComma,
        "SELECT count(id ORDER BY name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WithinGroupOrderBy,
        "SELECT calculate_total(1) WITHIN GROUP (ORDER BY |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::WithinGroupOrderByAfterComma,
        "SELECT calculate_total(1) WITHIN GROUP (ORDER BY name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::FunctionFilter,
        "SELECT count(id) FILTER (WHERE |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ArrayElement,
        "SELECT ARRAY[|] FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ArrayElementAfterComma,
        "SELECT ARRAY[id, |] FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ParenthesizedExpression,
        "SELECT (|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ParenthesizedExpressionAfterComma,
        "SELECT (id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CoalesceArgument,
        "SELECT COALESCE(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CoalesceArgumentAfterComma,
        "SELECT COALESCE(id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MinmaxArgument,
        "SELECT GREATEST(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MinmaxArgumentAfterComma,
        "SELECT LEAST(id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::NullifArgument,
        "SELECT NULLIF(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::NullifArgumentAfterComma,
        "SELECT NULLIF(id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::InListExpression,
        "SELECT id IN (|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::InListExpressionAfterComma,
        "SELECT id IN (1, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::GroupingArgument,
        "SELECT GROUPING(|) FROM users GROUP BY id",
        SlotContract::Column,
    ),
    (
        CompletionSlot::GroupingArgumentAfterComma,
        "SELECT GROUPING(id, |) FROM users GROUP BY id",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CaseOperand,
        "SELECT CASE | WHEN 1 THEN 2 END FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CaseWhenCondition,
        "SELECT CASE WHEN | THEN 1 END FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CaseThenResult,
        "SELECT CASE WHEN true THEN | END FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CaseElseResult,
        "SELECT CASE WHEN true THEN 1 ELSE | END FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CastArgument,
        "SELECT CAST(| AS integer) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ExtractArgument,
        "SELECT EXTRACT(YEAR FROM |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::NormalizeArgument,
        "SELECT NORMALIZE(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::PositionNeedle,
        "SELECT POSITION(| IN name) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::PositionHaystack,
        "SELECT POSITION('x' IN |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OverlaySource,
        "SELECT OVERLAY(| PLACING 'x' FROM 1) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OverlayReplacement,
        "SELECT OVERLAY(name PLACING | FROM 1) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OverlayStart,
        "SELECT OVERLAY(name PLACING 'x' FROM |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OverlayCount,
        "SELECT OVERLAY(name PLACING 'x' FROM 1 FOR |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SubstringSource,
        "SELECT SUBSTRING(| FROM 1) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SubstringStart,
        "SELECT SUBSTRING(name FROM |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SubstringCount,
        "SELECT SUBSTRING(name FROM 1 FOR |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SubstringPattern,
        "SELECT SUBSTRING(name SIMILAR | ESCAPE '#') FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SubstringEscape,
        "SELECT SUBSTRING(name SIMILAR '%#\"o#\"%' ESCAPE |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TrimArgument,
        "SELECT TRIM(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TrimArgumentAfterComma,
        "SELECT TRIM(name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TrimSource,
        "SELECT TRIM('x' FROM |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TrimSourceAfterComma,
        "SELECT TRIM('x' FROM name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlExistsXpath,
        "SELECT XMLEXISTS(| PASSING '<x/>') FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlExistsDocument,
        "SELECT XMLEXISTS('/x' PASSING |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::RowElement,
        "SELECT ROW(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::RowElementAfterComma,
        "SELECT ROW(id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlConcatArgument,
        "SELECT XMLCONCAT(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlConcatArgumentAfterComma,
        "SELECT XMLCONCAT(name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlElementContent,
        "SELECT XMLELEMENT(NAME item, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlElementContentAfterComma,
        "SELECT XMLELEMENT(NAME item, name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlAttributeExpression,
        "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(|)) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlAttributeExpressionAfterComma,
        "SELECT XMLELEMENT(NAME item, XMLATTRIBUTES(id, |)) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlForestExpression,
        "SELECT XMLFOREST(|) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlForestExpressionAfterComma,
        "SELECT XMLFOREST(id, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlParseValue,
        "SELECT XMLPARSE(DOCUMENT |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlPiValue,
        "SELECT XMLPI(NAME item, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlRootDocument,
        "SELECT XMLROOT(|, VERSION '1.0') FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlRootVersion,
        "SELECT XMLROOT(name, VERSION |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::XmlSerializeValue,
        "SELECT XMLSERIALIZE(CONTENT | AS text) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ExecuteParameter,
        "EXECUTE prepared_statement(|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::ExecuteParameterAfterComma,
        "EXECUTE prepared_statement(1, |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionListValue,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionListValueAfterComma,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES IN (1, |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionRangeFromValue,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (|) TO (10)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionRangeFromValueAfterComma,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, |) TO (10, 20)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionRangeToValue,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1) TO (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionRangeToValueAfterComma,
        "CREATE TABLE events_2026 PARTITION OF users FOR VALUES FROM (1, 2) TO (10, |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::MergeInsertValue,
        "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeInsertValueAfterComma,
        "MERGE INTO users u USING orders o ON true WHEN NOT MATCHED THEN INSERT VALUES (u.id, |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ReturningExpression,
        "UPDATE users SET name = 'x' RETURNING |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ReturningExpressionAfterComma,
        "UPDATE users SET name = 'x' RETURNING id, |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::GraphTableColumnExpression,
        "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (|))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::GraphTableColumnExpressionAfterComma,
        "SELECT * FROM GRAPH_TABLE(social MATCH (p) COLUMNS (p.id, |))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PropertyGraphPropertyExpression,
        "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (|))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PropertyGraphPropertyExpressionAfterComma,
        "CREATE PROPERTY GRAPH g VERTEX TABLES (users PROPERTIES (id, |))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JsonArrayAggOrderBy,
        "SELECT JSON_ARRAYAGG(id ORDER BY |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::JsonArrayAggOrderByAfterComma,
        "SELECT JSON_ARRAYAGG(id ORDER BY name, |) FROM users",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CteName,
        "WITH | AS (SELECT 1) SELECT 1",
        SlotContract::Declaration,
    ),
    (
        CompletionSlot::CteAliasColumn,
        "WITH recent(|) AS (SELECT 1) SELECT 1",
        SlotContract::Declaration,
    ),
    (
        CompletionSlot::CteAliasColumnAfterComma,
        "WITH recent(id, |) AS (SELECT 1, 2) SELECT 1",
        SlotContract::Declaration,
    ),
    (
        CompletionSlot::CteContinuation,
        "WITH recent AS (SELECT 1) |",
        SlotContract::Keyword,
    ),
    (
        CompletionSlot::InsertTargetRelation,
        "INSERT INTO |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::UpdateTargetRelation,
        "UPDATE |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::DeleteTargetRelation,
        "DELETE FROM |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::IndexRelation,
        "CREATE INDEX users_idx ON |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::AlterTableRelation,
        "ALTER TABLE |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::UpdateSetTarget,
        "UPDATE users SET |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::UpdateSetTargetAfterComma,
        "UPDATE users SET name = 'x', |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::UpdateSetValue,
        "UPDATE users SET name = |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::UpdateWhere,
        "UPDATE users SET name = 'x' WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::DeleteWhere,
        "DELETE FROM users WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ForPortionTarget,
        "DELETE FROM users FOR PORTION OF valid_time (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ForPortionStart,
        "UPDATE users FOR PORTION OF valid_time FROM | TO 10 SET name = 'x'",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ForPortionEnd,
        "UPDATE users FOR PORTION OF valid_time FROM 1 TO | SET name = 'x'",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictInferenceWhere,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id) WHERE | DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictSetTarget,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictSetTargetAfterComma,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x', |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictSetValue,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictUpdateWhere,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO UPDATE SET name = 'x' WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictSelectWhere,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT DO SELECT WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeJoinCondition,
        "MERGE INTO users u USING orders o ON | WHEN MATCHED THEN DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeWhenCondition,
        "MERGE INTO users u USING orders o ON true WHEN MATCHED AND | THEN DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeSetTarget,
        "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeSetTargetAfterComma,
        "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = 'x', |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::MergeSetValue,
        "MERGE INTO users u USING orders o ON true WHEN MATCHED THEN UPDATE SET name = |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AssignmentSubscriptLowerOrIndex,
        "UPDATE users SET name[|] = 'x'",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AssignmentSliceUpper,
        "UPDATE users SET name[1:|] = 'x'",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AlterColumnUsing,
        "ALTER TABLE users ALTER COLUMN name TYPE text USING |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AlterColumnDefault,
        "ALTER TABLE users ALTER COLUMN name SET DEFAULT |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AlterColumnExpression,
        "ALTER TABLE users ALTER COLUMN name SET EXPRESSION AS (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::PublicationRowFilter,
        "CREATE PUBLICATION p FOR TABLE users WHERE (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::RuleWhere,
        "CREATE RULE r AS ON UPDATE TO users WHERE | DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ColumnDefault,
        "ALTER TABLE users ADD COLUMN value integer DEFAULT |",
        SlotContract::Value,
    ),
    (
        CompletionSlot::ColumnCheck,
        "ALTER TABLE users ADD COLUMN value integer CHECK (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ColumnGenerated,
        "ALTER TABLE users ADD COLUMN value integer GENERATED ALWAYS AS (|) STORED",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TableCheck,
        "ALTER TABLE users ADD CHECK (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ExclusionWhere,
        "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =) WHERE (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::TriggerWhen,
        "CREATE TRIGGER users_trigger BEFORE UPDATE ON users FOR EACH ROW WHEN (|) EXECUTE FUNCTION calculate_total()",
        SlotContract::Column,
    ),
    (
        CompletionSlot::IndexPredicate,
        "CREATE INDEX users_idx ON users (id) WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::DomainDefault,
        "CREATE DOMAIN positive_integer AS integer DEFAULT |",
        SlotContract::Value,
    ),
    (
        CompletionSlot::DomainCheck,
        "CREATE DOMAIN positive_integer AS integer CHECK (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::AlterDomainDefault,
        "ALTER DOMAIN positive_integer SET DEFAULT |",
        SlotContract::Value,
    ),
    (
        CompletionSlot::AlterDomainCheck,
        "ALTER DOMAIN positive_integer ADD CHECK (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::CopyWhere,
        "COPY users FROM STDIN WHERE |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CreatePolicyUsing,
        "CREATE POLICY users_policy ON users USING (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CreatePolicyCheck,
        "CREATE POLICY users_policy ON users WITH CHECK (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AlterPolicyUsing,
        "ALTER POLICY users_policy ON users USING (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::AlterPolicyCheck,
        "ALTER POLICY users_policy ON users WITH CHECK (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ReturnExpression,
        "CREATE FUNCTION f() RETURNS integer BEGIN ATOMIC RETURN |; END",
        SlotContract::Value,
    ),
    (
        CompletionSlot::GraphTableWhere,
        "SELECT * FROM GRAPH_TABLE(social MATCH (p) WHERE | COLUMNS (p.id))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::GraphPathWhere,
        "SELECT * FROM GRAPH_TABLE(social MATCH ((p) WHERE |) COLUMNS (p.id))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::GraphElementWhere,
        "SELECT * FROM GRAPH_TABLE(social MATCH (p WHERE |) COLUMNS (p.id))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JsonTableContext,
        "SELECT * FROM JSON_TABLE(|, '$' COLUMNS (id integer))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JsonTablePassingArgument,
        "SELECT * FROM JSON_TABLE('{}', '$' PASSING | AS value COLUMNS (id integer))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JsonTablePassingArgumentAfterComma,
        "SELECT * FROM JSON_TABLE('{}', '$' PASSING 1 AS first, | AS second COLUMNS (id integer))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::StatisticsExpression,
        "CREATE STATISTICS s ON | FROM users",
        SlotContract::Value,
    ),
    (
        CompletionSlot::StatisticsExpressionAfterComma,
        "CREATE STATISTICS s ON id, | FROM users",
        SlotContract::Value,
    ),
    (
        CompletionSlot::CreateIndexElement,
        "CREATE INDEX users_idx ON users (|)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::CreateIndexElementAfterComma,
        "CREATE INDEX users_idx ON users (id, |)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ExclusionElement,
        "ALTER TABLE users ADD EXCLUDE USING gist (| WITH =)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::ExclusionElementAfterComma,
        "ALTER TABLE users ADD EXCLUDE USING gist (id WITH =, | WITH =)",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictInferenceElement,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT (|) DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::OnConflictInferenceElementAfterComma,
        "INSERT INTO users VALUES (1, 'x') ON CONFLICT (id, |) DO NOTHING",
        SlotContract::Column,
    ),
    (
        CompletionSlot::PartitionKeyExpression,
        "CREATE TABLE partitioned (id integer) PARTITION BY RANGE (|)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::PartitionKeyExpressionAfterComma,
        "CREATE TABLE partitioned (id integer, name text) PARTITION BY RANGE (id, |)",
        SlotContract::Value,
    ),
    (
        CompletionSlot::JsonTableDefaultBehavior,
        "SELECT * FROM JSON_TABLE('{}', '$' COLUMNS (id integer PATH '$.id' DEFAULT | ON EMPTY))",
        SlotContract::Value,
    ),
    (
        CompletionSlot::CallRoutine,
        "CALL |",
        SlotContract::FunctionReference,
    ),
    (
        CompletionSlot::InsertColumn,
        "INSERT INTO users(|) VALUES ('x')",
        SlotContract::Column,
    ),
    (
        CompletionSlot::SelectContinuation,
        "SELECT id |",
        SlotContract::Keyword,
    ),
    (
        CompletionSlot::JoinUsingColumn,
        "SELECT * FROM users u JOIN orders o USING (|)",
        SlotContract::JoinUsingColumn,
    ),
    (
        CompletionSlot::TypeName,
        "SELECT id::| FROM users",
        SlotContract::Type,
    ),
    (
        CompletionSlot::AlterTableColumnName,
        "ALTER TABLE users RENAME COLUMN |",
        SlotContract::Column,
    ),
    (
        CompletionSlot::DropRelation,
        "DROP TABLE |",
        SlotContract::Relation,
    ),
    (
        CompletionSlot::ObjectColumnName,
        "COMMENT ON COLUMN users.| IS 'column comment'",
        SlotContract::Column,
    ),
];
