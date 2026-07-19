# Separate syntax context from completion resolution

PostgreSQL completion is split across two modules at a deliberate seam.

`pg-parser::collect_completion` owns grammar-specific facts: the replacement
range, typed syntax expectations, the active statement range, visible range
references, CTEs, and the statement target relation. It remains independent of
database metadata and does not expose a partial PostgreSQL raw parse tree.
Its standard candidate pass runs the existing recursive-descent `Parser` and
`ExprParser` in completion mode against the prefix ending at the cursor. A
small completion-only token pass enriches cursor shapes that strict parsing
cannot represent, such as a partially typed name after `.`.

`pg-completion::complete` owns user-facing completion behaviour: catalog
queries, alias and scope resolution, PostgreSQL identifier quoting, prefix
filtering, ranking, deduplication, and graceful keyword-only degradation when
metadata is unavailable. Its output is editor-neutral; LSP conversion belongs
in an outer adapter.

We rejected stringly typed grammar-rule candidates because they create an
implicit protocol between parser and resolver. `Expectation`,
`NameExpectation`, and `ColumnContext` are the explicit interface instead.
We also rejected putting live database access in the parser. Metadata enters
through the `Catalog` interface, which supports both production and in-memory
adapters.
