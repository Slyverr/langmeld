# Langmeld

Langmeld is an early-stage, Rust-first framework for describing language
syntax, parsing source, and building language-specific behavior in Rust.

The current libraries provide source snapshots, revision-aware diagnostics, a
validated syntax model, and a generic parser for token patterns, alternative
forms, optional and repeated groups, and scoped blocks. The `.lm` frontend is
itself parsed by that generic parser. The parser does not yet provide
expressions, type checking, execution, code generation, or editor integrations;
language authors own those behaviors in Rust.

## Requirements

- Rust 1.88 or newer

## Try the example

```sh
cargo check --workspace --locked
cargo run -p langmeld-toy-language-example --locked
```

The example language definition and its Rust consumer are in
[`examples/toy-language`](examples/toy-language).

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
