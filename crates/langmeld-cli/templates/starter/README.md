# @NAME@

A Rust language project using Langmeld. `langmeld init` lets you choose a
declarative model in `language/`, a Rust model in `src/language/`, or a Rust
base model with local `.lm` extensions. Rust host code lives in `src/`, and
runnable source examples live in `examples/`.

## Try it

```sh
cargo test
cargo run --example hello
cargo run --bin @SLUG@-host -- check --project
cargo run --bin @SLUG@-host -- build --project
```

`src/lib.rs` uses the model compiled by `build.rs` by default and exposes
`load_language_runtime` and `load_project_runtime` for explicit runtime loading.
The generic parser is prepared from the selected model. Use the host as the
starting point for custom lexing, parsing, analysis, or output. Keep `.lm`
definitions modular under `language/`.
The generated `@SLUG@-host` CLI supports `check`, `build`, `configure`, and `run`. Project
commands use `langmeld.json`; source paths can also be passed directly.
`run` calls `run_sources` in `src/lib.rs`, where this starter reports that
runtime behavior still needs to be implemented.
Register a backend in `src/host/backend.rs` before using `build`. Project
backend defaults live in the `build` object in `langmeld.json`; command-line
options override those defaults for one build. The starter has no registered
backend, so a fresh project reports that one must be registered.
After registering one or more backends, use `configure --interactive` to save
backend defaults. Scripted updates can use `--backend`, `--format`, `--target`,
`--source-maps`, and repeatable `--set KEY=VALUE`; `--clear` removes only the
backend defaults from `langmeld.json`.
In interactive setup, choose whether to include the CLI, or pass
`langmeld init --library-only` to create a Rust library without it.

Deterministic editor and language metadata is created under `.langmeld/generated/`.
That directory is ignored and can be rebuilt from the validated language model
with `generate_tooling`.
