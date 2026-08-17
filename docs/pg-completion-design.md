# PostgreSQL 补全设计

## 设计结论

`pg-completion` 对外只提供一个深而稳定、与具体产品无关的 interface：

```rust
pub fn collect(source: &str, point: TextSize) -> CompletionContext;
```

它接收 SQL 和一个 UTF-8 源码偏移量，返回元数据 adapter 生成候选所需的全部信息。它不要求 SQL 合法，不返回残缺的原始解析树，不访问 Catalog，也不会因为编辑器提供的偏移量而 panic。

首个版本止于 `CompletionContext`。候选渲染、元数据 I/O、权限、排序、代码片段和 UTF-16/LSP 转换都属于建立在该 interface 之上的 adapter。只有当出现两个真实 adapter、足以证明共享 interface 已经稳定时，才引入与产品无关的 Catalog resolver。

## 公开模型

```rust
pub struct CompletionContext {
    /// 包含补全点的语句范围，不含语句终止符。
    pub statement_loc: Loc,
    /// 对齐到 UTF-8 字符边界后的有效补全点。
    pub point: TextSize,
    /// 接受候选时需要替换的文本范围。
    pub replacement_loc: Loc,
    /// 补全点处标识符片段的原始形式和归一化形式。
    pub prefix: CompletionPrefix,
    /// 语法上合法的终结符和具名语法槽位。
    pub expectations: ExpectationSet,
    /// 由语法期望推导出的对象类别和限定信息。
    pub intent: CompletionIntent,
    /// 补全点处语法上可见的名称。
    pub scope: ScopeSnapshot,
    /// 供调用方判断结果完整性并进行遥测的补全诊断。
    pub diagnostics: Vec<CompletionDiagnostic>,
}

pub struct CompletionPrefix {
    pub raw: String,
    pub normalized: String,
    pub quoting: IdentifierQuoting,
}

pub struct CompletionDiagnostic {
    pub kind: CompletionDiagnosticKind,
    pub loc: Loc,
}

pub enum CompletionDiagnosticKind {
    PointClampedToEof,
    PointMovedToCharBoundary,
    TokenizationRecovered,
    LexErrorBeforePoint,
    ScopeIncomplete,
}

pub struct ExpectationSet {
    pub tokens: Vec<TokenKind>,
    /// 由当前语法产生式直接引入的 Token；始终是 `tokens` 的子集。
    pub direct_tokens: Vec<TokenKind>,
    /// parser lookahead 观察到的合法关键字分支；无前缀时默认静默。
    pub lookahead_tokens: Vec<TokenKind>,
    /// 可以开始当前表达式的 Token；始终是 `tokens` 的子集。
    pub expression_start_tokens: Vec<TokenKind>,
    /// 可以延续已经解析完成的表达式的 Token；始终是 `tokens` 的子集。
    pub expression_continuation_tokens: Vec<TokenKind>,
    /// 结束当前表达式并进入外层产生式的 Token；始终是 `tokens` 的子集。
    pub follow_tokens: Vec<TokenKind>,
    /// 该位置语法上合法的固定多词单元，如 `GROUP BY`、`IF NOT EXISTS`。
    /// 短语首 Token 一定同时出现在 `tokens` 中；短语不声称首 Token 没有
    /// 其他延续方式。
    pub phrases: Vec<&'static [TokenKind]>,
    pub slots: Vec<GrammarSlot>,
}

pub enum GrammarSlot {
    Relation,
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Column,
    Attribute,
    Function,
    Procedure,
    Routine,
    Aggregate,
    Type,
    Domain,
    Schema,
    Sequence,
    Index,
    Constraint,
    Collation,
    Operator,
    OperatorClass,
    OperatorFamily,
    Role,
    Database,
    AccessMethod,
    Conversion,
    EventTrigger,
    Extension,
    ForeignDataWrapper,
    ForeignServer,
    Language,
    Policy,
    PropertyGraph,
    Publication,
    Rule,
    Statistics,
    Subscription,
    Tablespace,
    TextSearchConfiguration,
    TextSearchDictionary,
    TextSearchParser,
    TextSearchTemplate,
    Trigger,
    AnyName,
}

pub struct CompletionIntent {
    pub object_kinds: Vec<ObjectKind>,
    /// 当前补全片段之前已经完成的点分名称。
    pub qualifier: Vec<NamePart>,
    /// 部分候选是某个独立出现的 Catalog 对象的成员时，记录该关系。
    pub membership: Option<CatalogMembership>,
}

pub struct CatalogMembership {
    /// object_kinds 中受该成员关系约束的类别。
    pub member_kinds: Vec<ObjectKind>,
    pub owner: ObjectReference,
}

pub struct ObjectReference {
    /// 同一个语法 identity 可能对应的 Catalog 对象类别。
    pub object_kinds: Vec<ObjectKind>,
    pub name: Vec<NamePart>,
}

pub struct NamePart {
    pub text: String,
    pub normalized: String,
    pub quoted: bool,
    pub loc: Loc,
}
```

所有公开 `Loc` 均使用完整 SQL 的绝对 UTF-8 字节偏移量。`pg-completion` 在活动语句切片上调用 parser 时使用语句内相对偏移量，并在构造 `CompletionContext` 前加回语句起点。调用方需要行列位置时，通过 `pg_parser::SourceText` 将 `Loc` 转换为 `PositionRange`；核心列号按 Unicode 标量计数，UTF-16 等协议坐标由 adapter 转换。

这些模型按以下顺序协作：

1. `statement_loc` 和 `point` 确定本次补全处理哪条语句、在哪个 UTF-8 字节偏移量收集候选。
2. `replacement_loc` 和 `prefix` 描述当前正在输入的标识符片段。`raw` 用于保留用户输入，`normalized` 用于匹配，`quoting` 决定后续插入时采用哪种引用规则。
3. `expectations` 记录该位置语法上允许的 Token、固定短语和 `GrammarSlot`，不包含任何 Catalog 对象。Token 是单个终结符；`follow_tokens` 标出会结束当前表达式、进入外层产生式的 Token；短语把 `GROUP BY` 这类固定多词单元整体交给 adapter。adapter 因而无需扫描 SQL：当完整表达式后尚未输入下一个 token 时保持安静，用户开始输入前缀后再发布匹配的表达式延续或 clause transition。
4. `intent` 将 `GrammarSlot` 转成 adapter 可查询的 `ObjectKind`，通过 `qualifier` 保留当前片段之前的限定名，并在成员候选中通过 `membership` 指明 owner 对象。
5. `scope` 记录当前语法位置可见的关系、CTE、DML 目标和外部查询层级，供 adapter 解析列候选。
6. `diagnostics` 记录位置修正、词法恢复或作用域不完整等问题；它不会清除已经得到的其他信息。

例如，对 `SELECT u.na| FROM users AS u` 调用 `collect` 时，`|` 只表示补全点，不属于输入：

```text
statement_loc   = [0, 27)
point             = 11
replacement_loc = [9, 11)
prefix            = { raw: "na", normalized: "na", quoting: Unquoted }
expectations      = { tokens: [], slots: [Column, Function] }
intent            = { object_kinds: [Column, Function], qualifier: [u], membership: None }
scope.local       = [users AS u]
diagnostics       = []
```

`pg-completion` 到此停止。adapter 再使用 `qualifier = [u]` 在 `scope` 中定位 `users`，查询该关系的列，并用 `prefix = "na"` 过滤结果。`Function` 仍被保留，因为同一个限定符也可能是 schema；只有 Catalog adapter 能消除关系别名与 schema 的命名冲突。Catalog 中是否真的存在 `users`、同名 schema 或匹配的候选，不影响上述上下文的构造。

Catalog 成员关系和限定名表达不同的语法关系。例如 `ALTER TABLE app.users DROP COLUMN na|` 的 qualifier 为空、prefix 是 `na`，而 intent 的 membership 是 `{ member_kinds: [Column], owner: { object_kinds: [Table], name: [app, users] } }`。adapter 因而可以直接查询该表的列，不需要重新解析 `ALTER TABLE`。`COPY table (|)`、外键 `REFERENCES table (|)`、trigger 的 `UPDATE OF | ON table` 以及 type attribute 使用同一个模型；关系语法不能静态缩小到单一 Catalog 类别时，`ObjectReference.object_kinds` 保留所有合法类别。`member_kinds` 只约束对应候选，因此同一位置同时允许 `Column` 和 `Function` 时，函数仍按 search path 查询。

`GrammarSlot` 描述语法，由 `pg-parser` 产生；`ObjectKind` 描述 Catalog 意图，由 `pg-completion` 拥有。两者不能合并成一个枚举：`AnyName` 只表示语法接受一个没有稳定 Catalog 类别的名称，`Privilege` 表示可由 adapter 提供标准或扩展权限名，两者都不产生 `ObjectKind`；Catalog 名称使用具体 slot。查询 `FROM` 中的 `Relation` 保留开放的关系类别，DDL 产生式则发布 `Table`、`View`、`MaterializedView` 或 `ForeignTable` 等具体 slot，因此 `pg-completion` 不需要扫描整条语句猜测对象类别。

由多个 Catalog 对象共同组成的 identity 不增加虚假的单一 slot。例如 transform identity 分别发布 `Type` 和 `Language`，user mapping identity 分别发布 `Role` 和 `ForeignServer`；adapter 按当前产生式所在的组成位置查询对应对象。

`qualifier` 不包含正在输入的片段；当前片段只由 `CompletionPrefix` 表达。例如 `a.b|` 的 qualifier 是 `[a]`、prefix 是 `b`，`a.b.|` 的 qualifier 是 `[a, b]`、prefix 为空。限定名的具体语义取决于语法槽位和 adapter 元数据。

## 作用域模型

```rust
pub struct ScopeSnapshot {
    pub local: QueryScope,
    /// 相关外部作用域，最近的一层排在最前面。
    pub outer: Vec<QueryScope>,
    /// 当前语法位置可见的 CTE；内层查询优先，同名外层定义被遮蔽。
    pub ctes: Vec<CteDefinition>,
    /// 当前语法位置可见的 DML 目标；构造 INSERT、MERGE、UPDATE FROM 或 DELETE USING 来源时为空。
    pub dml_target: Option<VisibleRelation>,
    /// 当前语法位置可见的 MERGE 来源；解析来源自身时为空。
    pub merge_source: Option<VisibleRelation>,
}

pub struct QueryScope {
    /// SQL 可见性顺序，而不是字母顺序或 Catalog 顺序。
    pub relations: Vec<VisibleRelation>,
}

pub struct VisibleRelation {
    pub kind: RelationKind,
    pub name: Vec<NamePart>,
    pub alias: Option<NamePart>,
    pub explicit_columns: Vec<NamePart>,
    /// 仅能通过 alias 限定访问，不参与未限定列名查找。
    pub qualified_only: bool,
    pub syntax_loc: Loc,
    pub body_loc: Option<Loc>,
    pub lateral: bool,
    pub unsupported: Option<UnsupportedRelation>,
}

pub enum RelationKind {
    Relation,
    Cte,
    Subquery,
    TableFunction,
    JoinAlias,
    Values,
}
```

作用域收集器遵循 PostgreSQL 可见性规则，而不是收集语句中的所有 `RangeVar`：

- 最内层查询层级是 `local`。
- 相关外部层级分别保留，并按由近到远排序。
- 非 `LATERAL` 派生表看不到所属 `FROM` 列表中排在它之前的关系；`LATERAL` 派生表只能看到之前的条目。
- CTE body 中使用的源关系不能泄漏到使用该 CTE 的查询作用域。
- 集合运算的每个分支只看到自己的 `FROM` 作用域，看不到兄弟分支。
- 显式 CTE/子查询别名列、表函数列定义以及 `ROWS FROM` 各函数的列定义属于语法可知的输出列；推导出的输出列仍由 adapter 负责，除非后续 analysis 模块能够提供。
- DML 目标和来源只在 PostgreSQL 允许引用它们的位置出现；INSERT 来源、MERGE 来源自身和非 `LATERAL` 的 DML `FROM` 派生表不会继承目标关系。
- `ON CONFLICT DO UPDATE` 的 `excluded` 以及 `RETURNING` 的 `old`/`new`（含显式别名）作为 `qualified_only` 关系映射回 DML 目标；adapter 只在限定引用中查询它们，避免重复未限定列候选。
- 不支持的表表达式必须显式标记，不能为其虚构列。

## 内部接口

### 1. `pg-parser`：收集强类型语法期望

`pg-parser` 增加内部补全模式，并通过一个很窄的函数暴露：

```rust
pub fn collect_expectations(
    source: &str,
    point: TextSize,
) -> Result<ParserExpectations, LexError>;

pub struct ParserExpectations {
    pub tokens: Vec<TokenKind>,
    pub direct_tokens: Vec<TokenKind>,
    pub lookahead_tokens: Vec<TokenKind>,
    pub expression_start_tokens: Vec<TokenKind>,
    pub expression_continuation_tokens: Vec<TokenKind>,
    pub follow_tokens: Vec<TokenKind>,
    pub phrases: Vec<&'static [TokenKind]>,
    pub slots: Vec<GrammarSlot>,
}
```

现有的 `parse`、`parse_one` 和 `parse_with_locs` interface 保持严格且不变。

收集模式只解析到位于替换范围起点的合成补全标记。必选匹配自动发布相应 Token；语法分支和具有语义的名称位置显式发布候选：

```rust
collector.tokens([TokenKind::Where, TokenKind::GroupP, TokenKind::Order]);
collector.phrase(&[TokenKind::GroupP, TokenKind::By]);
collector.slot(GrammarSlot::Column);
```

短语由拥有该产生式的位置发布。`consume(A)` 后强制 `expect(B)` 的形态统一改写为 `consume_phrase(&[A, B])`，使发布点无法与解析行为漂移；批量候选列表和表达式尾部的子句跟随 Token 由固定的 follow 映射补齐对应短语。

收集过程通过强类型控制流退出，而不是 panic 或伪造 `ParseError`。当前解析器采用预测式解析且没有回溯，因此不需要回滚候选；如果未来引入回溯，解析器 checkpoint 必须同时保存 collector 状态。

#### Collect 完整性不变量

所有能够由 `pg-parser` 顶层语句分派入口解析的语句，最终都必须覆盖 collect 模式。`ExpectationSet` 不编码实现进度或恢复状态：有合法语法候选时返回相应 Token 和 slot；当前位置没有后续候选时返回空集合；位置修正、词法恢复和作用域不完整只记录在 `CompletionContext::diagnostics`。

阶段性交付可以按语句族逐步增加 instrumentation 和场景，但不引入 `Unsupported` 等运行时状态。遗漏由声明式候选基线和语句族覆盖门禁发现，而不是要求 adapter 解释临时状态。覆盖分为两个层级：

- **结构覆盖**：每个语句族都覆盖语句起始、主要子命令和可选子句边界，并发布该位置全部合法的关键字或空集合。这是所有语句的强制要求。
- **对象覆盖**：所有引用 Catalog 名称的位置都发布正确的 `GrammarSlot`，并由 `pg-completion` 收窄为 `CompletionIntent`。只有具有关系可见性的语句才需要构造非空查询作用域。

新增或扩展语法产生式时，parser 变更必须同时添加 collect instrumentation 和对应场景；collect 是语法实现的完成条件，而不是后续可选增强。

### 2. `pg-completion`：前缀、意图和作用域

该 crate 执行四个纯步骤：

1. 归一化编辑器偏移量，并隔离包含补全点的语句。
2. 提取标识符前缀和替换范围，同时保留引用方式。
3. 在替换范围起点向 `pg-parser` 请求强类型语法期望。
4. 将 `GrammarSlot` 和 `GrammarMembership` 投影为补全意图，并扫描当前语句的 Token 构造作用域。

这次作用域扫描不是第二套 SQL 解析器。它只识别关系可见性所需的结构：语句边界、括号、查询层级、`WITH`、集合运算分支、`FROM`/`JOIN`、别名、`LATERAL`、子查询、表函数、`VALUES` 和 DML 目标。Catalog 成员关系直接来自 parser，不由这次扫描猜测。

### 3. 调用方 adapter：解析元数据

adapter 接收 `CompletionContext`，结合自己的元数据源进行解析：

```text
Token 期望           -> adapter 筛选后的关键字候选
Unprefixed expr tail -> 不发布编辑器候选，保留 raw context
Expression follow    -> 输入前缀后可匹配的外层 clause transition
短语期望             -> GROUP BY、IF NOT EXISTS 等整体补全项
Relation 意图        -> Schema/关系候选
Column 意图 + 作用域 -> 可见关系中的列
Column/Attribute + membership -> 指定表、domain 或 type 中的成员
Function 意图        -> search_path 上可见的函数
Type 意图            -> search_path 上可见的类型
qualifier + prefix   -> 限定名逐级补全
```

关系列限定符遵循 PostgreSQL 的可见名称规则：没有 alias 的 schema-qualified 关系既可通过关系名（`users.`）也可通过完整名（`public.users.`）引用；一旦声明 `AS u`，只接受 `u.`，原关系名不再可见。

adapter 拥有 search path 策略、Catalog I/O、权限、插入文本引用、代码片段、定义、注释、排序、取消、缓存和输出位置编码。

## 容错契约

补全采用尽力而为但确定性的行为：

- 补全点超过源码末尾时，将其限制到 EOF。
- 补全点位于 UTF-8 码点内部时，移动到该码点起点。
- 只有包含补全点的语句参与处理；更早的错误语句不能阻塞当前补全。
- 包含补全点的 Token 成为前缀，并从语法收集中排除。
- 与补全点相交且未闭合的带引号标识符、字符串、dollar quote 和注释转成不完整 Token，而不是致命词法错误。
- 活动语句中严格位于补全点之前的词法错误产生空语法期望和恢复原因，但绝不 panic。
- 作用域推导失败不能丢弃已经收集到的语法期望。
- 标记为不支持的关系形态不能贡献猜测出的列。

这要求增加一个面向补全的 lexer 入口，返回恢复后的 Token 和问题列表。严格的 `lex` interface 保持不变。

## 模块布局

```text
pg-parser/src/
  lexer.rs                  严格 lex + 面向补全的恢复式 Token 化
  parser.rs                 collect 入口、collector 状态和顶层语句分派
  parser/completion.rs      collector 类型与强类型控制流
  parser/*.rs               各语法产生式在原位置发布 Token 和 GrammarSlot

pg-parser/tests/
  statements/coverage.rs    合法语句 Token 边界采样
  statements/*.rs           各语句族的解析回归语料

pg-completion/
  Cargo.toml
  src/lib.rs                公开 interface 和 re-export
  src/prefix.rs             补全点归一化和替换范围
  src/intent.rs             GrammarSlot、GrammarMembership 和限定名 -> CompletionIntent
  src/scope.rs              查询层级和可见关系
  src/statement.rs          活动语句隔离
  tests/completion.rs       声明式场景测试 runner
  tests/context.rs          公开上下文与作用域断言
  tests/performance.rs      大输入分配与延迟门禁
  test-data/completion/     按语句族组织的 YAML 场景
```

这里的 `parser/completion.rs` 只承载 collector 基础设施，不集中实现 PostgreSQL 补全语法。实现 collect 模式需要横向修改现有的 query、expression、name、DML、DDL 和 utility 等 parser 模块；每个产生式仍在定义它的文件中发布候选。内部模块保持私有。声明式测试统一通过 `pg_completion::collect` 验证公开行为；解析器测试则单独验证各语法产生式是否发布了正确的 `GrammarSlot` 和 Token 期望。

## 交付阶段

### 阶段 1：SELECT 基础

- 空输入和语句起始关键字。
- 前缀与替换范围，包括带引号标识符和 UTF-8。
- `SELECT`、`FROM`、JOIN、别名、`WHERE`、`GROUP BY`、`HAVING` 和 `ORDER BY`。
- Schema、表、函数和列语法槽位。
- 本地关系作用域。

### 阶段 2：嵌套作用域

- CTE 及显式 CTE 列。
- 子查询别名和显式输出别名。
- 相关外部作用域。
- `LATERAL`、表函数、JOIN 别名和 `VALUES`。
- 集合运算分支隔离。

### 阶段 3：DML

- INSERT 目标和目标列。
- UPDATE/DELETE 目标以及 `FROM`/`USING` 作用域。
- MERGE 目标、来源和 action 子句。
- `RETURNING` 表达式作用域。

### 阶段 4：DDL、Utility 与 PostgreSQL 对象

- CREATE/ALTER/DROP 对象意图。
- 类型、attribute、Schema、序列、索引、约束、排序规则、操作符、操作符类、角色和数据库。
- `ALTER TABLE`、`DROP VIEW`、`COMMENT ON COLUMN` 等语句中的对象类别收窄。
- SET/SHOW、事务、PREPARE、游标、COPY、VACUUM、EXPLAIN、CALL 以及其他已由顶层入口支持的 utility/admin 语句。
- 所有语句族完成结构覆盖；涉及 Catalog 名称的位置完成对象覆盖。

### 阶段 5：强化

- 多语句隔离和错误输入恢复。
- 带引号、保留字和大小写敏感名称。
- 大输入下的分配与延迟预算。
- 启用全语句 collect 覆盖门禁。
- 对 PostgreSQL 接受行为和既有解析器回归语料进行差分场景验证。

## 验收场景

补全行为测试采用声明式 YAML 夹具：`input` 使用唯一的 `|` 标记补全点，runner 删除标记、计算 UTF-8 偏移量、调用 `pg_completion::collect`，再对结果进行规范化比较。夹具按语句族拆分，避免一个文件随全语法覆盖无限增长。

`want.candidates` 断言的是无 Catalog 的语法候选，而不是最终补全项：Token 候选对应关键字、操作符和标点（如 `(`、`*`、`::`），phrase 候选以规范拼写（如 `GROUP BY`、`IF NOT EXISTS`）断言固定多词单元，slot 候选对应 `GrammarSlot`。标点和操作符是语法期望的组成部分，但默认 adapter 不把它们转换为编辑器候选；完整表达式后的空白位置也不发布语法候选，直到用户输入下一个 token 的前缀。candidates 不包含 Catalog 对象名称、插入文本、定义、注释、优先级或排序分数。`phrases` 键缺省断言为空。

```yaml
- input: SEL|
  want:
    candidates:
      tokens: [SELECT]
      slots: []

- input: SELECT u.| FROM users AS u
  want:
    candidates:
      tokens: ['*']
      slots: [Column, Function]
    qualifier: [u]
    scope:
      local: [users AS u]
```

每个用例必须包含 `input` 和 `want.candidates`。`candidates` 执行精确集合比较，空的 `tokens` 和 `slots` 表示该位置没有语法候选。runner 在比较前按稳定规则排序、去重，因此 YAML 不依赖 collector 的遍历顺序。

断言意图和作用域时，用例附加可选字段：`qualifier` 断言当前片段之前的限定名，`membership` 断言 Catalog 成员关系的规范渲染，`scope` 断言整个作用域快照。`scope` 一旦出现即覆盖全部子字段，省略的子字段断言为空；关系按 SQL 可见性顺序比较，不排序。这些字段缺省时不参与断言，候选覆盖场景因此保持精简，作用域和成员关系由带对应字段的场景显式钉住。

最低场景集覆盖：

- 空输入、关键字前缀、替换范围、带引号标识符和 UTF-8。
- SELECT、JOIN、CTE、子查询、相关作用域、`LATERAL` 和集合运算分支。
- INSERT、UPDATE、DELETE、MERGE 及 `RETURNING`。
- DDL 对象槽位以及全部 utility/admin 语句的结构候选。
- 多语句隔离、残缺 Token、恢复结果和合法的空候选。

除此之外，覆盖门禁维护一份与顶层语句分派入口对齐的语句族清单，并复用 `pg-parser/tests/statements/` 中的合法 SQL 作为样本：

- 每个语句族至少有语句起始、主要子命令和语句末尾场景。
- 在样本的每个 Token 起止边界调用 `collect`，验证不 panic 且至少发布一个 Token 或 slot；完整语句 EOF 可以为空，但不能残留 Catalog 对象 slot。
- 新增顶层分派分支却没有登记相应语句族和场景时，CI 失败。

Token 边界采样只能证明没有遗漏和崩溃，不能证明候选正确；候选正确性由 YAML 场景精确断言，parser 产生式级单元测试作为更近故障源的补充。

性能门禁使用包含 5,000 个可见关系的前向作用域场景；计时前先预热，并将源码构造排除在统计外。debug 测试中单次 `collect` 的预算为 1 秒、150,000 次分配和 16 MiB 分配字节，三项任一超限都使 CI 失败。

该模块的删除测试很直接：如果没有 `CompletionContext`，每个元数据 adapter 都必须重新实现前缀处理、错误输入恢复、语法期望推导、CTE 解析和嵌套可见性。将这些决策隐藏在 `collect` 后面，才能获得预期的 leverage 和 locality。
