//! Canonical SQL serialization for PostgreSQL raw parse trees.
//!
//! The serializer deliberately ignores source locations. For supported raw
//! syntax nodes, its output can be parsed again by this crate.

use std::error::Error;
use std::fmt;

use crate::ast::*;

/// Serializes a sequence returned by [`crate::parse`] as semicolon-separated SQL.
pub fn deparse(statements: &[RawStmt]) -> Result<std::string::String, DeparseError> {
    let mut output = Vec::with_capacity(statements.len());
    for statement in statements {
        output.push(deparse_statement(statement)?);
    }
    Ok(output.join("; "))
}

/// Serializes one statement returned by [`crate::parse_one`].
pub fn deparse_statement(statement: &RawStmt) -> Result<std::string::String, DeparseError> {
    let node = statement
        .stmt
        .as_deref()
        .ok_or_else(|| DeparseError::missing("RawStmt", "stmt"))?;
    deparse_node(node)
}

/// Serializes one raw syntax node.
///
/// This entry point is useful for statement and expression fragments. Nodes
/// belonging only to PostgreSQL's analysis tree return [`DeparseError`].
pub fn deparse_node(node: &Node) -> Result<std::string::String, DeparseError> {
    Deparser.render(node)
}

/// A raw tree cannot be represented as SQL because it is incomplete, invalid,
/// or belongs to the analysis tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeparseError {
    /// A required raw-tree field is absent.
    MissingField {
        node: &'static str,
        field: &'static str,
    },
    /// Field values do not form a valid raw syntax node.
    InvalidNode {
        node: &'static str,
        detail: &'static str,
    },
    /// The node belongs to the analysis tree or has no serializer yet.
    UnsupportedNode { node: &'static str },
}

impl DeparseError {
    fn missing(node: &'static str, field: &'static str) -> Self {
        Self::MissingField { node, field }
    }

    fn invalid(node: &'static str, detail: &'static str) -> Self {
        Self::InvalidNode { node, detail }
    }

    fn unsupported(node: &'static str) -> Self {
        Self::UnsupportedNode { node }
    }
}

impl fmt::Display for DeparseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeparseError::MissingField { node, field } => {
                write!(
                    formatter,
                    "cannot deparse {node}: required field `{field}` is missing"
                )
            }
            DeparseError::InvalidNode { node, detail } => {
                write!(formatter, "cannot deparse {node}: {detail}")
            }
            DeparseError::UnsupportedNode { node } => {
                write!(formatter, "cannot deparse unsupported node {node}")
            }
        }
    }
}

impl Error for DeparseError {}

struct Deparser;

impl Deparser {
    fn render(&self, node: &Node) -> Result<std::string::String, DeparseError> {
        match node {
            Node::RawStmt(statement) => {
                self.required_node(statement.stmt.as_deref(), "RawStmt", "stmt")
            }
            Node::SelectStmt(statement) => self.select(statement),
            Node::InsertStmt(statement) => self.insert(statement),
            Node::UpdateStmt(statement) => self.update(statement),
            Node::DeleteStmt(statement) => self.delete(statement),
            Node::MergeStmt(statement) => self.merge_statement(statement),
            Node::CreateStmt(statement) => self.create_table(statement),
            Node::CreateSchemaStmt(statement) => self.create_schema(statement),
            Node::IndexStmt(statement) => self.create_index(statement),
            Node::DropStmt(statement) => self.drop_statement(statement),
            Node::TruncateStmt(statement) => self.truncate(statement),
            Node::ViewStmt(statement) => self.create_view(statement),
            Node::CreateTableAsStmt(statement) => self.create_table_as(statement),
            Node::CreateDomainStmt(statement) => self.create_domain(statement),
            Node::CreateEnumStmt(statement) => self.create_enum(statement),
            Node::CreateRangeStmt(statement) => self.create_range(statement),
            Node::CreateFunctionStmt(statement) => self.create_function(statement),
            Node::ReturnStmt(statement) => Ok(format!(
                "RETURN {}",
                self.required_node(statement.returnval.as_deref(), "ReturnStmt", "returnval")?
            )),
            Node::CallStmt(statement) => self.call_statement(statement),
            Node::VariableSetStmt(statement) => self.variable_set(statement),
            Node::VariableShowStmt(statement) => self.variable_show(statement),
            Node::PrepareStmt(statement) => self.prepare(statement),
            Node::ExecuteStmt(statement) => self.execute(statement),
            Node::DeallocateStmt(statement) => self.deallocate(statement),
            Node::NotifyStmt(statement) => self.notify(statement),
            Node::ListenStmt(statement) => self.listen(statement),
            Node::UnlistenStmt(statement) => self.unlisten(statement),
            Node::TransactionStmt(statement) => self.transaction(statement),
            Node::ExplainStmt(statement) => self.explain(statement),
            Node::RefreshMatViewStmt(statement) => self.refresh_materialized_view(statement),
            Node::CommentStmt(statement) => self.comment(statement),
            Node::SecLabelStmt(statement) => self.security_label(statement),

            Node::RangeVar(range) => self.range_var(range),
            Node::RangeSubselect(range) => self.range_subselect(range),
            Node::RangeFunction(range) => self.range_function(range),
            Node::RangeTableSample(range) => self.range_table_sample(range),
            Node::JoinExpr(join) => self.join(join),
            Node::Alias(alias) => self.alias(alias),

            Node::AConst(value) => Ok(render_constant(value)),
            Node::ColumnRef(reference) => self.qualified_nodes(&reference.fields),
            Node::ParamRef(reference) => Ok(format!("${}", reference.number)),
            Node::AExpr(expression) => self.a_expr(expression),
            Node::BoolExpr(expression) => self.bool_expr(expression),
            Node::TypeCast(cast) => self.type_cast(cast),
            Node::CollateClause(clause) => self.collate_clause(clause),
            Node::FuncCall(call) => self.func_call(call),
            Node::NamedArgExpr(argument) => self.named_argument(argument),
            Node::AIndirection(indirection) => self.a_indirection(indirection),
            Node::AIndices(indices) => self.indices(indices),
            Node::AArrayExpr(array) => self.array_expr(array),
            Node::RowExpr(row) => Ok(format!("ROW({})", self.list(&row.args, ", ")?)),
            Node::CoalesceExpr(expression) => {
                Ok(format!("COALESCE({})", self.list(&expression.args, ", ")?))
            }
            Node::MinMaxExpr(expression) => self.min_max(expression),
            Node::SqlValueFunction(function) => self.sql_value_function(function),
            Node::NullTest(test) => self.null_test(test),
            Node::BooleanTest(test) => self.boolean_test(test),
            Node::SubLink(link) => self.sub_link(link),
            Node::CaseExpr(expression) => self.case_expr(expression),
            Node::CaseWhen(when) => self.case_when(when),
            Node::SetToDefault(_) => Ok("DEFAULT".to_owned()),
            Node::CurrentOfExpr(current) => self.current_of(current),
            Node::GroupingFunc(function) => {
                Ok(format!("GROUPING({})", self.list(&function.args, ", ")?))
            }
            Node::MergeSupportFunc(function) => self.merge_support_function(function),
            Node::XmlExpr(expression) => self.xml_expr(expression),
            Node::XmlSerialize(expression) => self.xml_serialize(expression),

            Node::ResTarget(target) => self.result_target(target),
            Node::SortBy(sort) => self.sort_by(sort),
            Node::WindowDef(window) => self.window_def(window),
            Node::LockingClause(locking) => self.locking_clause(locking),
            Node::GroupingSet(set) => self.grouping_set(set),
            Node::WithClause(clause) => self.with_clause(clause),
            Node::CommonTableExpr(cte) => self.common_table_expression(cte),
            Node::ReturningClause(clause) => self.returning_clause(clause),
            Node::ReturningOption(option) => self.returning_option(option),
            Node::InferClause(clause) => self.infer_clause(clause),
            Node::OnConflictClause(clause) => self.on_conflict_clause(clause),
            Node::MergeWhenClause(clause) => self.merge_when_clause(clause),
            Node::IndexElem(element) => self.index_element(element),
            Node::ColumnDef(column) => self.column_definition(column),
            Node::Constraint(constraint) => self.constraint(constraint),
            Node::DefElem(element) => self.definition_element(element),
            Node::FunctionParameter(parameter) => self.function_parameter(parameter),
            Node::ObjectWithArgs(object) => self.object_with_args(object),
            Node::TypeName(name) => self.type_name(name),
            Node::RoleSpec(role) => self.role(role),

            Node::Integer(value) => Ok(value.ival.to_string()),
            Node::Float(value) => Ok(value.fval.clone().unwrap_or_else(|| "0".to_owned())),
            Node::Boolean(value) => Ok(if value.boolval { "TRUE" } else { "FALSE" }.to_owned()),
            Node::String(value) => value
                .sval
                .as_deref()
                .map(quote_identifier)
                .ok_or_else(|| DeparseError::missing("String", "sval")),
            Node::BitString(value) => value
                .bsval
                .as_deref()
                .map(render_bit_string)
                .ok_or_else(|| DeparseError::missing("BitString", "bsval")),
            Node::AStar => Ok("*".to_owned()),

            Node::Query(_) => Err(DeparseError::unsupported("Query (analysis tree)")),
            Node::Var(_) => Err(DeparseError::unsupported("Var (analysis tree)")),
            Node::Const(_) => Err(DeparseError::unsupported("Const (analysis tree)")),
            _ => Err(DeparseError::unsupported("raw syntax node")),
        }
    }

    fn required_node(
        &self,
        node: Option<&Node>,
        owner: &'static str,
        field: &'static str,
    ) -> Result<std::string::String, DeparseError> {
        self.render(node.ok_or_else(|| DeparseError::missing(owner, field))?)
    }

    fn list(&self, nodes: &[Node], separator: &str) -> Result<std::string::String, DeparseError> {
        nodes
            .iter()
            .map(|node| self.render(node))
            .collect::<Result<Vec<_>, _>>()
            .map(|items| items.join(separator))
    }

    fn qualified_nodes(&self, nodes: &[Node]) -> Result<std::string::String, DeparseError> {
        nodes
            .iter()
            .map(|node| match node {
                Node::String(value) => value
                    .sval
                    .as_deref()
                    .map(quote_identifier)
                    .ok_or_else(|| DeparseError::missing("String", "sval")),
                Node::AStar => Ok("*".to_owned()),
                _ => Err(DeparseError::invalid(
                    "qualified name",
                    "name parts must be String or AStar nodes",
                )),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join("."))
    }

    fn select(&self, statement: &SelectStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = std::string::String::new();
        if let Some(with) = statement.with_clause.as_deref() {
            sql.push_str(&self.with_clause(with)?);
            sql.push(' ');
        }

        if statement.op != SetOperation::None {
            let left = statement
                .larg
                .as_deref()
                .ok_or_else(|| DeparseError::missing("SelectStmt", "larg"))?;
            let right = statement
                .rarg
                .as_deref()
                .ok_or_else(|| DeparseError::missing("SelectStmt", "rarg"))?;
            sql.push('(');
            sql.push_str(&self.select(left)?);
            sql.push_str(") ");
            sql.push_str(match statement.op {
                SetOperation::Union => "UNION",
                SetOperation::Intersect => "INTERSECT",
                SetOperation::Except => "EXCEPT",
                SetOperation::None => unreachable!(),
            });
            if statement.all {
                sql.push_str(" ALL");
            }
            sql.push_str(" (");
            sql.push_str(&self.select(right)?);
            sql.push(')');
        } else if !statement.values_lists.is_empty() {
            sql.push_str("VALUES ");
            let rows = statement
                .values_lists
                .iter()
                .map(|row| match row {
                    Node::AArrayExpr(values) => {
                        Ok(format!("({})", self.list(&values.elements, ", ")?))
                    }
                    _ => Err(DeparseError::invalid(
                        "SelectStmt",
                        "values_lists entries must be AArrayExpr nodes",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            sql.push_str(&rows.join(", "));
        } else {
            sql.push_str("SELECT");
            if !statement.distinct_clause.is_empty() {
                if is_distinct_marker(&statement.distinct_clause) {
                    sql.push_str(" DISTINCT");
                } else {
                    sql.push_str(" DISTINCT ON (");
                    sql.push_str(&self.list(&statement.distinct_clause, ", ")?);
                    sql.push(')');
                }
            }
            if !statement.target_list.is_empty() {
                sql.push(' ');
                sql.push_str(&self.list(&statement.target_list, ", ")?);
            }
            if let Some(into) = statement.into_clause.as_deref() {
                sql.push(' ');
                sql.push_str(&self.render_into_clause(into)?);
            }
            if !statement.from_clause.is_empty() {
                sql.push_str(" FROM ");
                sql.push_str(&self.list(&statement.from_clause, ", ")?);
            }
            if let Some(condition) = statement.where_clause.as_deref() {
                sql.push_str(" WHERE ");
                sql.push_str(&self.render(condition)?);
            }
            if !statement.group_clause.is_empty() {
                sql.push_str(" GROUP BY");
                if statement.group_distinct {
                    sql.push_str(" DISTINCT");
                } else if statement.group_by_all {
                    sql.push_str(" ALL");
                }
                sql.push(' ');
                sql.push_str(&self.list(&statement.group_clause, ", ")?);
            }
            if let Some(condition) = statement.having_clause.as_deref() {
                sql.push_str(" HAVING ");
                sql.push_str(&self.render(condition)?);
            }
            if !statement.window_clause.is_empty() {
                sql.push_str(" WINDOW ");
                sql.push_str(&self.list(&statement.window_clause, ", ")?);
            }
        }

        if !statement.sort_clause.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.list(&statement.sort_clause, ", ")?);
        }
        if let Some(offset) = statement.limit_offset.as_deref() {
            sql.push_str(" OFFSET ");
            sql.push_str(&self.render(offset)?);
        }
        if let Some(limit) = statement.limit_count.as_deref() {
            if statement.limit_option == LimitOption::WithTies {
                sql.push_str(" FETCH FIRST ");
                sql.push_str(&self.render(limit)?);
                sql.push_str(" ROWS WITH TIES");
            } else {
                sql.push_str(" LIMIT ");
                if is_null_constant(limit) {
                    sql.push_str("ALL");
                } else {
                    sql.push_str(&self.render(limit)?);
                }
            }
        }
        for locking in &statement.locking_clause {
            sql.push(' ');
            sql.push_str(&self.render(locking)?);
        }
        Ok(sql)
    }

    fn insert(&self, statement: &InsertStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = self.optional_with(statement.with_clause.as_deref())?;
        sql.push_str("INSERT INTO ");
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("InsertStmt", "relation"))?,
            )?,
        );
        if !statement.cols.is_empty() {
            let cols = statement
                .cols
                .iter()
                .map(|node| match node {
                    Node::ResTarget(target) => self.assignment_target(target),
                    _ => Err(DeparseError::invalid(
                        "InsertStmt",
                        "cols entries must be ResTarget nodes",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            sql.push_str(" (");
            sql.push_str(&cols.join(", "));
            sql.push(')');
        }
        sql.push_str(match statement.override_ {
            OverridingKind::NotSet => "",
            OverridingKind::UserValue => " OVERRIDING USER VALUE",
            OverridingKind::SystemValue => " OVERRIDING SYSTEM VALUE",
        });
        if let Some(source) = statement.select_stmt.as_deref() {
            sql.push(' ');
            sql.push_str(&self.render(source)?);
        } else {
            sql.push_str(" DEFAULT VALUES");
        }
        if let Some(conflict) = statement.on_conflict_clause.as_deref() {
            sql.push(' ');
            sql.push_str(&self.on_conflict_clause(conflict)?);
        }
        if let Some(returning) = statement.returning_clause.as_deref() {
            sql.push(' ');
            sql.push_str(&self.returning_clause(returning)?);
        }
        Ok(sql)
    }

    fn update(&self, statement: &UpdateStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = self.optional_with(statement.with_clause.as_deref())?;
        sql.push_str("UPDATE ");
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("UpdateStmt", "relation"))?,
            )?,
        );
        if let Some(portion) = statement.for_portion_of.as_deref() {
            sql.push(' ');
            sql.push_str(&self.for_portion_of(portion)?);
        }
        sql.push_str(" SET ");
        sql.push_str(&self.update_targets(&statement.target_list)?);
        if !statement.from_clause.is_empty() {
            sql.push_str(" FROM ");
            sql.push_str(&self.list(&statement.from_clause, ", ")?);
        }
        if let Some(condition) = statement.where_clause.as_deref() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.render(condition)?);
        }
        if let Some(returning) = statement.returning_clause.as_deref() {
            sql.push(' ');
            sql.push_str(&self.returning_clause(returning)?);
        }
        Ok(sql)
    }

    fn delete(&self, statement: &DeleteStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = self.optional_with(statement.with_clause.as_deref())?;
        sql.push_str("DELETE FROM ");
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("DeleteStmt", "relation"))?,
            )?,
        );
        if let Some(portion) = statement.for_portion_of.as_deref() {
            sql.push(' ');
            sql.push_str(&self.for_portion_of(portion)?);
        }
        if !statement.using_clause.is_empty() {
            sql.push_str(" USING ");
            sql.push_str(&self.list(&statement.using_clause, ", ")?);
        }
        if let Some(condition) = statement.where_clause.as_deref() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.render(condition)?);
        }
        if let Some(returning) = statement.returning_clause.as_deref() {
            sql.push(' ');
            sql.push_str(&self.returning_clause(returning)?);
        }
        Ok(sql)
    }

    fn merge_statement(&self, statement: &MergeStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = self.optional_with(statement.with_clause.as_deref())?;
        sql.push_str("MERGE INTO ");
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("MergeStmt", "relation"))?,
            )?,
        );
        sql.push_str(" USING ");
        sql.push_str(&self.required_node(
            statement.source_relation.as_deref(),
            "MergeStmt",
            "source_relation",
        )?);
        sql.push_str(" ON ");
        sql.push_str(&self.required_node(
            statement.join_condition.as_deref(),
            "MergeStmt",
            "join_condition",
        )?);
        for clause in &statement.merge_when_clauses {
            sql.push(' ');
            sql.push_str(&self.render(clause)?);
        }
        if let Some(returning) = statement.returning_clause.as_deref() {
            sql.push(' ');
            sql.push_str(&self.returning_clause(returning)?);
        }
        Ok(sql)
    }

    fn optional_with(
        &self,
        clause: Option<&WithClause>,
    ) -> Result<std::string::String, DeparseError> {
        match clause {
            Some(clause) => Ok(format!("{} ", self.with_clause(clause)?)),
            None => Ok(std::string::String::new()),
        }
    }

    fn range_var(&self, range: &RangeVar) -> Result<std::string::String, DeparseError> {
        let relname = range
            .relname
            .as_deref()
            .ok_or_else(|| DeparseError::missing("RangeVar", "relname"))?;
        let mut parts = Vec::new();
        if let Some(catalog) = range.catalogname.as_deref() {
            parts.push(quote_identifier(catalog));
        }
        if let Some(schema) = range.schemaname.as_deref() {
            parts.push(quote_identifier(schema));
        }
        parts.push(quote_identifier(relname));
        let name = parts.join(".");
        let mut sql = if range.inh {
            name
        } else {
            format!("ONLY {name}")
        };
        if let Some(alias) = range.alias.as_deref() {
            sql.push_str(" AS ");
            sql.push_str(&self.alias(alias)?);
        }
        Ok(sql)
    }

    fn alias(&self, alias: &Alias) -> Result<std::string::String, DeparseError> {
        let mut sql = quote_identifier(
            alias
                .aliasname
                .as_deref()
                .ok_or_else(|| DeparseError::missing("Alias", "aliasname"))?,
        );
        if !alias.colnames.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.qualified_name_list(&alias.colnames, ", ")?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn range_subselect(&self, range: &RangeSubselect) -> Result<std::string::String, DeparseError> {
        let mut sql = if range.lateral {
            "LATERAL (".to_owned()
        } else {
            "(".to_owned()
        };
        sql.push_str(&self.required_node(
            range.subquery.as_deref(),
            "RangeSubselect",
            "subquery",
        )?);
        sql.push(')');
        if let Some(alias) = range.alias.as_deref() {
            sql.push_str(" AS ");
            sql.push_str(&self.alias(alias)?);
        }
        Ok(sql)
    }

    fn range_function(&self, range: &RangeFunction) -> Result<std::string::String, DeparseError> {
        let mut sql = if range.lateral { "LATERAL " } else { "" }.to_owned();
        let functions = range
            .functions
            .iter()
            .map(|item| self.range_function_item(item))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        if range.is_rowsfrom {
            sql.push_str("ROWS FROM (");
            sql.push_str(&functions);
            sql.push(')');
        } else {
            sql.push_str(&functions);
        }
        if range.ordinality {
            sql.push_str(" WITH ORDINALITY");
        }
        if let Some(alias) = range.alias.as_deref() {
            sql.push_str(" AS ");
            sql.push_str(&self.alias(alias)?);
        }
        if !range.coldeflist.is_empty() {
            if range.alias.is_none() {
                sql.push_str(" AS");
            }
            sql.push_str(" (");
            sql.push_str(&self.list(&range.coldeflist, ", ")?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn range_function_item(&self, item: &Node) -> Result<std::string::String, DeparseError> {
        // `RangeFunction.functions` entries wrap a function expression and its
        // per-item column definition list as `[expression, coldeflist]`.
        let Node::AArrayExpr(wrapper) = item else {
            return self.render(item);
        };
        let mut elements = wrapper.elements.iter();
        let function = elements
            .next()
            .ok_or_else(|| DeparseError::missing("RangeFunction", "functions"))?;
        let mut sql = self.render(function)?;
        if let Some(coldeflist) = elements.next() {
            let Node::AArrayExpr(definitions) = coldeflist else {
                return Err(DeparseError::invalid(
                    "RangeFunction",
                    "function item column definitions must be a list",
                ));
            };
            if !definitions.elements.is_empty() {
                sql.push_str(" AS (");
                sql.push_str(&self.list(&definitions.elements, ", ")?);
                sql.push(')');
            }
        }
        Ok(sql)
    }

    fn range_table_sample(
        &self,
        range: &RangeTableSample,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql =
            self.required_node(range.relation.as_deref(), "RangeTableSample", "relation")?;
        sql.push_str(" TABLESAMPLE ");
        sql.push_str(&self.qualified_nodes(&range.method)?);
        sql.push('(');
        sql.push_str(&self.list(&range.args, ", ")?);
        sql.push(')');
        if let Some(repeatable) = range.repeatable.as_deref() {
            sql.push_str(" REPEATABLE (");
            sql.push_str(&self.render(repeatable)?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn join(&self, join: &JoinExpr) -> Result<std::string::String, DeparseError> {
        let mut sql = self.required_node(join.larg.as_deref(), "JoinExpr", "larg")?;
        sql.push(' ');
        if join.is_natural {
            sql.push_str("NATURAL ");
        }
        let no_join_condition = join.quals.is_none() && join.using_clause.is_empty();
        sql.push_str(match join.jointype {
            JoinType::Inner if !join.is_natural && no_join_condition => "CROSS JOIN",
            JoinType::Inner => "JOIN",
            JoinType::Left => "LEFT JOIN",
            JoinType::Full => "FULL JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Semi
            | JoinType::Anti
            | JoinType::RightSemi
            | JoinType::RightAnti
            | JoinType::UniqueOuter
            | JoinType::UniqueInner => {
                return Err(DeparseError::unsupported("analysis-tree JoinExpr"));
            }
        });
        sql.push(' ');
        let rarg = join
            .rarg
            .as_deref()
            .ok_or_else(|| DeparseError::missing("JoinExpr", "rarg"))?;
        if matches!(rarg, Node::JoinExpr(_)) {
            sql.push('(');
            sql.push_str(&self.render(rarg)?);
            sql.push(')');
        } else {
            sql.push_str(&self.render(rarg)?);
        }
        if !join.using_clause.is_empty() {
            sql.push_str(" USING (");
            sql.push_str(&self.qualified_name_list(&join.using_clause, ", ")?);
            sql.push(')');
            if let Some(alias) = join.join_using_alias.as_deref() {
                sql.push_str(" AS ");
                sql.push_str(&self.alias(alias)?);
            }
        } else if let Some(condition) = join.quals.as_deref() {
            sql.push_str(" ON ");
            sql.push_str(&self.render(condition)?);
        }
        if let Some(alias) = join.alias.as_deref() {
            sql = format!("({sql}) AS {}", self.alias(alias)?);
        }
        Ok(sql)
    }

    fn a_expr(&self, expression: &AExpr) -> Result<std::string::String, DeparseError> {
        let left = expression.lexpr.as_deref();
        let right = expression.rexpr.as_deref();
        let operator = operator_name(&expression.name)?;
        let sql = match expression.kind {
            AExprKind::Op => match (left, right) {
                (Some(left), Some(right)) => {
                    format!(
                        "({} {operator} {})",
                        self.render(left)?,
                        self.render(right)?
                    )
                }
                (None, Some(right)) => format!("({operator} {})", self.render(right)?),
                (Some(left), None) => format!("({} {operator})", self.render(left)?),
                (None, None) => return Err(DeparseError::missing("AExpr", "lexpr or rexpr")),
            },
            AExprKind::OpAny | AExprKind::OpAll => format!(
                "({} {operator} {} ({}))",
                self.required_node(left, "AExpr", "lexpr")?,
                if expression.kind == AExprKind::OpAny {
                    "ANY"
                } else {
                    "ALL"
                },
                self.required_node(right, "AExpr", "rexpr")?
            ),
            AExprKind::Distinct | AExprKind::NotDistinct => format!(
                "({} IS {}DISTINCT FROM {})",
                self.required_node(left, "AExpr", "lexpr")?,
                if expression.kind == AExprKind::NotDistinct {
                    "NOT "
                } else {
                    ""
                },
                self.required_node(right, "AExpr", "rexpr")?
            ),
            AExprKind::Nullif => format!(
                "NULLIF({}, {})",
                self.required_node(left, "AExpr", "lexpr")?,
                self.required_node(right, "AExpr", "rexpr")?
            ),
            AExprKind::In => format!(
                "({} {}IN {})",
                self.required_node(left, "AExpr", "lexpr")?,
                if operator == "<>" { "NOT " } else { "" },
                self.parenthesized_value(right, "AExpr", "rexpr")?
            ),
            AExprKind::Like | AExprKind::Ilike | AExprKind::Similar => format!(
                "({} {} {})",
                self.required_node(left, "AExpr", "lexpr")?,
                operator,
                self.required_node(right, "AExpr", "rexpr")?
            ),
            AExprKind::Between
            | AExprKind::NotBetween
            | AExprKind::BetweenSym
            | AExprKind::NotBetweenSym => {
                let bounds = match right {
                    Some(Node::AArrayExpr(array)) if array.elements.len() == 2 => &array.elements,
                    _ => {
                        return Err(DeparseError::invalid(
                            "AExpr",
                            "BETWEEN requires two bounds",
                        ));
                    }
                };
                let keyword = match expression.kind {
                    AExprKind::Between => "BETWEEN",
                    AExprKind::NotBetween => "NOT BETWEEN",
                    AExprKind::BetweenSym => "BETWEEN SYMMETRIC",
                    AExprKind::NotBetweenSym => "NOT BETWEEN SYMMETRIC",
                    _ => unreachable!(),
                };
                format!(
                    "({} {keyword} {} AND {})",
                    self.required_node(left, "AExpr", "lexpr")?,
                    self.render(&bounds[0])?,
                    self.render(&bounds[1])?
                )
            }
        };
        Ok(sql)
    }

    fn parenthesized_value(
        &self,
        node: Option<&Node>,
        owner: &'static str,
        field: &'static str,
    ) -> Result<std::string::String, DeparseError> {
        match node.ok_or_else(|| DeparseError::missing(owner, field))? {
            Node::AArrayExpr(array) => Ok(format!("({})", self.list(&array.elements, ", ")?)),
            other => Ok(format!("({})", self.render(other)?)),
        }
    }

    fn bool_expr(&self, expression: &BoolExpr) -> Result<std::string::String, DeparseError> {
        match expression.boolop {
            BoolExprType::NotExpr => {
                let argument = expression
                    .args
                    .first()
                    .ok_or_else(|| DeparseError::missing("BoolExpr", "args[0]"))?;
                Ok(format!("(NOT {})", self.render(argument)?))
            }
            BoolExprType::AndExpr | BoolExprType::OrExpr => {
                if expression.args.is_empty() {
                    return Err(DeparseError::missing("BoolExpr", "args"));
                }
                let separator = if expression.boolop == BoolExprType::AndExpr {
                    " AND "
                } else {
                    " OR "
                };
                Ok(format!("({})", self.list(&expression.args, separator)?))
            }
        }
    }

    fn type_cast(&self, cast: &TypeCast) -> Result<std::string::String, DeparseError> {
        let argument = self.required_node(cast.arg.as_deref(), "TypeCast", "arg")?;
        let type_name = self.type_name(
            cast.type_name
                .as_deref()
                .ok_or_else(|| DeparseError::missing("TypeCast", "type_name"))?,
        )?;
        Ok(format!("CAST({argument} AS {type_name})"))
    }

    fn type_name(&self, name: &TypeName) -> Result<std::string::String, DeparseError> {
        if name.names.is_empty() {
            return Err(DeparseError::missing("TypeName", "names"));
        }
        let mut sql = if name.setof { "SETOF " } else { "" }.to_owned();
        sql.push_str(&self.qualified_nodes(&name.names)?);
        if !name.typmods.is_empty() {
            sql.push('(');
            sql.push_str(&self.list(&name.typmods, ", ")?);
            sql.push(')');
        }
        if name.pct_type {
            sql.push_str("%TYPE");
        }
        for bound in &name.array_bounds {
            sql.push('[');
            if !matches!(bound, Node::Integer(Integer { ival: -1 })) {
                sql.push_str(&self.render(bound)?);
            }
            sql.push(']');
        }
        Ok(sql)
    }

    fn collate_clause(&self, clause: &CollateClause) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "({} COLLATE {})",
            self.required_node(clause.arg.as_deref(), "CollateClause", "arg")?,
            self.qualified_nodes(&clause.collname)?
        ))
    }

    fn func_call(&self, call: &FuncCall) -> Result<std::string::String, DeparseError> {
        let mut sql = self.qualified_nodes(&call.funcname)?;
        sql.push('(');
        if call.agg_distinct {
            sql.push_str("DISTINCT ");
        }
        if call.agg_star {
            sql.push('*');
        } else {
            let mut args = call
                .args
                .iter()
                .map(|argument| self.render(argument))
                .collect::<Result<Vec<_>, _>>()?;
            if call.func_variadic
                && let Some(last) = args.last_mut()
            {
                last.insert_str(0, "VARIADIC ");
            }
            sql.push_str(&args.join(", "));
        }
        if !call.agg_order.is_empty() && !call.agg_within_group {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.list(&call.agg_order, ", ")?);
        }
        sql.push(')');
        if call.agg_within_group {
            sql.push_str(" WITHIN GROUP (ORDER BY ");
            sql.push_str(&self.list(&call.agg_order, ", ")?);
            sql.push(')');
        }
        if let Some(filter) = call.agg_filter.as_deref() {
            sql.push_str(" FILTER (WHERE ");
            sql.push_str(&self.render(filter)?);
            sql.push(')');
        }
        if let Some(over) = call.over.as_deref() {
            sql.push_str(" OVER ");
            if over.partition_clause.is_empty()
                && over.order_clause.is_empty()
                && over.frame_options == 0
                && over.refname.is_none()
                && over.name.is_some()
            {
                sql.push_str(&quote_identifier(over.name.as_deref().unwrap()));
            } else {
                sql.push_str(&self.window_def(over)?);
            }
        }
        Ok(sql)
    }

    fn named_argument(&self, argument: &NamedArgExpr) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "{} => {}",
            quote_identifier(
                argument
                    .name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("NamedArgExpr", "name"))?
            ),
            self.required_node(argument.arg.as_deref(), "NamedArgExpr", "arg")?
        ))
    }

    fn a_indirection(
        &self,
        indirection: &AIndirection,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = self.required_node(indirection.arg.as_deref(), "AIndirection", "arg")?;
        for item in &indirection.indirection {
            match item {
                Node::String(name) => {
                    sql.push('.');
                    sql.push_str(&quote_identifier(
                        name.sval
                            .as_deref()
                            .ok_or_else(|| DeparseError::missing("String", "sval"))?,
                    ));
                }
                Node::AStar => sql.push_str(".*"),
                Node::AIndices(indices) => sql.push_str(&self.indices(indices)?),
                _ => {
                    return Err(DeparseError::invalid(
                        "AIndirection",
                        "indirection entries must be String, AStar, or AIndices nodes",
                    ));
                }
            }
        }
        Ok(sql)
    }

    fn indices(&self, indices: &AIndices) -> Result<std::string::String, DeparseError> {
        let mut sql = "[".to_owned();
        if indices.is_slice {
            if let Some(lower) = indices.lidx.as_deref() {
                sql.push_str(&self.render(lower)?);
            }
            sql.push(':');
            if let Some(upper) = indices.uidx.as_deref() {
                sql.push_str(&self.render(upper)?);
            }
        } else {
            sql.push_str(&self.required_node(indices.uidx.as_deref(), "AIndices", "uidx")?);
        }
        sql.push(']');
        Ok(sql)
    }

    fn array_expr(&self, array: &AArrayExpr) -> Result<std::string::String, DeparseError> {
        Ok(format!("ARRAY[{}]", self.list(&array.elements, ", ")?))
    }

    fn result_target(&self, target: &ResTarget) -> Result<std::string::String, DeparseError> {
        let mut sql = self.required_node(target.val.as_deref(), "ResTarget", "val")?;
        if let Some(name) = target.name.as_deref() {
            sql.push_str(" AS ");
            sql.push_str(&quote_identifier(name));
        }
        Ok(sql)
    }

    fn assignment_target(&self, target: &ResTarget) -> Result<std::string::String, DeparseError> {
        let mut sql = quote_identifier(
            target
                .name
                .as_deref()
                .ok_or_else(|| DeparseError::missing("ResTarget", "name"))?,
        );
        for item in &target.indirection {
            match item {
                Node::String(name) => {
                    sql.push('.');
                    sql.push_str(&quote_identifier(
                        name.sval
                            .as_deref()
                            .ok_or_else(|| DeparseError::missing("String", "sval"))?,
                    ));
                }
                Node::AIndices(indices) => sql.push_str(&self.indices(indices)?),
                _ => return Err(DeparseError::invalid("ResTarget", "invalid indirection")),
            }
        }
        Ok(sql)
    }

    fn update_targets(&self, targets: &[Node]) -> Result<std::string::String, DeparseError> {
        let mut assignments = Vec::new();
        let mut index = 0;
        while index < targets.len() {
            let Node::ResTarget(target) = &targets[index] else {
                return Err(DeparseError::invalid(
                    "UpdateStmt",
                    "target_list entries must be ResTarget nodes",
                ));
            };
            if let Some(Node::MultiAssignRef(reference)) = target.val.as_deref() {
                let count = usize::try_from(reference.ncolumns)
                    .map_err(|_| DeparseError::invalid("MultiAssignRef", "negative ncolumns"))?;
                if count == 0 || index + count > targets.len() {
                    return Err(DeparseError::invalid("MultiAssignRef", "invalid ncolumns"));
                }
                let names = targets[index..index + count]
                    .iter()
                    .map(|node| match node {
                        Node::ResTarget(target) => self.assignment_target(target),
                        _ => Err(DeparseError::invalid("MultiAssignRef", "invalid target")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                assignments.push(format!(
                    "({}) = {}",
                    names.join(", "),
                    self.required_node(reference.source.as_deref(), "MultiAssignRef", "source")?
                ));
                index += count;
            } else {
                assignments.push(format!(
                    "{} = {}",
                    self.assignment_target(target)?,
                    self.required_node(target.val.as_deref(), "ResTarget", "val")?
                ));
                index += 1;
            }
        }
        Ok(assignments.join(", "))
    }

    fn sort_by(&self, sort: &SortBy) -> Result<std::string::String, DeparseError> {
        let mut sql = self.required_node(sort.node.as_deref(), "SortBy", "node")?;
        match sort.sortby_dir {
            SortByDir::Default => {}
            SortByDir::Asc => sql.push_str(" ASC"),
            SortByDir::Desc => sql.push_str(" DESC"),
            SortByDir::Using => {
                sql.push_str(" USING ");
                sql.push_str(&operator_name(&sort.use_op)?);
            }
        }
        match sort.sortby_nulls {
            SortByNulls::Default => {}
            SortByNulls::First => sql.push_str(" NULLS FIRST"),
            SortByNulls::Last => sql.push_str(" NULLS LAST"),
        }
        Ok(sql)
    }

    fn window_def(&self, window: &WindowDef) -> Result<std::string::String, DeparseError> {
        let mut parts = Vec::new();
        if let Some(reference) = window.refname.as_deref() {
            parts.push(quote_identifier(reference));
        }
        if !window.partition_clause.is_empty() {
            parts.push(format!(
                "PARTITION BY {}",
                self.list(&window.partition_clause, ", ")?
            ));
        }
        if !window.order_clause.is_empty() {
            parts.push(format!(
                "ORDER BY {}",
                self.list(&window.order_clause, ", ")?
            ));
        }
        if window.frame_options != 0 && window.frame_options != FRAMEOPTION_DEFAULTS {
            parts.push(self.window_frame(window)?);
        }
        let body = format!("({})", parts.join(" "));
        if let Some(name) = window.name.as_deref() {
            Ok(format!("{} AS {body}", quote_identifier(name)))
        } else {
            Ok(body)
        }
    }

    fn window_frame(&self, window: &WindowDef) -> Result<std::string::String, DeparseError> {
        let options = window.frame_options;
        let mode = if options & FRAMEOPTION_ROWS != 0 {
            "ROWS"
        } else if options & FRAMEOPTION_GROUPS != 0 {
            "GROUPS"
        } else {
            "RANGE"
        };
        let start = self.frame_bound(options, true, window.start_offset.as_deref())?;
        let mut sql = if options & FRAMEOPTION_BETWEEN != 0 {
            let end = self.frame_bound(options, false, window.end_offset.as_deref())?;
            format!("{mode} BETWEEN {start} AND {end}")
        } else {
            format!("{mode} {start}")
        };
        if options & FRAMEOPTION_EXCLUDE_CURRENT_ROW != 0 {
            sql.push_str(" EXCLUDE CURRENT ROW");
        } else if options & FRAMEOPTION_EXCLUDE_GROUP != 0 {
            sql.push_str(" EXCLUDE GROUP");
        } else if options & FRAMEOPTION_EXCLUDE_TIES != 0 {
            sql.push_str(" EXCLUDE TIES");
        }
        Ok(sql)
    }

    fn frame_bound(
        &self,
        options: i32,
        start: bool,
        offset: Option<&Node>,
    ) -> Result<std::string::String, DeparseError> {
        let (
            unbounded_preceding,
            current_row,
            offset_preceding,
            offset_following,
            unbounded_following,
        ) = if start {
            (
                FRAMEOPTION_START_UNBOUNDED_PRECEDING,
                FRAMEOPTION_START_CURRENT_ROW,
                FRAMEOPTION_START_OFFSET_PRECEDING,
                FRAMEOPTION_START_OFFSET_FOLLOWING,
                FRAMEOPTION_START_UNBOUNDED_FOLLOWING,
            )
        } else {
            (
                FRAMEOPTION_END_UNBOUNDED_PRECEDING,
                FRAMEOPTION_END_CURRENT_ROW,
                FRAMEOPTION_END_OFFSET_PRECEDING,
                FRAMEOPTION_END_OFFSET_FOLLOWING,
                FRAMEOPTION_END_UNBOUNDED_FOLLOWING,
            )
        };
        if options & unbounded_preceding != 0 {
            Ok("UNBOUNDED PRECEDING".to_owned())
        } else if options & current_row != 0 {
            Ok("CURRENT ROW".to_owned())
        } else if options & offset_preceding != 0 {
            Ok(format!(
                "{} PRECEDING",
                self.required_node(offset, "WindowDef", "frame offset")?
            ))
        } else if options & offset_following != 0 {
            Ok(format!(
                "{} FOLLOWING",
                self.required_node(offset, "WindowDef", "frame offset")?
            ))
        } else if options & unbounded_following != 0 {
            Ok("UNBOUNDED FOLLOWING".to_owned())
        } else {
            Err(DeparseError::invalid("WindowDef", "invalid frame bound"))
        }
    }

    fn locking_clause(&self, locking: &LockingClause) -> Result<std::string::String, DeparseError> {
        let mut sql = match locking.strength {
            LockClauseStrength::None => "FOR UPDATE".to_owned(),
            LockClauseStrength::Forkeyshare => "FOR KEY SHARE".to_owned(),
            LockClauseStrength::Forshare => "FOR SHARE".to_owned(),
            LockClauseStrength::Fornokeyupdate => "FOR NO KEY UPDATE".to_owned(),
            LockClauseStrength::Forupdate => "FOR UPDATE".to_owned(),
        };
        if !locking.locked_rels.is_empty() {
            sql.push_str(" OF ");
            sql.push_str(&self.qualified_name_list(&locking.locked_rels, ", ")?);
        }
        match locking.wait_policy {
            LockWaitPolicy::Block => {}
            LockWaitPolicy::Skip => sql.push_str(" SKIP LOCKED"),
            LockWaitPolicy::Error => sql.push_str(" NOWAIT"),
        }
        Ok(sql)
    }

    fn grouping_set(&self, set: &GroupingSet) -> Result<std::string::String, DeparseError> {
        match set.kind {
            GroupingSetKind::Empty => Ok("()".to_owned()),
            GroupingSetKind::Simple => Ok(format!("({})", self.list(&set.content, ", ")?)),
            GroupingSetKind::Rollup => Ok(format!("ROLLUP ({})", self.list(&set.content, ", ")?)),
            GroupingSetKind::Cube => Ok(format!("CUBE ({})", self.list(&set.content, ", ")?)),
            GroupingSetKind::Sets => Ok(format!(
                "GROUPING SETS ({})",
                self.list(&set.content, ", ")?
            )),
        }
    }

    fn with_clause(&self, clause: &WithClause) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "WITH {}{}",
            if clause.recursive { "RECURSIVE " } else { "" },
            self.list(&clause.ctes, ", ")?
        ))
    }

    fn common_table_expression(
        &self,
        cte: &CommonTableExpr,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = quote_identifier(
            cte.ctename
                .as_deref()
                .ok_or_else(|| DeparseError::missing("CommonTableExpr", "ctename"))?,
        );
        if !cte.aliascolnames.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.qualified_name_list(&cte.aliascolnames, ", ")?);
            sql.push(')');
        }
        sql.push_str(" AS ");
        match cte.ctematerialized {
            CteMaterialize::Default => {}
            CteMaterialize::Always => sql.push_str("MATERIALIZED "),
            CteMaterialize::Never => sql.push_str("NOT MATERIALIZED "),
        }
        sql.push('(');
        sql.push_str(&self.required_node(
            cte.ctequery.as_deref(),
            "CommonTableExpr",
            "ctequery",
        )?);
        sql.push(')');
        if let Some(search) = cte.search_clause.as_deref() {
            sql.push_str(" SEARCH ");
            sql.push_str(if search.search_breadth_first {
                "BREADTH FIRST BY "
            } else {
                "DEPTH FIRST BY "
            });
            sql.push_str(&self.qualified_name_list(&search.search_col_list, ", ")?);
            sql.push_str(" SET ");
            sql.push_str(&quote_identifier(
                search
                    .search_seq_column
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("CteSearchClause", "search_seq_column"))?,
            ));
        }
        if let Some(cycle) = cte.cycle_clause.as_deref() {
            sql.push_str(" CYCLE ");
            sql.push_str(&self.qualified_name_list(&cycle.cycle_col_list, ", ")?);
            sql.push_str(" SET ");
            sql.push_str(&quote_identifier(
                cycle
                    .cycle_mark_column
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("CteCycleClause", "cycle_mark_column"))?,
            ));
            if let (Some(value), Some(default)) = (
                cycle.cycle_mark_value.as_deref(),
                cycle.cycle_mark_default.as_deref(),
            ) {
                sql.push_str(" TO ");
                sql.push_str(&self.render(value)?);
                sql.push_str(" DEFAULT ");
                sql.push_str(&self.render(default)?);
            }
            sql.push_str(" USING ");
            sql.push_str(&quote_identifier(
                cycle
                    .cycle_path_column
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("CteCycleClause", "cycle_path_column"))?,
            ));
        }
        Ok(sql)
    }

    fn returning_clause(
        &self,
        clause: &ReturningClause,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "RETURNING".to_owned();
        if !clause.options.is_empty() {
            sql.push_str(" WITH (");
            sql.push_str(&self.list(&clause.options, ", ")?);
            sql.push(')');
        }
        if !clause.exprs.is_empty() {
            sql.push(' ');
            sql.push_str(&self.list(&clause.exprs, ", ")?);
        }
        Ok(sql)
    }

    fn returning_option(
        &self,
        option: &ReturningOption,
    ) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "{} AS {}",
            match option.option {
                ReturningOptionKind::Old => "OLD",
                ReturningOptionKind::New => "NEW",
            },
            quote_identifier(
                option
                    .value
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("ReturningOption", "value"))?
            )
        ))
    }

    fn infer_clause(&self, clause: &InferClause) -> Result<std::string::String, DeparseError> {
        if let Some(constraint) = clause.conname.as_deref() {
            return Ok(format!("ON CONSTRAINT {}", quote_identifier(constraint)));
        }
        let mut sql = format!("({})", self.list(&clause.index_elems, ", ")?);
        if let Some(condition) = clause.where_clause.as_deref() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.render(condition)?);
        }
        Ok(sql)
    }

    fn on_conflict_clause(
        &self,
        clause: &OnConflictClause,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "ON CONFLICT".to_owned();
        if let Some(infer) = clause.infer.as_deref() {
            sql.push(' ');
            sql.push_str(&self.infer_clause(infer)?);
        }
        match clause.action {
            OnConflictAction::None => {
                return Err(DeparseError::invalid("OnConflictClause", "action is None"));
            }
            OnConflictAction::Nothing => sql.push_str(" DO NOTHING"),
            OnConflictAction::Update => {
                sql.push_str(" DO UPDATE SET ");
                sql.push_str(&self.update_targets(&clause.target_list)?);
                if let Some(condition) = clause.where_clause.as_deref() {
                    sql.push_str(" WHERE ");
                    sql.push_str(&self.render(condition)?);
                }
            }
            OnConflictAction::Select => {
                sql.push_str(" DO SELECT");
                sql.push_str(match clause.lock_strength {
                    LockClauseStrength::None => "",
                    LockClauseStrength::Forkeyshare => " FOR KEY SHARE",
                    LockClauseStrength::Forshare => " FOR SHARE",
                    LockClauseStrength::Fornokeyupdate => " FOR NO KEY UPDATE",
                    LockClauseStrength::Forupdate => " FOR UPDATE",
                });
                if let Some(condition) = clause.where_clause.as_deref() {
                    sql.push_str(" WHERE ");
                    sql.push_str(&self.render(condition)?);
                }
            }
        }
        Ok(sql)
    }

    fn merge_when_clause(
        &self,
        clause: &MergeWhenClause,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = match clause.match_kind {
            MergeMatchKind::Matched => "WHEN MATCHED".to_owned(),
            MergeMatchKind::NotMatchedBySource => "WHEN NOT MATCHED BY SOURCE".to_owned(),
            MergeMatchKind::NotMatchedByTarget => "WHEN NOT MATCHED BY TARGET".to_owned(),
        };
        if let Some(condition) = clause.condition.as_deref() {
            sql.push_str(" AND ");
            sql.push_str(&self.render(condition)?);
        }
        sql.push_str(" THEN ");
        match clause.command_type {
            CmdType::Nothing => sql.push_str("DO NOTHING"),
            CmdType::Delete => sql.push_str("DELETE"),
            CmdType::Update => {
                sql.push_str("UPDATE SET ");
                sql.push_str(&self.update_targets(&clause.target_list)?);
            }
            CmdType::Insert => {
                sql.push_str("INSERT");
                if !clause.target_list.is_empty() {
                    let names = clause
                        .target_list
                        .iter()
                        .map(|node| match node {
                            Node::ResTarget(target) => self.assignment_target(target),
                            _ => Err(DeparseError::invalid(
                                "MergeWhenClause",
                                "invalid insert target",
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    sql.push_str(" (");
                    sql.push_str(&names.join(", "));
                    sql.push(')');
                }
                sql.push_str(match clause.override_ {
                    OverridingKind::NotSet => "",
                    OverridingKind::UserValue => " OVERRIDING USER VALUE",
                    OverridingKind::SystemValue => " OVERRIDING SYSTEM VALUE",
                });
                if clause.values.is_empty() {
                    sql.push_str(" DEFAULT VALUES");
                } else {
                    sql.push_str(" VALUES (");
                    sql.push_str(&self.list(&clause.values, ", ")?);
                    sql.push(')');
                }
            }
            CmdType::Unknown | CmdType::Select | CmdType::Merge | CmdType::Utility => {
                return Err(DeparseError::invalid(
                    "MergeWhenClause",
                    "invalid command_type",
                ));
            }
        }
        Ok(sql)
    }

    fn index_element(&self, element: &IndexElem) -> Result<std::string::String, DeparseError> {
        let mut sql = if let Some(name) = element.name.as_deref() {
            quote_identifier(name)
        } else {
            format!(
                "({})",
                self.required_node(element.expr.as_deref(), "IndexElem", "expr")?
            )
        };
        if !element.collation.is_empty() {
            sql.push_str(" COLLATE ");
            sql.push_str(&self.qualified_nodes(&element.collation)?);
        }
        if !element.opclass.is_empty() {
            sql.push(' ');
            sql.push_str(&self.qualified_nodes(&element.opclass)?);
        }
        match element.ordering {
            SortByDir::Default => {}
            SortByDir::Asc => sql.push_str(" ASC"),
            SortByDir::Desc => sql.push_str(" DESC"),
            SortByDir::Using => {
                return Err(DeparseError::invalid(
                    "IndexElem",
                    "USING ordering is invalid",
                ));
            }
        }
        match element.nulls_ordering {
            SortByNulls::Default => {}
            SortByNulls::First => sql.push_str(" NULLS FIRST"),
            SortByNulls::Last => sql.push_str(" NULLS LAST"),
        }
        Ok(sql)
    }

    fn for_portion_of(
        &self,
        portion: &ForPortionOfClause,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = format!(
            "FOR PORTION OF {}",
            quote_identifier(
                portion
                    .range_name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("ForPortionOfClause", "range_name"))?
            )
        );
        if let Some(target) = portion.target.as_deref() {
            sql.push_str(" (");
            sql.push_str(&self.render(target)?);
            sql.push(')');
        } else {
            sql.push_str(" FROM ");
            sql.push_str(&self.required_node(
                portion.target_start.as_deref(),
                "ForPortionOfClause",
                "target_start",
            )?);
            sql.push_str(" TO ");
            sql.push_str(&self.required_node(
                portion.target_end.as_deref(),
                "ForPortionOfClause",
                "target_end",
            )?);
        }
        Ok(sql)
    }

    fn render_into_clause(&self, into: &IntoClause) -> Result<std::string::String, DeparseError> {
        let relation = into
            .rel
            .as_deref()
            .ok_or_else(|| DeparseError::missing("IntoClause", "rel"))?;
        let persistence = match relation.relpersistence {
            b't' => "TEMPORARY ",
            b'u' => "UNLOGGED ",
            _ => "",
        };
        let mut bare_relation = relation.clone();
        bare_relation.alias = None;
        let mut sql = format!(
            "INTO {persistence}TABLE {}",
            self.range_var(&bare_relation)?
        );
        if !into.col_names.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.qualified_name_list(&into.col_names, ", ")?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn min_max(&self, expression: &MinMaxExpr) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "{}({})",
            match expression.op {
                MinMaxOp::Greatest => "GREATEST",
                MinMaxOp::Least => "LEAST",
            },
            self.list(&expression.args, ", ")?
        ))
    }

    fn sql_value_function(
        &self,
        function: &SqlValueFunction,
    ) -> Result<std::string::String, DeparseError> {
        Ok(match function.op {
            SqlValueFunctionOp::CurrentDate => "CURRENT_DATE".to_owned(),
            SqlValueFunctionOp::CurrentTime => "CURRENT_TIME".to_owned(),
            SqlValueFunctionOp::CurrentTimeN => format!("CURRENT_TIME({})", function.typmod),
            SqlValueFunctionOp::CurrentTimestamp => "CURRENT_TIMESTAMP".to_owned(),
            SqlValueFunctionOp::CurrentTimestampN => {
                format!("CURRENT_TIMESTAMP({})", function.typmod)
            }
            SqlValueFunctionOp::Localtime => "LOCALTIME".to_owned(),
            SqlValueFunctionOp::LocaltimeN => format!("LOCALTIME({})", function.typmod),
            SqlValueFunctionOp::Localtimestamp => "LOCALTIMESTAMP".to_owned(),
            SqlValueFunctionOp::LocaltimestampN => format!("LOCALTIMESTAMP({})", function.typmod),
            SqlValueFunctionOp::CurrentRole => "CURRENT_ROLE".to_owned(),
            SqlValueFunctionOp::CurrentUser => "CURRENT_USER".to_owned(),
            SqlValueFunctionOp::User => "USER".to_owned(),
            SqlValueFunctionOp::SessionUser => "SESSION_USER".to_owned(),
            SqlValueFunctionOp::CurrentCatalog => "CURRENT_CATALOG".to_owned(),
            SqlValueFunctionOp::CurrentSchema => "CURRENT_SCHEMA".to_owned(),
        })
    }

    fn null_test(&self, test: &NullTest) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "({} IS {}NULL)",
            self.required_node(test.arg.as_deref(), "NullTest", "arg")?,
            if test.nulltesttype == NullTestType::NotNull {
                "NOT "
            } else {
                ""
            }
        ))
    }

    fn boolean_test(&self, test: &BooleanTest) -> Result<std::string::String, DeparseError> {
        let predicate = match test.booltesttype {
            BoolTestType::True => "TRUE",
            BoolTestType::NotTrue => "NOT TRUE",
            BoolTestType::False => "FALSE",
            BoolTestType::NotFalse => "NOT FALSE",
            BoolTestType::Unknown => "UNKNOWN",
            BoolTestType::NotUnknown => "NOT UNKNOWN",
        };
        Ok(format!(
            "({} IS {predicate})",
            self.required_node(test.arg.as_deref(), "BooleanTest", "arg")?
        ))
    }

    fn sub_link(&self, link: &SubLink) -> Result<std::string::String, DeparseError> {
        let query = self.required_node(link.subselect.as_deref(), "SubLink", "subselect")?;
        match link.sub_link_type {
            SubLinkType::ExistsSublink => Ok(format!("EXISTS ({query})")),
            SubLinkType::ExprSublink => Ok(format!("({query})")),
            SubLinkType::ArraySublink => Ok(format!("ARRAY({query})")),
            SubLinkType::AnySublink | SubLinkType::AllSublink => Ok(format!(
                "({} {} {} ({query}))",
                self.required_node(link.testexpr.as_deref(), "SubLink", "testexpr")?,
                operator_name(&link.oper_name)?,
                if link.sub_link_type == SubLinkType::AnySublink {
                    "ANY"
                } else {
                    "ALL"
                }
            )),
            SubLinkType::RowcompareSublink => Ok(format!(
                "({} {} ({query}))",
                self.required_node(link.testexpr.as_deref(), "SubLink", "testexpr")?,
                operator_name(&link.oper_name)?
            )),
            SubLinkType::MultiexprSublink | SubLinkType::CteSublink => {
                Err(DeparseError::unsupported("analysis-tree SubLink"))
            }
        }
    }

    fn case_expr(&self, expression: &CaseExpr) -> Result<std::string::String, DeparseError> {
        let mut sql = "CASE".to_owned();
        if let Some(argument) = expression.arg.as_deref() {
            sql.push(' ');
            sql.push_str(&self.render(argument)?);
        }
        for when in &expression.args {
            sql.push(' ');
            sql.push_str(&self.render(when)?);
        }
        if let Some(default) = expression.defresult.as_deref() {
            sql.push_str(" ELSE ");
            sql.push_str(&self.render(default)?);
        }
        sql.push_str(" END");
        Ok(sql)
    }

    fn case_when(&self, when: &CaseWhen) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "WHEN {} THEN {}",
            self.required_node(when.expr.as_deref(), "CaseWhen", "expr")?,
            self.required_node(when.result.as_deref(), "CaseWhen", "result")?
        ))
    }

    fn current_of(&self, current: &CurrentOfExpr) -> Result<std::string::String, DeparseError> {
        let name = current
            .cursor_name
            .as_deref()
            .ok_or_else(|| DeparseError::unsupported("parameterized CurrentOfExpr"))?;
        Ok(format!("CURRENT OF {}", quote_identifier(name)))
    }

    fn role(&self, role: &RoleSpec) -> Result<std::string::String, DeparseError> {
        Ok(match role.roletype {
            RoleSpecType::Cstring => quote_identifier(
                role.rolename
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("RoleSpec", "rolename"))?,
            ),
            RoleSpecType::CurrentRole => "CURRENT_ROLE".to_owned(),
            RoleSpecType::CurrentUser => "CURRENT_USER".to_owned(),
            RoleSpecType::SessionUser => "SESSION_USER".to_owned(),
            RoleSpecType::Public => "PUBLIC".to_owned(),
        })
    }

    fn merge_support_function(
        &self,
        _function: &MergeSupportFunc,
    ) -> Result<std::string::String, DeparseError> {
        Ok("MERGE_ACTION()".to_owned())
    }

    fn xml_expr(&self, expression: &XmlExpr) -> Result<std::string::String, DeparseError> {
        match expression.op {
            XmlExprOp::Xmlconcat => {
                Ok(format!("XMLCONCAT({})", self.list(&expression.args, ", ")?))
            }
            XmlExprOp::Xmlparse => Ok(format!(
                "XMLPARSE({} {}{} )",
                if expression.xmloption == XmlOptionType::Document {
                    "DOCUMENT"
                } else {
                    "CONTENT"
                },
                self.list(&expression.args, ", ")?,
                if expression.indent {
                    " PRESERVE WHITESPACE"
                } else {
                    " STRIP WHITESPACE"
                }
            )),
            _ => Err(DeparseError::unsupported("XmlExpr variant")),
        }
    }

    fn xml_serialize(
        &self,
        expression: &XmlSerialize,
    ) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "XMLSERIALIZE({} {} AS {}{})",
            if expression.xmloption == XmlOptionType::Document {
                "DOCUMENT"
            } else {
                "CONTENT"
            },
            self.required_node(expression.expr.as_deref(), "XmlSerialize", "expr")?,
            self.type_name(
                expression
                    .type_name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("XmlSerialize", "type_name"))?
            )?,
            if expression.indent {
                " INDENT"
            } else {
                " NO INDENT"
            }
        ))
    }

    fn create_schema(
        &self,
        statement: &CreateSchemaStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "CREATE SCHEMA ".to_owned();
        if statement.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        if let Some(name) = statement.schemaname.as_deref() {
            sql.push_str(&quote_identifier(name));
        }
        if let Some(role) = statement.authrole.as_deref() {
            if statement.schemaname.is_some() {
                sql.push_str(" AUTHORIZATION ");
            } else {
                sql.push_str("AUTHORIZATION ");
            }
            sql.push_str(&self.role(role)?);
        }
        for element in &statement.schema_elts {
            sql.push(' ');
            sql.push_str(&self.render(element)?);
        }
        Ok(sql)
    }

    fn create_table(&self, statement: &CreateStmt) -> Result<std::string::String, DeparseError> {
        let relation = statement
            .relation
            .as_deref()
            .ok_or_else(|| DeparseError::missing("CreateStmt", "relation"))?;
        let mut sql = "CREATE ".to_owned();
        match relation.relpersistence {
            b't' => sql.push_str("TEMPORARY "),
            b'u' => sql.push_str("UNLOGGED "),
            _ => {}
        }
        sql.push_str("TABLE ");
        if statement.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        let mut relation = relation.clone();
        relation.alias = None;
        relation.inh = true;
        sql.push_str(&self.range_var(&relation)?);
        if let Some(type_name) = statement.of_typename.as_deref() {
            sql.push_str(" OF ");
            sql.push_str(&self.type_name(type_name)?);
        }
        let mut elements = statement.table_elts.clone();
        elements.extend(statement.constraints.iter().cloned());
        if !elements.is_empty() || statement.of_typename.is_none() {
            sql.push_str(" (");
            sql.push_str(&self.list(&elements, ", ")?);
            sql.push(')');
        }
        if !statement.inh_relations.is_empty() {
            sql.push_str(" INHERITS (");
            sql.push_str(&self.list(&statement.inh_relations, ", ")?);
            sql.push(')');
        }
        if let Some(spec) = statement.partspec.as_deref() {
            sql.push_str(" PARTITION BY ");
            sql.push_str(match spec.strategy {
                PartitionStrategy::List => "LIST",
                PartitionStrategy::Range => "RANGE",
                PartitionStrategy::Hash => "HASH",
            });
            sql.push_str(" (");
            sql.push_str(&self.list(&spec.part_params, ", ")?);
            sql.push(')');
        }
        if !statement.options.is_empty() {
            sql.push_str(" WITH (");
            sql.push_str(&self.list(&statement.options, ", ")?);
            sql.push(')');
        }
        if let Some(method) = statement.access_method.as_deref() {
            sql.push_str(" USING ");
            sql.push_str(&quote_identifier(method));
        }
        match statement.oncommit {
            OnCommitAction::Noop => {}
            OnCommitAction::PreserveRows => sql.push_str(" ON COMMIT PRESERVE ROWS"),
            OnCommitAction::DeleteRows => sql.push_str(" ON COMMIT DELETE ROWS"),
            OnCommitAction::Drop => sql.push_str(" ON COMMIT DROP"),
        }
        if let Some(tablespace) = statement.tablespacename.as_deref() {
            sql.push_str(" TABLESPACE ");
            sql.push_str(&quote_identifier(tablespace));
        }
        Ok(sql)
    }

    fn column_definition(&self, column: &ColumnDef) -> Result<std::string::String, DeparseError> {
        let mut sql = quote_identifier(
            column
                .colname
                .as_deref()
                .ok_or_else(|| DeparseError::missing("ColumnDef", "colname"))?,
        );
        if let Some(type_name) = column.type_name.as_deref() {
            sql.push(' ');
            sql.push_str(&self.type_name(type_name)?);
        }
        if let Some(compression) = column.compression.as_deref() {
            sql.push_str(" COMPRESSION ");
            sql.push_str(&quote_identifier(compression));
        }
        if let Some(collation) = column.coll_clause.as_deref() {
            sql.push_str(" COLLATE ");
            sql.push_str(&self.qualified_nodes(&collation.collname)?);
        }
        for constraint in &column.constraints {
            sql.push(' ');
            sql.push_str(&self.render(constraint)?);
        }
        if !column.fdwoptions.is_empty() {
            sql.push_str(" OPTIONS (");
            sql.push_str(&self.list(&column.fdwoptions, ", ")?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn constraint(&self, constraint: &Constraint) -> Result<std::string::String, DeparseError> {
        let mut sql = std::string::String::new();
        if let Some(name) = constraint.conname.as_deref() {
            sql.push_str("CONSTRAINT ");
            sql.push_str(&quote_identifier(name));
            sql.push(' ');
        }
        match constraint.contype {
            ConstrType::Null => sql.push_str("NULL"),
            ConstrType::Notnull => sql.push_str("NOT NULL"),
            ConstrType::Default => {
                sql.push_str("DEFAULT ");
                sql.push_str(&self.required_node(
                    constraint.raw_expr.as_deref(),
                    "Constraint",
                    "raw_expr",
                )?);
            }
            ConstrType::Identity => {
                sql.push_str(if constraint.generated_when == b'd' {
                    "GENERATED BY DEFAULT AS IDENTITY"
                } else {
                    "GENERATED ALWAYS AS IDENTITY"
                });
                if !constraint.options.is_empty() {
                    sql.push_str(" (");
                    sql.push_str(&self.list(&constraint.options, " ")?);
                    sql.push(')');
                }
            }
            ConstrType::Generated => {
                sql.push_str("GENERATED ALWAYS AS (");
                sql.push_str(&self.required_node(
                    constraint.raw_expr.as_deref(),
                    "Constraint",
                    "raw_expr",
                )?);
                sql.push_str(") ");
                sql.push_str(if constraint.generated_kind == b's' {
                    "STORED"
                } else {
                    "VIRTUAL"
                });
            }
            ConstrType::Check => {
                sql.push_str("CHECK (");
                sql.push_str(&self.required_node(
                    constraint.raw_expr.as_deref(),
                    "Constraint",
                    "raw_expr",
                )?);
                sql.push(')');
            }
            ConstrType::Primary | ConstrType::Unique => {
                sql.push_str(if constraint.contype == ConstrType::Primary {
                    "PRIMARY KEY"
                } else {
                    "UNIQUE"
                });
                if constraint.nulls_not_distinct {
                    sql.push_str(" NULLS NOT DISTINCT");
                }
                if !constraint.keys.is_empty() {
                    sql.push_str(" (");
                    sql.push_str(&self.qualified_name_list(&constraint.keys, ", ")?);
                    if constraint.without_overlaps {
                        sql.push_str(" WITHOUT OVERLAPS");
                    }
                    sql.push(')');
                }
            }
            ConstrType::Foreign => {
                if !constraint.fk_attrs.is_empty() {
                    sql.push_str("FOREIGN KEY (");
                    sql.push_str(&self.qualified_name_list(&constraint.fk_attrs, ", ")?);
                    if constraint.fk_with_period {
                        sql.push_str(" PERIOD");
                    }
                    sql.push_str(") ");
                }
                sql.push_str("REFERENCES ");
                sql.push_str(
                    &self.range_var(
                        constraint
                            .pktable
                            .as_deref()
                            .ok_or_else(|| DeparseError::missing("Constraint", "pktable"))?,
                    )?,
                );
                if !constraint.pk_attrs.is_empty() {
                    sql.push_str(" (");
                    sql.push_str(&self.qualified_name_list(&constraint.pk_attrs, ", ")?);
                    if constraint.pk_with_period {
                        sql.push_str(" PERIOD");
                    }
                    sql.push(')');
                }
                self.foreign_key_actions(constraint, &mut sql)?;
            }
            ConstrType::AttrDeferrable => sql.push_str("DEFERRABLE"),
            ConstrType::AttrNotDeferrable => sql.push_str("NOT DEFERRABLE"),
            ConstrType::AttrDeferred => sql.push_str("INITIALLY DEFERRED"),
            ConstrType::AttrImmediate => sql.push_str("INITIALLY IMMEDIATE"),
            ConstrType::AttrEnforced => sql.push_str("ENFORCED"),
            ConstrType::AttrNotEnforced => sql.push_str("NOT ENFORCED"),
            ConstrType::Exclusion => return Err(DeparseError::unsupported("exclusion Constraint")),
        }
        if constraint.is_no_inherit {
            sql.push_str(" NO INHERIT");
        }
        if constraint.skip_validation {
            sql.push_str(" NOT VALID");
        }
        Ok(sql)
    }

    fn foreign_key_actions(
        &self,
        constraint: &Constraint,
        sql: &mut std::string::String,
    ) -> Result<(), DeparseError> {
        let action = |code| match code {
            FKCONSTR_ACTION_NOACTION => None,
            FKCONSTR_ACTION_RESTRICT => Some("RESTRICT"),
            FKCONSTR_ACTION_CASCADE => Some("CASCADE"),
            FKCONSTR_ACTION_SETNULL => Some("SET NULL"),
            FKCONSTR_ACTION_SETDEFAULT => Some("SET DEFAULT"),
            _ => None,
        };
        if let Some(update) = action(constraint.fk_upd_action) {
            sql.push_str(" ON UPDATE ");
            sql.push_str(update);
        }
        if let Some(delete) = action(constraint.fk_del_action) {
            sql.push_str(" ON DELETE ");
            sql.push_str(delete);
            if !constraint.fk_del_set_cols.is_empty() {
                sql.push_str(" (");
                sql.push_str(&self.qualified_name_list(&constraint.fk_del_set_cols, ", ")?);
                sql.push(')');
            }
        }
        match constraint.fk_matchtype {
            FKCONSTR_MATCH_FULL => sql.push_str(" MATCH FULL"),
            FKCONSTR_MATCH_PARTIAL => sql.push_str(" MATCH PARTIAL"),
            _ => {}
        }
        Ok(())
    }

    fn definition_element(&self, element: &DefElem) -> Result<std::string::String, DeparseError> {
        let mut sql = std::string::String::new();
        if let Some(namespace) = element.defnamespace.as_deref() {
            sql.push_str(&quote_identifier(namespace));
            sql.push('.');
        }
        sql.push_str(&quote_identifier(
            element
                .defname
                .as_deref()
                .ok_or_else(|| DeparseError::missing("DefElem", "defname"))?,
        ));
        if let Some(argument) = element.arg.as_deref() {
            sql.push_str(" = ");
            sql.push_str(&self.render(argument)?);
        }
        Ok(sql)
    }

    fn create_index(&self, statement: &IndexStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = "CREATE ".to_owned();
        if statement.unique {
            sql.push_str("UNIQUE ");
        }
        sql.push_str("INDEX ");
        if statement.concurrent {
            sql.push_str("CONCURRENTLY ");
        }
        if statement.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        if let Some(name) = statement.idxname.as_deref() {
            sql.push_str(&quote_identifier(name));
            sql.push(' ');
        }
        sql.push_str("ON ");
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("IndexStmt", "relation"))?,
            )?,
        );
        if let Some(method) = statement.access_method.as_deref() {
            sql.push_str(" USING ");
            sql.push_str(&quote_identifier(method));
        }
        sql.push_str(" (");
        sql.push_str(&self.list(&statement.index_params, ", ")?);
        sql.push(')');
        if !statement.index_including_params.is_empty() {
            sql.push_str(" INCLUDE (");
            sql.push_str(&self.list(&statement.index_including_params, ", ")?);
            sql.push(')');
        }
        if statement.nulls_not_distinct {
            sql.push_str(" NULLS NOT DISTINCT");
        }
        if !statement.options.is_empty() {
            sql.push_str(" WITH (");
            sql.push_str(&self.list(&statement.options, ", ")?);
            sql.push(')');
        }
        if let Some(tablespace) = statement.table_space.as_deref() {
            sql.push_str(" TABLESPACE ");
            sql.push_str(&quote_identifier(tablespace));
        }
        if let Some(condition) = statement.where_clause.as_deref() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.render(condition)?);
        }
        Ok(sql)
    }

    fn drop_statement(&self, statement: &DropStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = format!("DROP {} ", object_type_sql(statement.remove_type));
        if statement.concurrent {
            sql.push_str("CONCURRENTLY ");
        }
        if statement.missing_ok {
            sql.push_str("IF EXISTS ");
        }
        let objects = statement
            .objects
            .iter()
            .map(|object| self.object_identity(object))
            .collect::<Result<Vec<_>, _>>()?;
        sql.push_str(&objects.join(", "));
        if statement.behavior == DropBehavior::Cascade {
            sql.push_str(" CASCADE");
        }
        Ok(sql)
    }

    fn object_identity(&self, node: &Node) -> Result<std::string::String, DeparseError> {
        match node {
            Node::AArrayExpr(array) => self.qualified_nodes(&array.elements),
            Node::ObjectWithArgs(object) => self.object_with_args(object),
            Node::TypeName(name) => self.type_name(name),
            Node::RangeVar(range) => self.range_var(range),
            other => self.render(other),
        }
    }

    fn object_with_args(
        &self,
        object: &ObjectWithArgs,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = self.qualified_nodes(&object.objname)?;
        if !object.args_unspecified {
            let args = object
                .objargs
                .iter()
                .map(|argument| match argument {
                    Some(node) => self.render(node),
                    None => Ok("NONE".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            sql.push('(');
            sql.push_str(&args.join(", "));
            sql.push(')');
        }
        Ok(sql)
    }

    fn truncate(&self, statement: &TruncateStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = format!("TRUNCATE TABLE {}", self.list(&statement.relations, ", ")?);
        sql.push_str(if statement.restart_seqs {
            " RESTART IDENTITY"
        } else {
            " CONTINUE IDENTITY"
        });
        if statement.behavior == DropBehavior::Cascade {
            sql.push_str(" CASCADE");
        }
        Ok(sql)
    }

    fn create_view(&self, statement: &ViewStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = if statement.replace {
            "CREATE OR REPLACE VIEW ".to_owned()
        } else {
            "CREATE VIEW ".to_owned()
        };
        sql.push_str(
            &self.range_var(
                statement
                    .view
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("ViewStmt", "view"))?,
            )?,
        );
        if !statement.aliases.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.qualified_name_list(&statement.aliases, ", ")?);
            sql.push(')');
        }
        if !statement.options.is_empty() {
            sql.push_str(" WITH (");
            sql.push_str(&self.list(&statement.options, ", ")?);
            sql.push(')');
        }
        sql.push_str(" AS ");
        sql.push_str(&self.required_node(statement.query.as_deref(), "ViewStmt", "query")?);
        match statement.with_check_option {
            ViewCheckOption::NoCheckOption => {}
            ViewCheckOption::LocalCheckOption => sql.push_str(" WITH LOCAL CHECK OPTION"),
            ViewCheckOption::CascadedCheckOption => sql.push_str(" WITH CASCADED CHECK OPTION"),
        }
        Ok(sql)
    }

    fn create_table_as(
        &self,
        statement: &CreateTableAsStmt,
    ) -> Result<std::string::String, DeparseError> {
        let into = statement
            .into
            .as_deref()
            .ok_or_else(|| DeparseError::missing("CreateTableAsStmt", "into"))?;
        let relation = into
            .rel
            .as_deref()
            .ok_or_else(|| DeparseError::missing("IntoClause", "rel"))?;
        let mut sql = "CREATE ".to_owned();
        if relation.relpersistence == b't' {
            sql.push_str("TEMPORARY ");
        } else if relation.relpersistence == b'u' {
            sql.push_str("UNLOGGED ");
        }
        sql.push_str(if statement.objtype == ObjectType::Matview {
            "MATERIALIZED VIEW "
        } else {
            "TABLE "
        });
        if statement.if_not_exists {
            sql.push_str("IF NOT EXISTS ");
        }
        sql.push_str(&self.range_var(relation)?);
        if !into.col_names.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.qualified_name_list(&into.col_names, ", ")?);
            sql.push(')');
        }
        sql.push_str(" AS ");
        sql.push_str(&self.required_node(
            statement.query.as_deref(),
            "CreateTableAsStmt",
            "query",
        )?);
        if into.skip_data {
            sql.push_str(" WITH NO DATA");
        }
        Ok(sql)
    }

    fn create_domain(
        &self,
        statement: &CreateDomainStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = format!(
            "CREATE DOMAIN {} AS {}",
            self.qualified_nodes(&statement.domainname)?,
            self.type_name(
                statement
                    .type_name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("CreateDomainStmt", "type_name"))?
            )?
        );
        if let Some(collation) = statement.coll_clause.as_deref() {
            sql.push_str(" COLLATE ");
            sql.push_str(&self.qualified_nodes(&collation.collname)?);
        }
        for constraint in &statement.constraints {
            sql.push(' ');
            sql.push_str(&self.render(constraint)?);
        }
        Ok(sql)
    }

    fn create_enum(&self, statement: &CreateEnumStmt) -> Result<std::string::String, DeparseError> {
        let values = statement
            .vals
            .iter()
            .map(|node| match node {
                Node::String(value) => Ok(quote_literal(value.sval.as_deref().unwrap_or_default())),
                _ => Err(DeparseError::invalid(
                    "CreateEnumStmt",
                    "vals must be String nodes",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!(
            "CREATE TYPE {} AS ENUM ({})",
            self.qualified_nodes(&statement.type_name)?,
            values.join(", ")
        ))
    }

    fn create_range(
        &self,
        statement: &CreateRangeStmt,
    ) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "CREATE TYPE {} AS RANGE ({})",
            self.qualified_nodes(&statement.type_name)?,
            self.list(&statement.params, ", ")?
        ))
    }

    fn create_function(
        &self,
        statement: &CreateFunctionStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "CREATE ".to_owned();
        if statement.replace {
            sql.push_str("OR REPLACE ");
        }
        sql.push_str(if statement.is_procedure {
            "PROCEDURE "
        } else {
            "FUNCTION "
        });
        sql.push_str(&self.qualified_nodes(&statement.funcname)?);
        sql.push('(');
        sql.push_str(&self.list(&statement.parameters, ", ")?);
        sql.push(')');
        if let Some(return_type) = statement.return_type.as_deref() {
            sql.push_str(" RETURNS ");
            sql.push_str(&self.type_name(return_type)?);
        }
        for option in &statement.options {
            sql.push(' ');
            match option {
                Node::DefElem(element) => sql.push_str(&self.function_option(element)?),
                _ => sql.push_str(&self.render(option)?),
            }
        }
        if let Some(body) = statement.sql_body.as_deref() {
            sql.push(' ');
            sql.push_str(&self.render(body)?);
        }
        Ok(sql)
    }

    fn function_parameter(
        &self,
        parameter: &FunctionParameter,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = match parameter.mode {
            FunctionParameterMode::In => std::string::String::new(),
            FunctionParameterMode::Out => "OUT ".to_owned(),
            FunctionParameterMode::Inout => "INOUT ".to_owned(),
            FunctionParameterMode::Variadic => "VARIADIC ".to_owned(),
            FunctionParameterMode::Table => "TABLE ".to_owned(),
            FunctionParameterMode::Default => std::string::String::new(),
        };
        if let Some(name) = parameter.name.as_deref() {
            sql.push_str(&quote_identifier(name));
            sql.push(' ');
        }
        sql.push_str(
            &self.type_name(
                parameter
                    .arg_type
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("FunctionParameter", "arg_type"))?,
            )?,
        );
        if let Some(default) = parameter.defexpr.as_deref() {
            sql.push_str(" DEFAULT ");
            sql.push_str(&self.render(default)?);
        }
        Ok(sql)
    }

    fn function_option(&self, option: &DefElem) -> Result<std::string::String, DeparseError> {
        let name = option
            .defname
            .as_deref()
            .ok_or_else(|| DeparseError::missing("DefElem", "defname"))?;
        let keyword = name.replace('_', " ").to_ascii_uppercase();
        match option.arg.as_deref() {
            Some(Node::String(value)) => Ok(format!(
                "{keyword} {}",
                if matches!(name, "as" | "transform") {
                    quote_literal(value.sval.as_deref().unwrap_or_default())
                } else {
                    quote_identifier(value.sval.as_deref().unwrap_or_default())
                }
            )),
            Some(argument) => Ok(format!("{keyword} {}", self.render(argument)?)),
            None => Ok(keyword),
        }
    }

    fn call_statement(&self, statement: &CallStmt) -> Result<std::string::String, DeparseError> {
        let call = statement
            .funccall
            .as_deref()
            .ok_or_else(|| DeparseError::missing("CallStmt", "funccall"))?;
        Ok(format!("CALL {}", self.func_call(call)?))
    }

    fn variable_set(
        &self,
        statement: &VariableSetStmt,
    ) -> Result<std::string::String, DeparseError> {
        let name = statement.name.as_deref().unwrap_or("all");
        match statement.kind {
            VariableSetKind::ResetAll => Ok("RESET ALL".to_owned()),
            VariableSetKind::Reset => Ok(format!("RESET {}", quote_identifier(name))),
            VariableSetKind::SetDefault => Ok(format!(
                "SET {}{} TO DEFAULT",
                if statement.is_local { "LOCAL " } else { "" },
                quote_identifier(name)
            )),
            VariableSetKind::SetCurrent => Ok(format!(
                "SET {}{} FROM CURRENT",
                if statement.is_local { "LOCAL " } else { "" },
                quote_identifier(name)
            )),
            VariableSetKind::SetValue | VariableSetKind::SetMulti => Ok(format!(
                "SET {}{} TO {}",
                if statement.is_local { "LOCAL " } else { "" },
                quote_identifier(name),
                self.list(&statement.args, ", ")?
            )),
        }
    }

    fn variable_show(
        &self,
        statement: &VariableShowStmt,
    ) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "SHOW {}",
            statement
                .name
                .as_deref()
                .map(quote_identifier)
                .unwrap_or_else(|| "ALL".to_owned())
        ))
    }

    fn prepare(&self, statement: &PrepareStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = format!(
            "PREPARE {}",
            quote_identifier(
                statement
                    .name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("PrepareStmt", "name"))?
            )
        );
        if !statement.argtypes.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.list(&statement.argtypes, ", ")?);
            sql.push(')');
        }
        sql.push_str(" AS ");
        sql.push_str(&self.required_node(statement.query.as_deref(), "PrepareStmt", "query")?);
        Ok(sql)
    }

    fn execute(&self, statement: &ExecuteStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = format!(
            "EXECUTE {}",
            quote_identifier(
                statement
                    .name
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("ExecuteStmt", "name"))?
            )
        );
        if !statement.params.is_empty() {
            sql.push('(');
            sql.push_str(&self.list(&statement.params, ", ")?);
            sql.push(')');
        }
        Ok(sql)
    }

    fn deallocate(&self, statement: &DeallocateStmt) -> Result<std::string::String, DeparseError> {
        if statement.isall {
            Ok("DEALLOCATE ALL".to_owned())
        } else {
            Ok(format!(
                "DEALLOCATE {}",
                quote_identifier(
                    statement
                        .name
                        .as_deref()
                        .ok_or_else(|| DeparseError::missing("DeallocateStmt", "name"))?
                )
            ))
        }
    }

    fn notify(&self, statement: &NotifyStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = format!(
            "NOTIFY {}",
            quote_identifier(
                statement
                    .conditionname
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("NotifyStmt", "conditionname"))?
            )
        );
        if let Some(payload) = statement.payload.as_deref() {
            sql.push_str(", ");
            sql.push_str(&quote_literal(payload));
        }
        Ok(sql)
    }

    fn listen(&self, statement: &ListenStmt) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "LISTEN {}",
            quote_identifier(
                statement
                    .conditionname
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("ListenStmt", "conditionname"))?
            )
        ))
    }

    fn unlisten(&self, statement: &UnlistenStmt) -> Result<std::string::String, DeparseError> {
        Ok(match statement.conditionname.as_deref() {
            Some(name) => format!("UNLISTEN {}", quote_identifier(name)),
            None => "UNLISTEN *".to_owned(),
        })
    }

    fn transaction(
        &self,
        statement: &TransactionStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = match statement.kind {
            TransactionStmtKind::Begin => "BEGIN".to_owned(),
            TransactionStmtKind::Start => "START TRANSACTION".to_owned(),
            TransactionStmtKind::Commit => "COMMIT".to_owned(),
            TransactionStmtKind::Rollback => "ROLLBACK".to_owned(),
            TransactionStmtKind::Savepoint => format!(
                "SAVEPOINT {}",
                quote_identifier(statement.savepoint_name.as_deref().ok_or_else(|| {
                    DeparseError::missing("TransactionStmt", "savepoint_name")
                })?)
            ),
            TransactionStmtKind::Release => format!(
                "RELEASE SAVEPOINT {}",
                quote_identifier(statement.savepoint_name.as_deref().ok_or_else(|| {
                    DeparseError::missing("TransactionStmt", "savepoint_name")
                })?)
            ),
            TransactionStmtKind::RollbackTo => format!(
                "ROLLBACK TO SAVEPOINT {}",
                quote_identifier(statement.savepoint_name.as_deref().ok_or_else(|| {
                    DeparseError::missing("TransactionStmt", "savepoint_name")
                })?)
            ),
            TransactionStmtKind::Prepare => format!(
                "PREPARE TRANSACTION {}",
                quote_literal(
                    statement
                        .gid
                        .as_deref()
                        .ok_or_else(|| DeparseError::missing("TransactionStmt", "gid"))?
                )
            ),
            TransactionStmtKind::CommitPrepared => format!(
                "COMMIT PREPARED {}",
                quote_literal(
                    statement
                        .gid
                        .as_deref()
                        .ok_or_else(|| DeparseError::missing("TransactionStmt", "gid"))?
                )
            ),
            TransactionStmtKind::RollbackPrepared => format!(
                "ROLLBACK PREPARED {}",
                quote_literal(
                    statement
                        .gid
                        .as_deref()
                        .ok_or_else(|| DeparseError::missing("TransactionStmt", "gid"))?
                )
            ),
        };
        if !statement.options.is_empty() {
            sql.push(' ');
            sql.push_str(&self.transaction_options(&statement.options)?);
        }
        if statement.chain {
            sql.push_str(" AND CHAIN");
        }
        Ok(sql)
    }

    fn transaction_options(&self, options: &[Node]) -> Result<std::string::String, DeparseError> {
        options
            .iter()
            .map(|option| match option {
                Node::DefElem(element) => {
                    let name = element.defname.as_deref().unwrap_or_default();
                    let (value, enabled) = match element.arg.as_deref() {
                        Some(Node::String(value)) => (
                            value
                                .sval
                                .as_deref()
                                .unwrap_or_default()
                                .replace('_', " ")
                                .to_ascii_uppercase(),
                            false,
                        ),
                        Some(Node::AConst(AConst {
                            val: ValUnion::String(value),
                            ..
                        })) => (
                            value
                                .sval
                                .as_deref()
                                .unwrap_or_default()
                                .replace('_', " ")
                                .to_ascii_uppercase(),
                            false,
                        ),
                        Some(Node::AConst(AConst {
                            val: ValUnion::Integer(value),
                            ..
                        })) => (value.ival.to_string(), value.ival != 0),
                        Some(argument) => (self.render(argument)?, false),
                        None => (std::string::String::new(), false),
                    };
                    Ok(match name {
                        "transaction_isolation" => format!("ISOLATION LEVEL {value}"),
                        "transaction_read_only" => {
                            if enabled {
                                "READ ONLY".to_owned()
                            } else {
                                "READ WRITE".to_owned()
                            }
                        }
                        "transaction_deferrable" => {
                            if enabled {
                                "DEFERRABLE".to_owned()
                            } else {
                                "NOT DEFERRABLE".to_owned()
                            }
                        }
                        _ => format!("{} {value}", name.replace('_', " ").to_ascii_uppercase()),
                    })
                }
                _ => self.render(option),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|items| items.join(", "))
    }

    fn explain(&self, statement: &ExplainStmt) -> Result<std::string::String, DeparseError> {
        let mut sql = "EXPLAIN".to_owned();
        if !statement.options.is_empty() {
            sql.push_str(" (");
            sql.push_str(&self.list(&statement.options, ", ")?);
            sql.push(')');
        }
        sql.push(' ');
        sql.push_str(&self.required_node(statement.query.as_deref(), "ExplainStmt", "query")?);
        Ok(sql)
    }

    fn refresh_materialized_view(
        &self,
        statement: &RefreshMatViewStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "REFRESH MATERIALIZED VIEW ".to_owned();
        if statement.concurrent {
            sql.push_str("CONCURRENTLY ");
        }
        sql.push_str(
            &self.range_var(
                statement
                    .relation
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("RefreshMatViewStmt", "relation"))?,
            )?,
        );
        if statement.skip_data {
            sql.push_str(" WITH NO DATA");
        }
        Ok(sql)
    }

    fn comment(&self, statement: &CommentStmt) -> Result<std::string::String, DeparseError> {
        Ok(format!(
            "COMMENT ON {} {} IS {}",
            object_type_sql(statement.objtype),
            self.object_identity(
                statement
                    .object
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("CommentStmt", "object"))?
            )?,
            statement
                .comment
                .as_deref()
                .map(quote_literal)
                .unwrap_or_else(|| "NULL".to_owned())
        ))
    }

    fn security_label(
        &self,
        statement: &SecLabelStmt,
    ) -> Result<std::string::String, DeparseError> {
        let mut sql = "SECURITY LABEL".to_owned();
        if let Some(provider) = statement.provider.as_deref() {
            sql.push_str(" FOR ");
            sql.push_str(&quote_identifier(provider));
        }
        sql.push_str(" ON ");
        sql.push_str(object_type_sql(statement.objtype));
        sql.push(' ');
        sql.push_str(
            &self.object_identity(
                statement
                    .object
                    .as_deref()
                    .ok_or_else(|| DeparseError::missing("SecLabelStmt", "object"))?,
            )?,
        );
        sql.push_str(" IS ");
        sql.push_str(
            &statement
                .label
                .as_deref()
                .map(quote_literal)
                .unwrap_or_else(|| "NULL".to_owned()),
        );
        Ok(sql)
    }

    fn qualified_name_list(
        &self,
        nodes: &[Node],
        separator: &str,
    ) -> Result<std::string::String, DeparseError> {
        nodes
            .iter()
            .map(|node| match node {
                Node::String(value) => value
                    .sval
                    .as_deref()
                    .map(quote_identifier)
                    .ok_or_else(|| DeparseError::missing("String", "sval")),
                Node::AArrayExpr(array) => self.qualified_nodes(&array.elements),
                _ => Err(DeparseError::invalid("name list", "invalid name node")),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|names| names.join(separator))
    }
}

fn render_constant(value: &AConst) -> std::string::String {
    if value.isnull {
        return "NULL".to_owned();
    }
    match &value.val {
        ValUnion::Integer(value) => value.ival.to_string(),
        ValUnion::Float(value) => value.fval.clone().unwrap_or_else(|| "0".to_owned()),
        ValUnion::Boolean(value) => if value.boolval { "TRUE" } else { "FALSE" }.to_owned(),
        ValUnion::String(value) => quote_literal(value.sval.as_deref().unwrap_or_default()),
        ValUnion::BitString(value) => render_bit_string(value.bsval.as_deref().unwrap_or_default()),
    }
}

fn render_bit_string(value: &str) -> std::string::String {
    if let Some(bits) = value.strip_prefix('b') {
        format!("B'{}'", bits.replace('\'', "''"))
    } else if let Some(bits) = value.strip_prefix('x') {
        format!("X'{}'", bits.replace('\'', "''"))
    } else {
        format!("B'{}'", value.replace('\'', "''"))
    }
}

fn quote_literal(value: &str) -> std::string::String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_identifier(value: &str) -> std::string::String {
    let mut chars = value.chars();
    let bare = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_lowercase())
        && chars.all(|character| {
            character == '_'
                || character == '$'
                || character.is_ascii_lowercase()
                || character.is_ascii_digit()
        });
    let is_keyword = crate::KEYWORDS
        .binary_search_by_key(&value, |keyword| keyword.word)
        .is_ok();
    if bare && !is_keyword {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}

fn operator_name(nodes: &[Node]) -> Result<std::string::String, DeparseError> {
    let parts = nodes
        .iter()
        .map(|node| match node {
            Node::String(value) => value
                .sval
                .clone()
                .ok_or_else(|| DeparseError::missing("String", "sval")),
            _ => Err(DeparseError::invalid(
                "operator name",
                "parts must be String nodes",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [] => Err(DeparseError::missing("operator name", "parts")),
        [operator] => Ok(operator.clone()),
        [schema, operator] => Ok(format!(
            "OPERATOR({}.{})",
            quote_identifier(schema),
            operator
        )),
        _ => Err(DeparseError::invalid("operator name", "too many parts")),
    }
}

fn is_distinct_marker(nodes: &[Node]) -> bool {
    matches!(
        nodes,
        [Node::String(value)] if value.sval.as_deref() == Some("distinct")
    )
}

fn is_null_constant(node: &Node) -> bool {
    matches!(node, Node::AConst(value) if value.isnull)
}

fn object_type_sql(object_type: ObjectType) -> &'static str {
    match object_type {
        ObjectType::AccessMethod => "ACCESS METHOD",
        ObjectType::Aggregate => "AGGREGATE",
        ObjectType::Amop => "OPERATOR",
        ObjectType::Amproc => "FUNCTION",
        ObjectType::Attribute => "ATTRIBUTE",
        ObjectType::Cast => "CAST",
        ObjectType::Column => "COLUMN",
        ObjectType::Collation => "COLLATION",
        ObjectType::Conversion => "CONVERSION",
        ObjectType::Database => "DATABASE",
        ObjectType::Default => "OBJECT",
        ObjectType::Defacl => "DEFAULT PRIVILEGES",
        ObjectType::Domain => "DOMAIN",
        ObjectType::Domconstraint | ObjectType::Tabconstraint => "CONSTRAINT",
        ObjectType::EventTrigger => "EVENT TRIGGER",
        ObjectType::Extension => "EXTENSION",
        ObjectType::Fdw => "FOREIGN DATA WRAPPER",
        ObjectType::ForeignServer => "SERVER",
        ObjectType::ForeignTable => "FOREIGN TABLE",
        ObjectType::Function => "FUNCTION",
        ObjectType::Index => "INDEX",
        ObjectType::Language => "LANGUAGE",
        ObjectType::Largeobject => "LARGE OBJECT",
        ObjectType::Matview => "MATERIALIZED VIEW",
        ObjectType::Opclass => "OPERATOR CLASS",
        ObjectType::Operator => "OPERATOR",
        ObjectType::Opfamily => "OPERATOR FAMILY",
        ObjectType::ParameterAcl => "PARAMETER",
        ObjectType::Policy => "POLICY",
        ObjectType::Procedure => "PROCEDURE",
        ObjectType::Propgraph => "PROPERTY GRAPH",
        ObjectType::Publication => "PUBLICATION",
        ObjectType::PublicationNamespace => "SCHEMA",
        ObjectType::PublicationRel => "TABLE",
        ObjectType::Role => "ROLE",
        ObjectType::Routine => "ROUTINE",
        ObjectType::Rule => "RULE",
        ObjectType::Schema => "SCHEMA",
        ObjectType::Sequence => "SEQUENCE",
        ObjectType::Subscription => "SUBSCRIPTION",
        ObjectType::StatisticExt => "STATISTICS",
        ObjectType::Table => "TABLE",
        ObjectType::Tablespace => "TABLESPACE",
        ObjectType::Transform => "TRANSFORM",
        ObjectType::Trigger => "TRIGGER",
        ObjectType::Tsconfiguration => "TEXT SEARCH CONFIGURATION",
        ObjectType::Tsdictionary => "TEXT SEARCH DICTIONARY",
        ObjectType::Tsparser => "TEXT SEARCH PARSER",
        ObjectType::Tstemplate => "TEXT SEARCH TEMPLATE",
        ObjectType::Type => "TYPE",
        ObjectType::UserMapping => "USER MAPPING",
        ObjectType::View => "VIEW",
    }
}
