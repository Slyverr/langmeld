# Custom lexer

This example combines a modular `.langmeld` grammar with a Rust lexer for raw
strings. The grammar declares its line-comment marker in
`language/language.langmeld`, and the lexer reads that rule from the loaded
`LanguageSpec`; Rust supplies only the tokenization that the built-in lexer
does not support.

Run it from the workspace root:

```sh
cargo run -p langmeld-custom-lexer-example --locked
cargo run -p langmeld-custom-lexer-example --locked -- path/to/program.rawsay
cargo test -p langmeld-custom-lexer-example --locked
```

The custom lexer owns tokenization and applies the grammar's comment rule. The
generic parser still validates token spans and matches them against the
declarative grammar.
