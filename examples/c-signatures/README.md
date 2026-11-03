# C signature importer

This independent Rust consumer uses a declarative `.lm` model for a focused C
header grammar, then registers a host parser for C's declaration syntax. It
accepts `void name(void);` and two-parameter `int name(int left, int right);`
prototypes and emits Rust `extern "C"` declarations. The example demonstrates
how a language host can combine declarative construct metadata with a custom
parser; it does not claim to parse all of C.

## Try it

```sh
cargo test --offline --locked
cargo run --offline --locked --bin csignatures-host -- check --project
cargo run --offline --locked --bin csignatures-host -- run --project
```

The generic model provides construct metadata and relationships. The parser in
`src/host/parser.rs` owns C tokenization and declaration parsing, while
`src/lib.rs` owns the C-to-Rust output contract. Edit the `.lm` model and host
parser together when adding syntax.
