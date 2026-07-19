use pg_parser::{
    AlterDomainType, AlterPropGraphElementKind, AlterSubscriptionType, AlterTableType,
    AlterTsConfigType, ConstrType, DefElem, DefElemAction, DropBehavior, Node, ObjectType,
    VariableSetKind,
};

use super::common::{expect_node, parse_node};

fn def(node: &Node) -> &DefElem {
    expect_node!(node, DefElem)
}

#[path = "alter/objects.rs"]
mod objects;
#[path = "alter/routines.rs"]
mod routines;
#[path = "alter/specialized.rs"]
mod specialized;
#[path = "alter/table.rs"]
mod table;
