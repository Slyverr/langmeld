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
cargo run --offline --locked --bin csignatures-host -- lsp
```

The generic model provides construct metadata and relationships. The parser in
`src/host/parser.rs` owns C tokenization and declaration parsing, while
`src/lib.rs` owns the C-to-Rust output contract. Edit the `.lm` model and host
parser together when adding syntax.

Because this language uses a host custom parser, it cannot be exported by the
declarative Tree-sitter grammar generator. Langmeld reports the unsupported
construct instead of emitting a grammar that omits the C parser's behavior.

The `lsp` command embeds Langmeld's LSP server with the same registered C
parser used by `check` and `run`. It reads and writes the standard LSP stream on
stdin and stdout, so an editor can launch it as a language server. The
integration test opens valid and unsupported C declarations and checks the
diagnostics produced by the host parser.
