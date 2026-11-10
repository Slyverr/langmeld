# Custom lexer

This example combines a modular `.langmeld` grammar with a Rust lexer for raw
strings and line comments. The grammar remains in `language/language.langmeld`;
Rust supplies only the tokenization that the built-in lexer does not support.

Run it from the workspace root:

```sh
cargo run -p langmeld-custom-lexer-example --locked
cargo run -p langmeld-custom-lexer-example --locked -- path/to/program.rawsay
cargo test -p langmeld-custom-lexer-example --locked
```

The custom lexer owns all tokenization, including comment handling. The
generic parser still validates token spans and matches them against the
declarative grammar.
