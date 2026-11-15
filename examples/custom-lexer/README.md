# Custom lexer

This example combines a modular `.langmeld` grammar with a Rust extension for
raw strings. Langmeld's built-in lexer handles identifiers, punctuation,
whitespace, and the line-comment marker declared in
`language/language.langmeld`; Rust supplies only the tokenization the default
lexer does not support.

Run it from the workspace root:

```sh
cargo run -p langmeld-custom-lexer-example --locked
cargo run -p langmeld-custom-lexer-example --locked -- path/to/program.rawsay
cargo test -p langmeld-custom-lexer-example --locked
```

`RawStringExtension` is composed with `ExtensibleLexer`, so ordinary tokens,
declared comments, and token limits continue to use the built-in behavior.
Implement `Lexer` directly when a language needs to replace the scanning
model, such as Brainfuck's command-only lexer.
