# Editor integration

`langmeld-lsp` provides a synchronous Language Server Protocol adapter for
Langmeld languages. It supports full-document synchronization, diagnostics,
completion, go-to-definition, and references. It advertises only completion
and navigation operations enabled by the backend.

## Declarative languages

`DeclarativeBackend` accepts a validated `LanguageSpec`. It parses open source
files and publishes parser diagnostics, and uses declared grammar scopes for
completion. It does not infer symbol resolution, semantic diagnostics,
hover, rename, formatting, or semantic highlighting.

## Rust hosts

Implement `LanguageBackend` when a language owns its AST or semantic rules. The
backend's associated document state can retain the parse and analysis for each
open source snapshot. Implement only the queries the language supports and
return matching `BackendCapabilities`; the server uses that report to build
its LSP capabilities.

```rust,ignore
use langmeld_core::SourceFile;
use langmeld_lsp::{BackendCapabilities, BackendDocument, LanguageBackend};

struct MyLanguage;

impl LanguageBackend for MyLanguage {
    type Document = MyParseAndAnalysis;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let (state, diagnostics) = analyze_my_language(source);
        BackendDocument::new(source, state, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            definition: true,
            references: true,
        }
    }
}
```

Run an embedded server over stdio with `langmeld_lsp::run_stdio(backend)` or
use `run_connection` with an existing LSP connection. Host implementations own
their resolution policies and must return source locations tied to the
corresponding source snapshots. Cross-file locations are converted when their
target document is open in the server.

## Current limits

The server uses full-text document changes. Incremental edits, rename, hover,
document symbols, formatting, semantic tokens, TextMate, and Tree-sitter
exports are not provided yet. Diagnostics without a source location or tied to
another file are not published as diagnostics for the current document.
