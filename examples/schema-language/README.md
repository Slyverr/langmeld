# Schema language example

This consumer implements a compact `record Name { field: Type }` language with
its own lexer, parser, AST, and semantic analysis. It uses Langmeld's source,
diagnostic, parser, analyzer, and workspace-analysis contracts. The analyzer
collects a global record namespace before resolving field types, so forward and
recursive references work without Aster's lexical-scope rules. The workspace
analyzer extends that namespace across currently open documents and keeps the
cross-file declaration and reference locations in a shared analysis index.
Completion, definition, and references all use the same resolved results.

Run it with:

```sh
cargo run -p langmeld-schema-language-example --locked
```

Run the standalone LSP server using the host parser, analyzer, completion, and
navigation contracts:

```sh
cargo run -p langmeld-schema-language-example --bin schema-lsp --locked
```

The server publishes parser and semantic diagnostics, completes field types
from the global record namespace, and supports definition and reference
queries across open schema files. Closed files are not indexed by this example.

The example keeps schema-specific type rules in its host crate. It is intended
to test which semantic policies might be reusable across language designs;
Langmeld does not generate this analysis.
