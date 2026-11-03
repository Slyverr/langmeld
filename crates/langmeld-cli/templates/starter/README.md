# @NAME@

A small Rust language project using Langmeld. The language model is split into
`language/language.lm` and `language/constructs/`; Rust host code lives in
`src/`, and runnable source examples live in `examples/`.

## Try it

```sh
cargo test
cargo run --example hello
cargo run --bin @SLUG@-host -- check --project
```

`src/lib.rs` loads the modular `.lm` definition and prepares the generic parser.
Use it as the starting point for host-specific lexing, parsing, analysis, or
output. Add new constructs in their own files under `language/constructs/`.
The generated `@SLUG@-host` CLI supports `check` and `run`. `check --project`
uses `langmeld.json`; `run` calls `run_sources` in `src/lib.rs`, where this
starter reports that runtime behavior still needs to be implemented. Pass
source paths directly to either command to work without the project manifest.
In interactive setup, choose whether to include the CLI, or pass
`langmeld init --library-only` to create a Rust library without it.

Deterministic editor and language metadata is created under `.langmeld/generated/`.
That directory is ignored and can be rebuilt from the validated language model
with `generate_tooling`.
