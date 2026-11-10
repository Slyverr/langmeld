# Editor integration

`langmeld-lsp` provides a Language Server Protocol adapter for Langmeld
languages. It supports full-document replacements and incremental ranged
changes, diagnostics, completion, go-to-definition, references, host-owned
rename, hover, and host-provided document symbols and semantic tokens. It also
supports host-provided full-document formatting and advertises only operations
enabled by the backend.

## Declarative languages

`DeclarativeBackend` accepts a validated `LanguageSpec`. It parses open source
files and publishes parser diagnostics, and uses declared grammar scopes for
completion. It does not infer symbol resolution, semantic diagnostics,
hover, rename, document symbols, formatting, or semantic highlighting.

## Langmeld definitions

Run `langmeld lsp --definitions` to edit `.langmeld` files with the built-in
definition grammar. This mode publishes parser diagnostics, context-aware
syntax completions for root declarations and nested construct syntax, and
formatting edits that honor the editor's indentation options. Formatting is
available only for syntactically valid files. This mode does not run
definition-level semantic checks without a project manifest. When a manifest is
available in the working directory or passed with `--manifest-path`, the
backend validates the complete imported definition graph using all open
project files as overlays; unopened files are read from disk. When any open
file changes or closes, diagnostics are refreshed for every remaining open
project file. Use
`langmeld check --manifest-path langmeld.toml` to validate the complete on-disk
project definition and imports.

### Neovim

Neovim 0.11 or newer can use its built-in LSP client and completion menu for
`.langmeld` files. Add this to your Lua configuration, replacing the command
with an absolute executable path if `langmeld` is not on Neovim's `PATH`:

```lua
vim.filetype.add({ extension = { langmeld = "langmeld" } })

vim.lsp.config("langmeld-definitions", {
  cmd = { "langmeld", "lsp", "--definitions" },
  filetypes = { "langmeld" },
  root_markers = { "langmeld.toml" },
  on_attach = function(client, bufnr)
    vim.lsp.completion.enable(true, client.id, bufnr)
    vim.keymap.set("i", "<C-Space>", function()
      vim.lsp.completion.get()
    end, { buffer = bufnr, desc = "Langmeld completion" })
  end,
})

vim.lsp.enable("langmeld-definitions")
```

Start Neovim from the language project's root (for example, `nvim .`) so the
server finds `langmeld.toml` and enables project validation and imported-name
completion. Opening a `.langmeld` file without a project manifest still gives
the built-in syntax diagnostics, completion, and formatting. Use `<C-Space>`
to open the completion menu and `<C-y>` to accept a selected item. See the
[Neovim LSP guide](https://neovim.io/doc/user/lsp.html) for client setup and
completion details.

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
`HostLanguageBackend::new(parser).with_analyzer(analyzer)` combines their
parse and semantic diagnostics for LSP. Add `with_completer` and
`with_navigator` to expose host completion, definition, and reference
callbacks. Add `with_workspace_analyzer` when semantics depend on more than one
open document; its analyzer receives each source with its parsed tree and must
return one source-bound analysis result for each input. The adapter combines
those results with parser diagnostics and reanalyzes open documents when the
workspace changes. It parses each open source again for a workspace pass;
implement `LanguageBackend` directly when the host needs persistent parse
caches, unopened files, or finer control. The parser-only form publishes
parser diagnostics. Rename, formatting, and hover still require a custom
`LanguageBackend`.
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

The [Aster example](../examples/aster-language) demonstrates a host backend
that combines its own AST and semantic diagnostics with resolved definition,
reference, and rename queries. It also provides semantic token types for
functions, parameters, and variables, classifying declarations and resolved references.
Hosts return token names and UTF-8 byte spans; the adapter validates the legend,
converts spans using the negotiated position encoding, and splits multiline
spans into line-local tokens. Declarative grammars do not imply semantic roles.

## TextMate grammar export

The optional `langmeld-textmate` crate converts a declarative `LanguageSpec`
into a JSON TextMate grammar. Supply a unique scope name such as
`source.widget`; file extensions are optional metadata:

```rust,ignore
use langmeld_textmate::{TextMateOptions, generate};

let options = TextMateOptions::new("source.widget").file_types(["widget"]);
let grammar_json = generate(&language, &options)?;
```

The output highlights declared literal words and punctuation, decimal
integers, double-quoted strings, and comments configured in the language
definition. It is lexical highlighting, not a generated parser: undeclared
comments, semantic roles, and host-specific lexer rules are not inferred. CI
tokenizes the generated example grammar with VS Code's TextMate
engine and checks representative keyword, punctuation, number, string, escape,
and non-keyword scopes. This validates the emitted grammar's behavior in the
TextMate tokenizer; it does not test VS Code extension packaging, themes, or
editor UI. Use the LSP for semantic diagnostics and navigation. See the
[VS Code syntax highlighting guide](https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide)
for TextMate grammar integration details.

## Tree-sitter grammar export

The optional `langmeld-tree-sitter` crate converts a declarative `LanguageSpec`
to a Tree-sitter `grammar.js` source. It maps construct variants, captures,
optional and repeated groups, and child blocks. The generated lexer covers the
configured line comments and the built-in identifier, decimal integer, quoted
string, and punctuation categories. Configured block comments are reported as
unsupported.
It cannot infer a host lexer, custom parser, semantic rules, or runtime
behavior, so those languages need a host-authored Tree-sitter grammar.

Generate a grammar from a project manifest with:

```sh
langmeld generate tree-sitter \
  --manifest-path langmeld.toml \
  --grammar-name my_language > grammar.js
```

The Tree-sitter CLI is required to validate and compile the generated grammar:
run `tree-sitter generate grammar.js` from the grammar project. Tree-sitter can
reject conflicts that Langmeld's generic grammar model cannot resolve; edit
the generated grammar or keep a host-authored grammar for those cases. The
exporter does not generate a Tree-sitter project, parser bindings, or
highlighting queries.

## WebAssembly

The `langmeld-core`, `langmeld-expression`, `langmeld-language`,
`langmeld-project`, `langmeld-textmate`, and `langmeld-tree-sitter` crates
compile for `wasm32-unknown-unknown`. A browser host can use
`InMemoryDefinitionResolver` for embedded or virtual definition sources. The
manifest and directory loaders in `langmeld-project` still require native
filesystem access. Langmeld does not provide JavaScript bindings or a browser
editor adapter, and the LSP adapter currently serves protocol connections
rather than browser workers.

## Current limits

The server accepts both full-document replacements and incremental ranged
changes. It applies changes in order using the negotiated LSP position
encoding. Each change still produces a new source snapshot and runs analysis
for that snapshot; this is not incremental parsing or semantic analysis.
TextMate output is lexical only; it includes configured comments but does not
infer undeclared comments, host lexer rules, or semantic classifications.
Diagnostics without a source location or tied to
another file are not published as diagnostics for the current document.
