# SQL Parsing

This context defines the trees represented by `pg-parser` and which nodes a SQL text parser is responsible for producing.

## Language

**Raw parse tree**:
The syntactic tree produced directly from SQL tokens, before name resolution, type resolution, or semantic transformation. Its expression nodes include `AExpr`, `AConst`, `ColumnRef`, and other grammar-level nodes.
_Avoid_: Parsed analysis tree, semantic AST

**Analysis tree**:
The semantically transformed tree produced after parsing; nodes such as `Query`, `Var`, `Const`, and `OpExpr` belong here and are not outputs of the SQL text parser.
_Avoid_: Raw AST, parser output

**Syntax node**:
A `NodeTag` that is reachable in a raw parse tree according to the PostgreSQL grammar. Not every `NodeTag` is a syntax node because `ast.rs` also contains analysis-tree nodes.
_Avoid_: Any NodeTag

**Statement node**:
A raw-parse-tree syntax node whose Rust type ends in `Stmt`; it may be a top-level statement or a grammar-produced nested statement such as `ReplicaIdentityStmt`.
_Avoid_: Top-level statement only

**Source offset**:
A zero-based UTF-8 byte offset into SQL source text. It is distinct from a character index or a displayed line and column.
_Avoid_: Character offset, column

**Text range**:
A half-open source interval `[start, end)` expressed by two source offsets. It describes source coverage rather than the PostgreSQL semantic anchor stored in a raw node's `location` field.
_Avoid_: Location, inclusive range

**Location**:
The PostgreSQL-compatible semantic anchor attached to some raw parse tree nodes, usually the source offset of the grammar token that introduced the node. A location is not the node's complete text range.
_Avoid_: Span, node range
