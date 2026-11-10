# Schema language example

This consumer implements a compact `record Name { field: Type }` language with
its own lexer, parser, AST, and semantic analysis. It uses Langmeld's source,
diagnostic, host-parser, and host-analyzer contracts. Its analyzer collects a
global record namespace before resolving field types, so forward and recursive
references work without Aster's lexical-scope rules.

Run it with:

```sh
cargo run -p langmeld-schema-language-example --locked
```

The example keeps schema-specific type rules in its host crate. It is intended
to test which semantic policies might be reusable across language designs;
Langmeld does not generate this analysis.
