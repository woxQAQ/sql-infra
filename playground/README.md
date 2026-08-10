# pg-completion playground

A Vue 3 and Monaco playground backed by the real `pg-completion` collector.
The Rust adapter is compiled to WebAssembly and runs in a Web Worker; the
browser does not send SQL or catalog data to a server.

## Run

Enter the repository's devenv shell. It provides Rust (including the
`wasm32-unknown-unknown` target), Node.js, and pnpm:

```bash
devenv shell
pnpm install
playground
```

The `playground` command starts the Vite development server. From inside the
devenv shell, it is equivalent to `pnpm --dir playground dev`.

Open <http://127.0.0.1:5173>. A production build is created with:

```bash
build-playground
```

## What is real

- SQL candidates come from `pg_completion::collect`, not a TypeScript keyword
  list.
- Catalog JSON is the caller-owned metadata adapter. Editing it changes the
  next completion request.
- Relation columns, outer scopes, CTEs, DML pseudo-relations, DDL containers,
  schema objects, grammar tokens, phrases, and privileges are resolved into a
  single Monaco completion list.
- Monaco offsets are converted from UTF-16 code units to Rust UTF-8 byte
  offsets. Returned replacement ranges are converted back to UTF-16.
- Completion runs off the UI thread. The inspector exposes candidates,
  completion intent, scope, diagnostics, and the full wire response.

The Catalog JSON shape is intentionally generic:

```json
{
  "searchPath": ["public"],
  "objects": [
    {
      "kind": "Table",
      "name": ["public", "users"],
      "detail": "optional object detail",
      "members": [
        { "kind": "Column", "name": "id", "detail": "bigint" }
      ]
    },
    {
      "kind": "Function",
      "name": ["u", "refresh"],
      "detail": "refresh() → void"
    }
  ]
}
```

Names are semantic PostgreSQL identifier parts. Lowercase strings model normal
unquoted identifiers; mixed-case strings require quoted SQL insertion. Objects
outside `searchPath` are inserted with a qualified name.

## Implementation map

- `wasm/src/lib.rs`: UTF-16 mapping, catalog resolution, candidate rendering,
  context DTO, and the one-operation JSON/WASM interface.
- `src/completion.worker.ts`: owns the WASM instance and serializes requests.
- `src/worker-client.ts`: asynchronous browser-side request client.
- `src/composables/usePlayground.ts`: Monaco provider, initial query, Catalog
  validation, worker requests, and completion state.
- `src/components`: Vue editor shells, inspector views, and
  responsive workspace navigation.
- `src/monaco.ts`: Monaco workers, SQL themes, and Catalog JSON schema.
- `scripts/build-wasm.mjs`: reproducible Rust-to-WASM build and asset copy.
