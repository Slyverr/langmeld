# langmeld-lsp

This crate adapts Langmeld language backends to the Language Server Protocol.
It provides full-document synchronization, diagnostics, completion,
go-to-definition, references, rename, formatting, hover, and semantic tokens
and document symbols for the capabilities a backend declares.

`DeclarativeBackend` parses source with a `LanguageSpec` and provides grammar
diagnostics and scoped syntax completions. Rust hosts can implement
`LanguageBackend` to retain their own AST and semantic state and supply
language-specific editor queries. Backend lifecycle hooks include document
close notification, initial workspace context, workspace-folder changes,
opt-in configuration and watched-file events, and workspace analysis so hosts
can account for unsaved changes across open documents. Configuration and file
events are registered only when supported by the client. The LSP layer handles
protocol positions, document versions, semantic-token encoding, and capability
advertisement. `LanguageDefinitionBackend` supplies an outline for language,
construct, and import declarations in `.langmeld` files. Semantic token names
and source spans come from the host; the adapter validates the legend, splits
multiline spans into line-local tokens,
and encodes them using the negotiated position encoding. Rename is
also host-owned: the adapter validates every result against open snapshots and
rejects the edit rather than applying only part of it. The adapter does not
infer semantic classifications or language-specific formatting from syntax.
Hosts return document outline entries with UTF-8 byte ranges; the adapter
validates and converts them, and selects nested or flat output to match the
client. `HostLanguageBackend` connects the existing
`LanguageParser` and optional `LanguageAnalyzer` contracts to parser and
semantic diagnostics, and can attach host completion and navigation
providers. Other editor queries remain host-owned.

For semantics that span several open documents, attach a
`LanguageWorkspaceAnalyzer` with `HostLanguageBackend::with_workspace_analyzer`.
The adapter validates that results map to the corresponding source snapshots
and refreshes diagnostics for every open document. Hosts that need persistent
parse caches or unopened project files can implement `LanguageBackend`
directly.

See the [LSP integration guide](../../docs/editor/lsp.md) for the current
scope and embedding API.
