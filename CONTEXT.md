# SQL 解析

本上下文定义 `pg-parser` 表示的树，以及 SQL 文本解析器负责产生哪些节点。

## 术语

**原始解析树（Raw parse tree）**:
直接从 SQL Token 产生、尚未经过名称解析、类型解析或语义转换的语法树。其表达式节点包括 `AExpr`、`AConst`、`ColumnRef` 等语法级节点。
_避免使用_: 已解析分析树、语义 AST

**分析树（Analysis tree）**:
解析后经过语义转换得到的树；`Query`、`Var`、`Const`、`OpExpr` 等节点属于分析树，不是 SQL 文本解析器的输出。
_避免使用_: 原始 AST、解析器输出

**语法节点（Syntax node）**:
按照 PostgreSQL 语法能够从原始解析树到达的 `NodeTag`。并非每个 `NodeTag` 都是语法节点，因为 `ast.rs` 还包含分析树节点。
_避免使用_: 任意 `NodeTag`

**语句节点（Statement node）**:
Rust 类型名以 `Stmt` 结尾的原始解析树语法节点；它既可能是顶层语句，也可能是 `ReplicaIdentityStmt` 之类由语法产生的嵌套语句。
_避免使用_: 仅指顶层语句

**源码偏移量（Source offset）**:
SQL 源文本中从零开始的 UTF-8 字节偏移量。它不同于字符下标，也不同于界面显示的行号和列号。
_避免使用_: 字符偏移量、列号

**文本范围（Text range）**:
由两个源码偏移量表示的半开区间 `[start, end)`。它描述源码覆盖范围，而不是原始节点 `location` 字段中保存的 PostgreSQL 语义锚点。
_避免使用_: Location、闭区间

**位置（Location）**:
附着在部分原始解析树节点上的 PostgreSQL 兼容语义锚点，通常是引入该节点的语法 Token 的源码偏移量。位置不是节点的完整文本范围。
_避免使用_: Span、节点范围

**补全点（Completion point）**:
发起补全请求的源码偏移量；该偏移量已归一化到 SQL 源码中的 UTF-8 字符边界。
_避免使用_: 光标位置、Token 下标

**替换范围（Replacement range）**:
接受补全候选后应被替换的文本范围，通常是以补全点结尾的标识符片段。
_避免使用_: 前缀范围、光标范围

**语法期望（Grammar expectation）**:
在补全点处语法上合法的 Token 或具名语法槽位。
_避免使用_: 建议、Catalog 候选

**语法候选（Grammar candidate）**:
语法期望集合中的一个 Token 或具名语法槽位；它不包含从 Catalog 解析出的对象，也不包含编辑器展示信息。
_避免使用_: 补全项、Catalog 候选

**补全项（Completion item）**:
adapter 将语法候选、作用域和 Catalog 元数据解析并渲染后产生的最终建议，包含插入文本及可选的展示信息。
_避免使用_: 语法候选、语法期望

**补全意图（Completion intent）**:
由补全点处的语法期望推导出的 SQL 对象类别和部分限定名。
_避免使用_: 补全上下文、候选类型

**可见关系（Visible relation）**:
其列可能在补全点处可见的语法级关系引用，包括基础关系、CTE、子查询、JOIN 和表函数。
_避免使用_: Catalog 表、RangeVar

**查询作用域（Query scope）**:
属于同一查询层级的一组有序可见关系。本地查询作用域与每一层相关外部查询作用域彼此独立。
_避免使用_: 语句中的所有表、扁平引用列表
