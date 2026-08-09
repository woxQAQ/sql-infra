# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust workspace with two crates. `pg-parser` is a hand-written recursive-descent parser that turns PostgreSQL SQL text into a **raw parse tree** — the syntactic tree straight from the tokens, before any name/type resolution. It intentionally mirrors PostgreSQL's own `gram.y`/`Node` structures rather than inventing an abstract AST. `pg-completion` builds on it: `collect(source, point)` returns a `CompletionContext` (grammar expectations, prefix, intent, scope) for editor completion, stopping short of catalog resolution — design in `docs/pg-completion-design.md`. Edition 2024; no third-party dependencies at runtime (`pg-completion` has serde/serde_yaml as dev-dependencies for its YAML fixtures).

## Commands

```bash
cargo build                                   # build the workspace
cargo test                                    # run all tests
cargo test -p pg-parser                       # test one crate (also: -p pg-completion)
cargo test --test statements                  # run the parser integration test suite
cargo test <substring>                        # run tests whose name matches
cargo test --test statements query::          # run one statements submodule (e.g. query)
cargo test -p pg-completion --test completion # run the completion YAML scenarios
PG_COMPLETION_RECORD=1 cargo test -p pg-completion --test completion  # re-record fixtures
cargo clippy --all-targets                    # lint
cargo fmt                                      # format
```

Tests are named as full sentences (e.g. `select_stmt_populates_query_clauses`), so filter by keyword.

## Pipeline

`parse(sql)` (in `pg-parser/src/parser.rs`) is the top entry point. Flow:

1. `lex()` (`src/lexer.rs`) → `Vec<Token>`, with `lookup_keyword()` classifying reserved words via `src/ast/keywords.rs`.
2. `Parser { tokens, pos }` walks tokens; `Parser::parse()` splits on `;` into `Vec<RawStmt>`, recording `stmt_location`/`stmt_len` to match PostgreSQL semicolon semantics.
3. `parse_statement()` (in `src/parser.rs`) is the central dispatch: it classifies the leading tokens into a `StatementFamily` and delegates to the right submodule.

Other entry points: `parse_one` (asserts exactly one statement), `parse_type_name`, `parse_plpgsql_assignment`, `parse_plpgsql_expression`, plus the completion seam used by `pg-completion`: `collect_expectations` (grammar candidates at a byte offset) and `lex_for_completion` (recovering tokenizer).

## Code structure

Inside `pg-parser/`:

- `src/ast/mod.rs` (~4500 lines) — the node universe. The `Node` enum variant is the single node-kind discriminator. Statement structs end in `Stmt`.
- `src/ast/enums.rs`, `src/ast/keywords.rs` — grammar enums and the keyword table.
- `src/parser.rs` — `Parser`, the `parse*` functions, `PResult`, `ParseError` (build errors with `self.error_here(...)`), and the `mod`/`use` declarations for ~80 parser submodules.
- `src/parser/*.rs` — one concern per file (`create_table.rs`, `alter_table.rs`, `expression*.rs`, `privileges.rs`, `dml.rs`, `query.rs`, `plpgsql.rs`, …). `expression` parsing is split across many `expression_*.rs` files by construct (json, xml, call, prefix, tail, sql).
- `src/parser/completion.rs` — the collect-mode collector and `GrammarSlot`; each grammar production publishes its completion candidates in place.

`pg-completion/src/` — `lib.rs` (public `CompletionContext` and `collect`), with `prefix.rs`, `intent.rs`, `scope.rs`, `statement.rs` as pure steps over pg-parser tokens and expectations.

## Design invariants (read before touching the AST)

- **Raw expression children use `Box<Node>`, not `Box<Expr>`** (see `docs/adr/0001-raw-expression-children-use-node.md`). A grammar `Expr *` field is polymorphic (`AExpr`, `AConst`, `ColumnRef`, `FuncCall`, …), so it holds `Box<Node>`. Use a typed box only where the grammar fixes the concrete child type. Do not introduce a second expression enum parallel to `Node`.
- Raw-parse-tree fidelity is the goal. `ast/mod.rs` also contains **analysis-tree** nodes (`Query`, `Var`, `Const`, `OpExpr`) that the text parser never produces — don't emit them from parser code. `CONTEXT.md` is the source of truth for this vocabulary (raw parse tree vs. analysis tree, syntax node, statement node).
- Parser code uses the private `node!(Type { ... })` and `node!(Type::constructor(...))` forms when a `Node` variant and its inline payload type have the same name. Existing payload values continue to use `Node::Variant(value)`.
- Repeated `Option<&Token>` kind and location queries use the private `TokenOptionExt` methods.
- When adding a node, add its `Node` variant and parser construction path together. Coverage tests fail when parser-produced statement structs lack a corresponding variant or test (see below).

## Tests

Integration tests live in `pg-parser/tests/`. `tests/statements.rs` wires up submodules via `#[path = "statements/<x>.rs"]`. Shared helpers are in `tests/statements/common.rs`: `parse_statement(sql)`, `parse_error(sql)`, `assert_statement_cases(&[StatementCase])`.

`tests/statements/coverage.rs` is a guardrail suite that reads the source text of `ast/mod.rs` and the parser modules and asserts:
- Every parser-produced `*Stmt` struct has a corresponding `Node` variant.
- Every parser-produced statement constructor has a smoke case or nested-node test.

If you rename or add nodes and coverage tests fail, update the AST struct, `Node` variant, parser constructor, and tests together.

`pg-completion` tests live in `pg-completion/tests/`: `completion.rs` runs declarative YAML scenarios from `test-data/completion/` (`|` marks the completion point; `PG_COMPLETION_RECORD=1` re-records candidates and refreshes any present `qualifier`/`container`/`scope` assertions), `context.rs` asserts the public context and scope rules programmatically, and `performance.rs` enforces the allocation/latency budget. New pg-parser grammar must also publish collect-mode expectations — collect coverage is a completion condition for parser changes, not an optional add-on (see `docs/pg-completion-design.md`).
