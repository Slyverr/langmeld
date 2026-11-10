# Editor integration

`langmeld-lsp` provides a synchronous Language Server Protocol adapter for
Langmeld languages. It supports full-document synchronization, diagnostics,
completion, go-to-definition, references, host-owned rename, hover, and
host-provided document symbols and semantic tokens. It also supports
host-provided full-document formatting and advertises only operations enabled
by the backend.

## Declarative languages

`DeclarativeBackend` accepts a validated `LanguageSpec`. It parses open source
files and publishes parser diagnostics, and uses declared grammar scopes for
completion. It does not infer symbol resolution, semantic diagnostics,
hover, rename, document symbols, formatting, or semantic highlighting.

## Rust hosts

Implement `LanguageBackend` when a language owns its AST or semantic rules. The
backend's associated document state can retain the parse and analysis for each
open source snapshot. Implement only the queries the language supports and
return matching `BackendCapabilities`; the server uses that report to build
its LSP capabilities. `LanguageBackend::did_close` receives the last snapshot
when a document closes, allowing hosts to remove it from workspace indexes.
Hosts can implement `format_document` to return formatted text for a complete
document. Langmeld passes through the editor's formatting options and returns
one whole-document edit using the negotiated position encoding. Formatting
policy remains language-owned.
Hosts can also implement `hover` to return plain-text or Markdown information,
optionally associated with a source range. The adapter converts both the
request position and returned range using the negotiated position encoding,
and sends Markdown only when the client advertises support for it.
For rename, hosts return every declaration and reference span for the symbol.
The adapter groups edits by open document and rejects the whole edit if a
location is missing, stale, ambiguous, malformed, or overlaps another edit.
This avoids returning a partial rename when the host's workspace index cannot
be mapped to the server's open snapshots.
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
target document is open in the server.

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
integers, and double-quoted strings. It is lexical highlighting, not a generated
parser: comments, semantic roles, and host-specific lexer rules are not
inferred. CI tokenizes the generated example grammar with VS Code's TextMate
engine and checks representative keyword, punctuation, number, string, escape,
and non-keyword scopes. This validates the emitted grammar's behavior in the
TextMate tokenizer; it does not test VS Code extension packaging, themes, or
editor UI. Use the LSP for semantic diagnostics and navigation. See the
[VS Code syntax highlighting guide](https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide)
for TextMate grammar integration details.

## WebAssembly

The `langmeld-core`, `langmeld-expression`, `langmeld-language`, and
`langmeld-textmate` crates compile for `wasm32-unknown-unknown`. This makes
their Rust APIs usable from a Rust WebAssembly application, but Langmeld does
not provide JavaScript bindings or a browser editor adapter. The project
resolver uses filesystem APIs, and the LSP adapter currently serves protocol
connections rather than browser workers.

## Current limits

The server uses full-text document changes. Incremental edits and Tree-sitter
exports are not provided yet.
TextMate output is lexical only; it does not infer comments, host lexer rules,
or semantic classifications. Diagnostics without a source location or tied to
another file are not published as diagnostics for the current document.
