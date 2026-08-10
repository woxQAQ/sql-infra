# 原始表达式子节点使用 `Node`

PostgreSQL 原始语法节点会保存多态表达式子节点：一个 `Expr *` 字段可以指向 `AExpr`、`AConst`、`ColumnRef`、`FuncCall` 或其他原始语法节点。在 Rust 中，这些原始子节点字段使用 `Box<Node>`，而不是 `Box<Expr>`；只有当语法确定了具体子节点类型时才使用相应的强类型 Box。这样既能保持原始解析树的保真度，也不需要引入一个与 `Node` 平行的第二套表达式枚举。
