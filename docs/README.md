# Langmeld documentation

This index is the entry point to the checked-in guides for language authors and
framework users. The guides describe behavior available in the current codebase.

## Get started

- [Project workflow](guides/project-workflow.md): initialize, run, and validate
  a language project.
- [Construct handlers](language/handlers.md): implement and register Rust
  behavior for declarative constructs.
- [Language definitions](language/definitions.md): organize `.langmeld` files,
  describe token patterns, and decide when to use Rust code.

## Integrate with editors

- [Editor integration](editor/README.md): choose an integration path.
- [Language Server Protocol](editor/lsp.md): configure definition editing or
  connect a Rust language host.
- [Neovim](editor/neovim.md): configure Neovim's built-in LSP client for
  `.langmeld` files.
- [TextMate](editor/textmate.md): generate and use lexical highlighting
  grammars.
- [Tree-sitter](editor/tree-sitter.md): generate compatible grammars and
  understand their limits.
- [WebAssembly](editor/webassembly.md): use Langmeld crates from a browser
  host and understand the filesystem boundary.

## Understand the framework

- [Architecture overview](architecture/overview.md): crate responsibilities,
  data flow, and the boundary between generated support and host-owned language
  behavior.
