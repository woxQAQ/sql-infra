# pg-completion playground

A local Monaco playground backed by the real `pg-completion` collector. The
Rust adapter is compiled to WebAssembly and runs in a Web Worker; the browser
does not send SQL or catalog data to a server.

## Run

Prerequisites: current Rust, the `wasm32-unknown-unknown` target, Node.js 20+,
and pnpm.

```bash
rustup target add wasm32-unknown-unknown
cd playground
pnpm install
pnpm dev
```

Open <http://127.0.0.1:5173>. A production build is created with:

```bash
pnpm build
```

On Nix-based macOS environments where Rust was installed without `rust-lld`,
the build script automatically retries through `nixpkgs#lld`.

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
  completion intent, scope, recovery information, and the full wire response.

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
- `src/main.ts`: Monaco providers, scenarios, Catalog validation, and the
  inspector.
- `scripts/build-wasm.mjs`: reproducible Rust-to-WASM build and asset copy.
