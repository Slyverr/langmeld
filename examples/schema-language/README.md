# Schema language example

This consumer implements a compact `record Name { field: Type }` language with
its own lexer, parser, AST, and semantic analysis. It uses Langmeld's source,
diagnostic, host-parser, and host-analyzer contracts. Its analyzer collects a
global record namespace before resolving field types, so forward and recursive
references work without Aster's lexical-scope rules. Its `SchemaCompleter`
suggests record types for a partially typed field type using the existing
analysis result rather than reparsing or reanalyzing the document.
`SchemaNavigator` maps definition and references queries through the resolved
global record namespace, including forward and recursive references.

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
queries.

The example keeps schema-specific type rules in its host crate. It is intended
to test which semantic policies might be reusable across language designs;
Langmeld does not generate this analysis.
