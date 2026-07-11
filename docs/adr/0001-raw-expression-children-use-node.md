# Raw expression children use `Node`

Raw PostgreSQL grammar nodes store polymorphic expression children: an `Expr *` field can point to an `AExpr`, `AConst`, `ColumnRef`, `FuncCall`, or another raw syntax node. In Rust, those raw child fields use `Box<Node>` rather than `Box<Expr>`; typed boxes remain appropriate only where the grammar fixes the concrete child type. This preserves raw-tree fidelity without introducing a second expression enum parallel to `Node`.
