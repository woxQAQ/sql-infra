# pg-completion

`pg-completion` turns PostgreSQL syntax context plus optional catalog metadata
into editor-neutral completion items.

```rust
use pg_completion::{complete, CompletionRequest};
use pg_parser::TextSize;

let sql = "SELECT na FROM users u";
let result = complete(
    CompletionRequest {
        sql,
        cursor: TextSize::new(9),
        search_path: &["public", "pg_catalog"],
    },
    Some(&catalog),
)?;
```

Implement `Catalog` for a live PostgreSQL metadata cache, or use
`MemoryCatalog` for embedding and tests. The crate does not depend on LSP;
clients map `CompletionItem` and `replacement` to their protocol of choice.

## Tests

Public completion behavior is covered by integration tests under
`tests/completion/`. Scenarios use a single `|` marker for the cursor and are
grouped by responsibility:

- `syntax`, `boundaries`: parser expectations, replacement ranges, lexical
  suppression, cursor boundaries, and result invariants;
- `expression_forms`, `expression_slots`: every supported expression family
  and the query, DML, DDL, and utility positions that host expressions;
- `relations`, `columns`, `routines_types`: catalog-backed name resolution;
- `ranking`: ordering, search path, prefix filtering, and deduplication;
- `contract`, `catalog`: the `Catalog` seam and public `MemoryCatalog` adapter.

Add new SQL completion behavior to the relevant integration scenario instead
of adding resolver tests inside `src/lib.rs`.
