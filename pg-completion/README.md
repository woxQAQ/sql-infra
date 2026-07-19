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
