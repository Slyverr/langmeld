# Combined Brief language example

Brief starts with a Rust-authored base language model in
`src/language/spec.rs`. The model declares the program root, a statement
category, and the `print` construct. `language/extensions/starter.langmeld` adds a
`note` construct to that category, and the combined starter source uses both.

The Rust host in `src/lib.rs` renders the parsed AST as Markdown. This example
shows how Rust APIs and `.langmeld` extensions can contribute constructs to one
validated language while the host chooses its output format and semantics.

Run from this directory:

```sh
cargo test --offline --locked
cargo run --offline --locked -- check --project
cargo run --offline --locked -- run --project
```

The `run` command prints a Markdown paragraph and a blockquote.
