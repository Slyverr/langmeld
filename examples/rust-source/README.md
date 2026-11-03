# Rust source parser consumer

This independent language project uses Langmeld for its canonical construct
model, generic AST projection, source spans, and typed Rust accessors. Its host
delegates Rust syntax parsing to the mature `syn` parser, then retains a typed,
host-owned signature projection alongside each generic Langmeld node. Since
`syn` signatures include thread-affine span handles, the host maps them into an
owned `Send + Sync` representation before attaching that data to Langmeld.
The host also replaces Langmeld's default lexer with an opaque source token;
this keeps the generic lexer from rejecting Rust punctuation or Unicode before
`syn` sees the source. Hosts that need token-level editor highlighting can emit
a richer token stream from their own lexer.

The example indexes top-level function declarations from Rust files. It is not
a Rust compiler or a replacement for `rust-analyzer`; it demonstrates how a
language host can reuse an existing parser while exposing selected constructs
to Langmeld's shared tooling and typed model.

Run the consumer checks from this directory:

```sh
cargo test --offline --locked
cargo clippy --all-targets --offline --locked -- -D warnings
```
