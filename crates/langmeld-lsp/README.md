# langmeld-lsp

This crate adapts Langmeld language backends to the Language Server Protocol.
It provides full-document synchronization, diagnostics, completion,
go-to-definition, references, and full-document semantic tokens for the
capabilities a backend declares.

`DeclarativeBackend` parses source with a `LanguageSpec` and provides grammar
diagnostics and scoped syntax completions. Rust hosts can implement
`LanguageBackend` to retain their own AST and semantic state and supply
language-specific editor queries. Backend lifecycle hooks include document
close notification so hosts can keep workspace indexes in sync. The LSP layer
handles protocol positions, document versions, semantic-token encoding, and
capability advertisement. Semantic token names and source spans come from the
host; the adapter validates the legend, splits multiline spans into line-local
tokens, and encodes them using the negotiated position encoding. It does not
infer semantic classifications from syntax.

See [Editor integration](../../docs/editor-integration.md) for the current
scope and embedding API.
