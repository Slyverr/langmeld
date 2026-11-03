# Rust authored function language

This library-only consumer builds its canonical language model with Langmeld's
Rust API in `src/language/spec.rs`. Its generic parser recognizes function
declarations with zero or more typed parameters, `fn`/`func` keyword choices,
and an optional `unsafe` modifier:

```text
module {
  fn empty() -> i32
  fn identity(value: i32) -> i32
  func unsafe add(left: i32, right: i32, extra: i32) -> i32
}
```

The host in `src/lib.rs` lowers parsed declarations into Rust source. It
validates names and accepts only the `i32` type for this output contract; this
example is a focused compiler front end, not a complete Rust parser. It lowers
zero-parameter functions to `0`, one-parameter functions to their argument,
and multi-parameter functions to a sum of their arguments.

Run the consumer checks from this directory:

```sh
cargo test --offline --locked
cargo clippy --all-targets --offline --locked -- -D warnings
```
