# Editor integration

Langmeld provides an LSP adapter for diagnostics and host-provided language
features, plus optional exporters for lexical TextMate and Tree-sitter
grammars. These are integration building blocks; the project does not currently
ship editor plugins or a browser editor package.

- Use the [LSP guide](lsp.md) for declarative language support, `.langmeld`
  definition editing, and Rust-hosted language features.
- Follow the [Neovim guide](neovim.md) to connect Neovim's built-in LSP client
  to the `.langmeld` definition server.
- Use the [TextMate exporter](textmate.md) for lexical syntax highlighting.
- Use the [Tree-sitter exporter](tree-sitter.md) when the declarative grammar
  fits the exporter's supported subset.
- Read [WebAssembly support](webassembly.md) for browser-host constraints.

Editor-facing generated artifacts do not supply language semantics. A Rust
host remains responsible for its AST, name resolution, diagnostics, execution,
and any behavior that cannot be inferred from the declared grammar.
