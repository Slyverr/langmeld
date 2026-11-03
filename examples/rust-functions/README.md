# Rust authored function language

This library-only consumer builds its canonical language model with Langmeld's
Rust API in `src/language/spec.rs`. Its generic parser recognizes two function
forms, including declarations with parameters:

```text
module {
  fn empty() -> i32
  fn add(left: i32, right: i32) -> i32
}
```

The host in `src/lib.rs` lowers parsed declarations into Rust source. It
validates names and accepts only the `i32` type for this output contract; this
example is a focused compiler front end, not a complete Rust parser.

Run the consumer checks from this directory:

```sh
cargo test --offline --locked
cargo clippy --all-targets --offline --locked -- -D warnings
```
