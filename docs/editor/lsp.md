# Language Server Protocol

`langmeld-lsp` provides a Language Server Protocol adapter for Langmeld
languages. It supports full-document replacements and incremental ranged
changes, diagnostics, completion, go-to-definition, references, host-owned
rename, hover, document symbols, and semantic tokens. It also supports
host-provided full-document formatting and advertises only operations enabled
by the backend.

## Declarative languages

`DeclarativeBackend` accepts a validated `LanguageSpec`. It parses open source
files and publishes parser diagnostics, and uses declared grammar scopes for
completion. It does not infer symbol resolution, semantic diagnostics,
hover, rename, document symbols, formatting, or semantic highlighting.

## Langmeld definitions

Run `langmeld lsp --definitions` to edit `.langmeld` files with the built-in
definition grammar. This mode publishes parser diagnostics, context-aware
syntax completions for root declarations and nested construct syntax, a
document outline for language, construct, and import declarations, and
formatting edits that honor the editor's indentation options. Formatting is
available only for syntactically valid files. This mode does not run
definition-level semantic checks without a project manifest. When a manifest is
available in the working directory or passed with `--manifest-path`, the
backend validates the complete imported definition graph using all open
project files as overlays; unopened files are read from disk. When any open
file changes or closes, diagnostics are refreshed for every remaining open
project file. Project mode also provides go-to-definition and references for
construct and category names across imported definition files. Use
`langmeld check --manifest-path langmeld.toml` to validate the complete on-disk
project definition and imports.

## Rust hosts

Implement `LanguageBackend` when a language owns its AST or semantic rules. The
backend's associated document state can retain the parse and analysis for each
open source snapshot. Implement only the queries the language supports and
return matching `BackendCapabilities`; the server uses that report to build
its LSP capabilities. Implement `analyze_workspace` when analysis depends on
multiple open files: the server supplies all current snapshots and expects one
result per source in the same order. This lets a host include unsaved imports
and refresh diagnostics together. The default leaves per-document analysis
unchanged. `LanguageBackend::did_close` receives the last snapshot when a
document closes, allowing hosts to remove it from workspace indexes.
`initialize_workspace` receives the client's root URI, initial workspace
folders, folder-support flag, and initialization options before the server
finishes initialization. Return `BackendWorkspaceCapabilities` from
`workspace_capabilities` to opt into workspace lifecycle events. Set
`workspace_folders` when the backend handles folder changes; the server then
advertises workspace-folder support and forwards
`workspace/didChangeWorkspaceFolders` when the client supports it. Set
`configuration_changes` to receive editor settings through
`did_change_configuration`, and provide `watched_files` patterns to receive
matching file events through `did_change_watched_files`. Configuration and file
watchers are dynamically registered only when the client advertises support;
callbacks begin only after the client accepts the registration. The backend
owns indexing and decides how these events affect project state. These hooks
report changes; they do not load files or rebuild indexes automatically. After
each accepted workspace event, Langmeld reanalyzes open documents and republishes
their diagnostics. The initial workspace context reports whether the client
supports relative watcher patterns. If the backend requests any relative
pattern from a client that does not support them, Langmeld skips that
file-watcher registration.
For hosts that already implement `LanguageParser` and `LanguageAnalyzer`,
`HostLanguageBackend::new_with_analyzer(parser, analyzer)` combines their
parse and semantic diagnostics for LSP. Add `with_completer` and
`with_navigator` to expose host completion, definition, and reference
callbacks. Add `with_workspace_analyzer` when semantics depend on more than one
open document; its analyzer receives each source with its parsed tree and must
return one source-bound analysis result for each input. The adapter combines
those results with parser diagnostics and reanalyzes open documents when the
workspace changes. If parsing or workspace analysis violates the source
identity or result-count contract, the adapter falls back to per-source
analysis for every supplied document so other open documents do not retain
stale diagnostics. It parses each open source again for a workspace pass;
implement `LanguageBackend` directly when the host needs persistent parse
caches, unopened files, or finer control. The parser-only form publishes
parser diagnostics. Select the analyzer at construction, then attach its
workspace analyzer, completer, and navigator. Rename, formatting, and hover
still require a custom `LanguageBackend`.
Hosts can implement `format_document` to return formatted text for a complete
document. Langmeld passes through the editor's formatting options and returns
one whole-document edit using the negotiated position encoding. Formatting
policy remains language-owned.
Hosts can also implement `hover` to return plain-text or Markdown information,
optionally associated with a source range. The adapter converts both the
request position and returned range using the negotiated position encoding,
and sends Markdown only when the client advertises support for it.
For rename, hosts return every declaration and reference span for the symbol.
The adapter groups edits by URI and rejects the whole edit if a location is
missing, stale, ambiguous, malformed, or overlaps another edit. A custom
`LanguageBackend` can implement `source_for_location` to resolve closed-file
targets; `HostLanguageBackend` accepts the same resolver with
`with_source_resolver`:

```rust,ignore
use std::sync::Arc;
use langmeld_lsp::BackendSource;

let backend = backend.with_source_resolver(move |location| {
    let indexed = project_index.get(&location.source_name)?;
    (indexed.source.identity() == location.identity).then(|| {
        BackendSource::new(indexed.uri.clone(), Arc::clone(&indexed.source))
    })
});
```

The resolver must return the source snapshot matching the location's name and
identity. Langmeld validates the identity and span before returning navigation
results or edits; the host remains responsible for keeping its closed-file
index current.

Workspace symbol search is also host-owned. Implement `workspace_symbols` on a
custom `LanguageBackend`, or use `HostLanguageBackend::with_workspace_symbol_provider`
to query the host's project index. Return `WorkspaceSymbolInfo` entries with
source locations; Langmeld resolves open files itself and asks
`source_for_location` for closed-file snapshots. The host decides which symbol
kinds are searchable and how queries match names.

When a host index overlays unsaved editor buffers, register
`HostLanguageBackend::with_document_close_handler` to remove that overlay or
reload the closed file's disk snapshot when the editor closes it. The callback
receives the last source snapshot Langmeld analyzed for that document.

Hosts can also return document outline trees with UTF-8 byte ranges for each
symbol and its selection range. The adapter validates that every range is
inside its source snapshot, that selections are contained by their symbols, and
that child symbols are contained by their parents. It converts the ranges using
the negotiated position encoding and returns either a nested outline or a
flat list according to the client's advertised support.

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
            ..BackendCapabilities::default()
        }
    }
}
```

Run an embedded server over stdio with `langmeld_lsp::run_stdio(backend)` or
use `run_connection` with an existing LSP connection. Host implementations own
their resolution policies and must return source locations tied to the
corresponding source snapshots. Cross-file locations are converted when their
target document is open, or when the backend resolves a closed-file target to
its current source snapshot and URI.

The [Aster example](../../examples/aster-language) demonstrates a host backend
that combines its own AST and semantic diagnostics with resolved definition,
reference, and rename queries. It also provides semantic token types for
functions, parameters, and variables, classifying declarations and resolved references.
Hosts return token names and UTF-8 byte spans; the adapter validates the legend,
converts spans using the negotiated position encoding, and splits multiline
spans into line-local tokens. Declarative grammars do not imply semantic roles.

## Current limits

The server accepts both full-document replacements and incremental ranged
changes. It applies changes in order using the negotiated LSP position
encoding. Each change still produces a new source snapshot and runs analysis
for that snapshot; this is not incremental parsing or semantic analysis.
TextMate output is lexical only; it includes configured comments but does not
infer undeclared comments, host lexer rules, or semantic classifications.
Diagnostics without a source location or tied to
another file are not published as diagnostics for the current document.
