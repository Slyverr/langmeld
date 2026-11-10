# langmeld-lsp

This crate adapts Langmeld language backends to the Language Server Protocol.
It provides full-document synchronization, diagnostics, completion,
go-to-definition, and references for the capabilities a backend declares.

`DeclarativeBackend` parses source with a `LanguageSpec` and provides grammar
diagnostics and scoped syntax completions. Rust hosts can implement
`LanguageBackend` to retain their own AST and semantic state and supply
language-specific editor queries. The LSP layer handles protocol positions,
document versions, and capability advertisement.

See [Editor integration](../../docs/editor-integration.md) for the current
scope and embedding API.
