use pg_parser::{
    CmdType, ConstrType, FunctionParameterMode, Node, NodeTag, PartitionStrategy,
    PropGraphProperties, TableLikeOption, VariableSetKind, ViewCheckOption,
};

use super::common::{expect_node, parse_node};

#[path = "create/extension_fdw.rs"]
mod extension_fdw;
#[path = "create/index_view.rs"]
mod index_view;
#[path = "create/policy_trigger_graph.rs"]
mod policy_trigger_graph;
#[path = "create/routine.rs"]
mod routine;
#[path = "create/schema_role.rs"]
mod schema_role;
#[path = "create/table.rs"]
mod table;
#[path = "create/type_operator.rs"]
mod type_operator;
