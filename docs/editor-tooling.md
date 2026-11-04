# Editor and generated tooling

Langmeld can generate or provide several editor-facing surfaces from a
language definition. Each surface has different capabilities and limits.

## Language Server Protocol

The `langmeld-lsp` crate provides diagnostics and editor features for
declarative languages. Start it for a language project with:

```sh
cargo run -p langmeld-lsp -- --project langmeld.json
```

It supports document edits, parsing and semantic diagnostics, literal-keyword
completion, construct document symbols, schema-based hover, and semantic
tokens. When a language declares symbol metadata, it can provide definitions,
references, and safe rename across project files. Formatting is available for
languages using the built-in lexer. Rust hosts can register custom formatting,
completion, parsing, and analysis behavior.

Langmeld's own `.langmeld` files can be edited with its built-in definition grammar:

```sh
cargo run -p langmeld-lsp -- --definitions --root language/language.langmeld
```

Project mode reloads valid definition changes and reparses open documents.
Clients must support the relevant LSP features, such as dynamic watched-file
registration, to receive all project-file updates.

The LSP server runs as a native Rust process. It does not itself compile to or
run in WebAssembly.

## Highlighting and grammar exports

`langmeld generate` emits lexical TextMate rules and conventional editor
metadata from the language model. These rules cover lexical classification;
they do not provide semantic highlighting.

The optional Tree-sitter exporter produces grammars for compatible declarative
syntax. Some custom parser behavior and language constructs cannot be
represented as a Tree-sitter grammar. The exporter reports unsupported
definitions rather than claiming grammar equivalence. Tree-sitter grammar
validation is separate from Langmeld's runtime parser and semantic checks.

LSP semantic tokens supply semantic highlighting when the editor supports
them. They complement lexical TextMate or Tree-sitter highlighting.

## Browser and WebAssembly

The initializer can add a WebAssembly adapter to a generated language project.
It runs parsing and generic analysis in the browser and returns diagnostics,
syntax data, editor classifications, and supported completion or reference
information from caller-supplied source text.

The caller supplies language definitions, imports, and workspace source text;
the adapter does not read files or resolve paths. Source spans are UTF-8 byte
offsets. A generated project's `fromProject` adapter uses its compiled model
and registered Rust callbacks. The adapter is a runtime target for language
tools, not a backend that translates the user's language into WebAssembly.

See the generated project's `README-wasm.md` for its build and JavaScript
binding instructions.
