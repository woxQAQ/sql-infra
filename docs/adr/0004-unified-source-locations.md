# 统一源码位置模型

`ParseLoc` 保留 PostgreSQL 原始解析树的语义锚点；Token、错误、语句、补全结果以及新增 AST 节点范围统一使用 UTF-8 半开源码范围 `Loc`。`Position` 表示从零开始的行号和 Unicode 标量列号，由 `SourceText` 在 `Loc` 与行列坐标之间转换；公开 `Loc` 使用完整 SQL 的绝对坐标，语句内相对范围必须在返回前转换。字段、参数和变量按语义命名：`ParseLoc` 锚点使用 `parse_loc` 或 `*_parse_loc`，`Loc` 使用 `loc` 或 `*_loc`，UTF-8 字节位置使用 `offset` 或 `*_offset`，字节长度使用 `len` 或 `*_len`，`Position` 使用 `position` 或 `*_position`。该设计保留 PostgreSQL 字段语义，同时为诊断和编辑器调用方提供一致的范围与行列位置。
