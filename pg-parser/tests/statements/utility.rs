use pg_parser::{
    CURSOR_OPT_ASENSITIVE, CURSOR_OPT_BINARY, CURSOR_OPT_FAST_PLAN, CURSOR_OPT_HOLD,
    CURSOR_OPT_INSENSITIVE, CURSOR_OPT_NO_SCROLL, CURSOR_OPT_SCROLL, DefElem, DiscardMode,
    DropBehavior, FetchDirection, FetchDirectionKeywords, ImportForeignSchemaType, Node,
    ObjectType, ReindexObjectType, RepackCommand, TransactionStmtKind, ValUnion, VariableSetKind,
};

use super::common::{expect_node, parse_node};
fn def(node: &Node) -> &DefElem {
    expect_node!(node, DefElem)
}

#[path = "utility/comment_security.rs"]
mod comment_security;
#[path = "utility/copy_maintenance.rs"]
mod copy_maintenance;
#[path = "utility/cursor_prepare.rs"]
mod cursor_prepare;
#[path = "utility/misc.rs"]
mod misc;
#[path = "utility/session_transaction.rs"]
mod session_transaction;
