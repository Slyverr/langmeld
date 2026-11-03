# @NAME@

A small Rust language project using Langmeld. The language model is split into
`language/language.lm` and `language/constructs/`; Rust host code lives in
`src/`, and runnable source examples live in `examples/`.

## Try it

```sh
cargo test
cargo run --example hello
cargo run --bin @SLUG@-host -- check --project
cargo run --bin @SLUG@-host -- build --project
```

`src/lib.rs` loads the modular `.lm` definition and prepares the generic parser.
Use it as the starting point for host-specific lexing, parsing, analysis, or
output. Add new constructs in their own files under `language/constructs/`.
The generated `@SLUG@-host` CLI supports `check`, `build`, and `run`. Project
commands use `langmeld.json`; source paths can also be passed directly.
`run` calls `run_sources` in `src/lib.rs`, where this starter reports that
runtime behavior still needs to be implemented.
Register a backend in `src/host/backend.rs` before using `build`. Project
backend defaults live in the `build` object in `langmeld.json`; command-line
options override those defaults for one build. The starter has no registered
backend, so a fresh project reports that one must be registered.
In interactive setup, choose whether to include the CLI, or pass
`langmeld init --library-only` to create a Rust library without it.

Deterministic editor and language metadata is created under `.langmeld/generated/`.
That directory is ignored and can be rebuilt from the validated language model
with `generate_tooling`.
