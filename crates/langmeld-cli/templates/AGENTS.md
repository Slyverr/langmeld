# Agent instructions

Read `README.md` for the project workflow and use these rules when changing the
language.

- Keep language-specific meaning and runtime behavior in Rust.
- For the declarative template, keep syntax in `language/` and construct
  behavior in `src/constructs/`.
- For the custom-parser template, keep syntax and AST decisions in the Rust
  parser implementation.
- Add behavior tests under `tests/` where they can exercise the public project
  API.
- Run `cargo fmt --all -- --check`, `cargo test --locked`, and `cargo run`
  before considering a language change complete.
