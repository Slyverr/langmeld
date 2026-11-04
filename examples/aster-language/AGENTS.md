# Project guidance

Read [README.md](README.md) for the project workflow. Keep declarative language
definitions in `language/`, with each construct in `language/constructs/`.
Implement host-specific behavior in `src/` and keep examples under `examples/`.

Do not edit `.langmeld/generated/`; it is derived output. Rebuild it from the
validated language model with Langmeld's tooling API.

Before changes are ready, run `cargo fmt --all -- --check`, `cargo test`, and
`cargo clippy --all-targets -- -D warnings`.
