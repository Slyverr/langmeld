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

`langmeld generate` emits lexical TextMate rules and editor comment settings
from the language model. These rules cover lexical classification; they do not
provide semantic highlighting. Schema 4 comment delimiters drive the built-in
lexer and the generated editor settings. Tree-sitter export reports configured
block comments as unavailable until they can be represented exactly.
TextMate alternatives put longer tokens first, and identifier matching uses
the language's Unicode and hyphen continuation rules.
Generated bracket and auto-closing pairs come from declared blocks and the
built-in list, map, property, and expression field forms. Host-defined custom
syntax is not inferred from the declarative model.

The optional Tree-sitter exporter produces grammars for compatible declarative
syntax. Some custom parser behavior and language constructs cannot be
represented as a Tree-sitter grammar. The exporter reports unsupported
definitions rather than claiming grammar equivalence. Tree-sitter grammar
validation is separate from Langmeld's runtime parser and semantic checks.

LSP semantic tokens supply semantic highlighting when the editor supports
them. They complement lexical TextMate or Tree-sitter highlighting.

## Capability report

`langmeld generate` writes `agent-skill/references/capabilities.json` using
schema version 2. The Rust API exposes the same report through
`generate_tooling_capabilities`. It lists each generated artifact and generic
language-service feature as `supported`, `partial`, `customized`, or
`unavailable`; partial and unavailable entries include a reason. `customized`
means a Rust host must supply the behavior, such as execution or domain
lowering. The report describes Langmeld support for the validated language
model. It does not claim that a particular editor client implements every LSP
feature or that host behavior has been registered.

The generated grammar reference includes the report's editor-configuration
assumptions and tables of artifact and feature statuses. The generated Agent
Skill uses the same comment and bracket assumptions and points authors to the
reference and machine-readable report instead of maintaining a second list of
editor capabilities.

The generated `highlighting.json` and LSP initialization response use the same
semantic-token type and modifier legends. Their token indices therefore stay
aligned with the shared Langmeld classifications.

Project-definition reload is partial because it depends on editor clients
supporting dynamic watched-file registration. A language with custom parser
callbacks reports source formatting as customized so hosts can provide a
formatter for syntax outside the declarative grammar.

## Browser and WebAssembly

The initializer can add a WebAssembly adapter to a generated language project.
It runs parsing and generic analysis in the browser and returns diagnostics,
syntax data, editor classifications, and supported completion or reference
information from caller-supplied source text.

The caller supplies language definitions, imports, and workspace source text;
the adapter does not read files or resolve paths. Source spans are UTF-8 byte
offsets. Parse responses include source spans for generic block openers and
closers, which editor integrations can use for folding and delimiter matching.
Expression-valued fields are returned as recursively tagged JSON AST nodes,
including operator and member-name spans, so browser tools can inspect
subexpressions without reparsing source text.
A generated project's `fromProject` adapter uses its compiled model
and registered Rust callbacks. The adapter is a runtime target for language
tools, not a backend that translates the user's language into WebAssembly.

See the generated project's `README-wasm.md` for its build and JavaScript
binding instructions.
