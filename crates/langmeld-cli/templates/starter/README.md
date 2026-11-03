# @NAME@

A small Rust language project using Langmeld. The language model is split into
`language/language.lm` and `language/constructs/`; Rust host code lives in
`src/`, and runnable source examples live in `examples/`.

## Try it

```sh
cargo test
cargo run --example hello
```

`src/lib.rs` loads the modular `.lm` definition and prepares the generic parser.
Use it as the starting point for host-specific lexing, parsing, analysis, or
output. Add new constructs in their own files under `language/constructs/`.

Deterministic editor and language metadata is created under `.langmeld/generated/`.
That directory is ignored and can be rebuilt from the validated language model
with `generate_tooling`.
