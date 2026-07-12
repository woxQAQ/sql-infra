# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust workspace whose single crate, `pg-parser`, is a hand-written recursive-descent parser that turns PostgreSQL SQL text into a **raw parse tree** — the syntactic tree straight from the tokens, before any name/type resolution. It intentionally mirrors PostgreSQL's own `gram.y`/`Node` structures rather than inventing an abstract AST. Edition 2024; no third-party dependencies.

## Commands

```bash
cargo build                                   # build the workspace
cargo test                                    # run all tests
cargo test -p pg-parser                       # test just the crate (only crate today)
cargo test --test statements                  # run the integration test suite
cargo test <substring>                        # run tests whose name matches
cargo test --test statements query::          # run one statements submodule (e.g. query)
cargo clippy --all-targets                    # lint
cargo fmt                                      # format
```

Tests are named as full sentences (e.g. `select_stmt_populates_query_clauses`), so filter by keyword.

## Pipeline

`parse(sql)` (in `src/parser.rs`) is the top entry point. Flow:

1. `lex()` (`src/lexer.rs`) → `Vec<Token>`, with `lookup_keyword()` classifying reserved words via `src/keywords.rs`.
2. `Parser { tokens, pos }` walks tokens; `Parser::parse()` splits on `;` into `Vec<RawStmt>`, recording `stmt_location`/`stmt_len` to match PostgreSQL semicolon semantics.
3. `parse_statement()` (`src/parser/statement.rs`) is the central dispatch: it matches the leading `TokenKind` and delegates to the right submodule.

Other entry points: `parse_one` (asserts exactly one statement), `parse_type_name`, `parse_plpgsql_assignment`, `parse_plpgsql_expression`.

## Code structure

- `src/ast.rs` (~4500 lines) — the node universe. Three things that must stay in lockstep: the `Node` enum, the `NodeTag` enum, and the `Node::tag()` mapping arms. Statement structs end in `Stmt`.
- `src/enums.rs`, `src/keywords.rs` — grammar enums and the keyword table.
- `src/parser.rs` — `Parser`, the `parse*` functions, `PResult`, and the `mod`/`use` declarations for ~80 parser submodules.
- `src/parser/*.rs` — one concern per file (`create_table.rs`, `alter_table.rs`, `expression*.rs`, `privileges.rs`, `dml.rs`, `query.rs`, `plpgsql.rs`, …). `expression` parsing is split across many `expression_*.rs` files by construct (json, xml, call, prefix, tail, sql).
- `src/parser/error.rs` — `ParseError` (byte offset + message); build errors with `self.error_here(...)`.

## Design invariants (read before touching the AST)

- **Raw expression children use `Box<Node>`, not `Box<Expr>`** (see `docs/adr/0001-raw-expression-children-use-node.md`). A grammar `Expr *` field is polymorphic (`AExpr`, `AConst`, `ColumnRef`, `FuncCall`, …), so it holds `Box<Node>`. Use a typed box only where the grammar fixes the concrete child type. Do not introduce a second expression enum parallel to `Node`.
- Raw-parse-tree fidelity is the goal. `ast.rs` also contains **analysis-tree** nodes (`Query`, `Var`, `Const`, `OpExpr`) that the text parser never produces — don't emit them from parser code. `CONTEXT.md` is the source of truth for this vocabulary (raw parse tree vs. analysis tree, syntax node, statement node).
- When adding a node: add the `Node` variant, the matching `NodeTag`, and the `Node::tag()` arm together. Coverage tests fail otherwise (see below).

## Tests

Integration tests live in `pg-parser/tests/`. `tests/statements.rs` wires up submodules via `#[path = "statements/<x>.rs"]`. Shared helpers are in `tests/statements/common.rs`: `parse_statement(sql)`, `parse_error(sql)`, `assert_statement_cases(&[StatementCase])`.

`tests/statements/coverage.rs` is a guardrail suite that reads the source text of `ast.rs` and the parser modules and asserts:
- `Node` variants, `NodeTag` variants, and `tag()` arms have not drifted from each other.
- Every `*Stmt` struct has a corresponding variant, tag, and mapping.

If you rename/add nodes and coverage tests fail, the fix is to sync the three lists — not to weaken the test.
