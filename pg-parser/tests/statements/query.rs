use pg_parser::{
    AExprKind, Alias, BoolExprType, BoolTestType, CteCycleClause, CteMaterialize, CteSearchClause,
    FRAMEOPTION_BETWEEN, FRAMEOPTION_EXCLUDE_TIES, FRAMEOPTION_ROWS,
    FRAMEOPTION_START_OFFSET_PRECEDING, GraphElementPatternKind, GroupingSetKind, JoinType,
    JsonBehavior, JsonBehaviorType, JsonEncoding, JsonExprOp, JsonFormatType, JsonQuotes,
    JsonReturning, JsonTableColumnType, JsonTablePathSpec, JsonValueType, JsonWrapper,
    LockClauseStrength, LockWaitPolicy, MinMaxOp, Node, NullTestType, SetOperation, ValUnion,
    WithClause, XmlExprOp,
};

use super::common::{expect_node, parse_node};

fn set_shape(stmt: &pg_parser::SelectStmt) -> String {
    if let (Some(left), Some(right)) = (&stmt.larg, &stmt.rarg) {
        format!("{:?}({},{})", stmt.op, set_shape(left), set_shape(right))
    } else {
        "leaf".to_owned()
    }
}

#[path = "query/core.rs"]
mod core;
#[path = "query/cte.rs"]
mod cte;
#[path = "query/expressions.rs"]
mod expressions;
#[path = "query/graph.rs"]
mod graph;
#[path = "query/json.rs"]
mod json;
#[path = "query/ranges_joins.rs"]
mod ranges_joins;
#[path = "query/xml.rs"]
mod xml;
