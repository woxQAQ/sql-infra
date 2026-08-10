# SQL Infra

infracrate that used for any program looking for SQL language analyse.

The browser-based [`pg-completion` Monaco playground](playground/README.md)
runs the Rust completion collector locally through WebAssembly.

## Development environment

The repository uses [devenv](https://devenv.sh/) to provide Rust, the
`wasm32-unknown-unknown` target, Node.js, and pnpm:

```bash
devenv shell
cargo test --workspace
```

Useful commands available inside the shell:

- `playground` starts the playground development server.
- `build-playground` builds its WASM module and production frontend.
- `devenv test` runs the Rust workspace tests and the playground build.
