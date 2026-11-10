# langmeld-lsp

This crate adapts Langmeld language backends to the Language Server Protocol.
It provides full-document synchronization, diagnostics, completion,
go-to-definition, references, rename, formatting, hover, and semantic tokens
and document symbols for the capabilities a backend declares.

`DeclarativeBackend` parses source with a `LanguageSpec` and provides grammar
diagnostics and scoped syntax completions. Rust hosts can implement
`LanguageBackend` to retain their own AST and semantic state and supply
language-specific editor queries. Backend lifecycle hooks include document
close notification and optional workspace analysis so hosts can account for
unsaved changes across open documents. The LSP layer handles protocol
positions, document versions, semantic-token encoding, and capability
advertisement. Semantic token names and source spans come from the host; the
adapter validates the legend, splits multiline spans into line-local tokens,
and encodes them using the negotiated position encoding. Rename is
also host-owned: the adapter validates every result against open snapshots and
rejects the edit rather than applying only part of it. The adapter does not
infer semantic classifications, document outline, or language-specific
formatting from syntax. Hosts return document outline entries with UTF-8 byte
ranges; the adapter validates and converts them, and selects nested or flat
output to match the client.

See [Editor integration](../../docs/editor-integration.md) for the current
scope and embedding API.
