use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, SourceLocation};
use langmeld_language::{
    CompletionCandidate, DefinitionFormatOptions, LanguageSpec, ParseResult, complete_at,
    format_language_definition, language_definition_grammar, parse,
};
use lsp_types::FormattingOptions;

use crate::{
    document_symbol::DocumentSymbolInfo,
    hover::HoverInfo,
    semantic::{SemanticToken, SemanticTokenLegend},
};

/// A host's retained document state and diagnostics for one source revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendDocument<T> {
    /// Identity and revision analyzed by the backend, when database-managed.
    pub source: Option<SourceIdentity>,
    /// Host-owned parse tree, analysis state, or combined document data.
    pub state: T,
    /// Findings to publish for this document.
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> BackendDocument<T> {
    /// Associates host state and diagnostics with the supplied source snapshot.
    #[must_use]
    pub fn new(source: &SourceFile, state: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source: source.identity(),
            state,
            diagnostics,
        }
    }
}

/// Editor features a backend has implemented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// The backend returns completion items.
    pub completion: bool,
    /// The backend supports go-to-definition queries.
    pub definition: bool,
    /// The backend supports find-references queries.
    pub references: bool,
    /// The backend supplies host-defined semantic tokens.
    pub semantic_tokens: bool,
    /// The backend formats a complete document.
    pub formatting: bool,
    /// The backend provides hover information.
    pub hover: bool,
    /// The backend provides symbol rename edits.
    pub rename: bool,
    /// The backend provides document outline symbols.
    pub document_symbols: bool,
}

/// Language analysis and optional editor queries consumed by the LSP adapter.
///
/// A host can retain any parsed or analyzed state in `Document`. Name
/// resolution, project semantics, and tree shape remain host-owned. Methods
/// with no implementation should keep their capability disabled.
pub trait LanguageBackend: Send + Sync + 'static {
    /// Backend-owned state retained for an open document.
    type Document: Send + Sync + 'static;

    /// Parses and analyzes one immutable source snapshot.
    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document>;

    /// Analyzes a set of open source snapshots as one workspace revision.
    ///
    /// Return `Some` only when the backend has produced one document, in the
    /// same order, for every supplied source. Each result must carry that
    /// source's identity. Returning `None` keeps the server's default
    /// per-document analysis behavior. Workspace-aware backends can use this
    /// hook to account for unsaved changes in related open files. If a
    /// returned workspace does not match the supplied snapshots, the server
    /// falls back to analyzing each open document separately.
    fn analyze_workspace(
        &self,
        _sources: &[SourceFile],
    ) -> Option<Vec<BackendDocument<Self::Document>>> {
        None
    }

    /// Notifies the backend when an open document is closed by the client.
    ///
    /// Hosts that maintain workspace indexes can remove this source here.
    /// The server calls this with the last analyzed snapshot before discarding
    /// it. The default implementation has no additional cleanup.
    fn did_close(&self, _source: &SourceFile, _document: &BackendDocument<Self::Document>) {}

    /// Reports implemented requests so the server advertises only support it
    /// actually has.
    fn capabilities(&self) -> BackendCapabilities;

    /// Returns completion candidates at a UTF-8 byte offset.
    fn complete(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
    ) -> Vec<CompletionCandidate> {
        Vec::new()
    }

    /// Returns declaration locations for a symbol at a UTF-8 byte offset.
    fn definition(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        Vec::new()
    }

    /// Returns references for a symbol at a UTF-8 byte offset.
    fn references(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
        _include_declaration: bool,
    ) -> Vec<SourceLocation> {
        Vec::new()
    }

    /// Returns the semantic token names advertised to the editor.
    ///
    /// Names follow the LSP semantic token vocabulary. Custom names are also
    /// allowed. This legend is used only when `semantic_tokens` is enabled.
    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        SemanticTokenLegend::default()
    }

    /// Returns host-classified source spans for one analyzed document.
    ///
    /// Tokens must use names present in [`Self::semantic_token_legend`]. The
    /// LSP adapter sorts tokens by source position, splits multi-line spans at
    /// line breaks, and ignores malformed, unknown, or overlapping spans.
    fn semantic_tokens(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
    ) -> Vec<SemanticToken> {
        Vec::new()
    }

    /// Formats one complete document using the editor's requested options.
    ///
    /// Return `None` when the document needs no edit. Implementations own all
    /// language-specific formatting decisions; Langmeld returns the result as
    /// a single full-document edit.
    fn format_document(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _options: &FormattingOptions,
    ) -> Option<String> {
        None
    }

    /// Returns host-defined information for an editor hover request.
    ///
    /// The position is a UTF-8 byte offset in the source snapshot. Optional
    /// hover ranges also use UTF-8 byte offsets and are validated by the LSP
    /// adapter before they are returned to the editor.
    fn hover(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
    ) -> Option<HoverInfo> {
        None
    }

    /// Resolves every source span to replace when renaming the symbol at an
    /// offset. Return `None` when the position is not a renameable symbol or
    /// the requested name is invalid for the language.
    ///
    /// Returned locations must refer to the symbol declaration and all of its
    /// references. The adapter applies the new name to every location or
    /// rejects the complete edit if any location is stale, unavailable, or
    /// malformed.
    fn rename(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
        _new_name: &str,
    ) -> Option<Vec<SourceLocation>> {
        None
    }

    /// Returns hierarchical document outline symbols for this source snapshot.
    ///
    /// Ranges and selection ranges use UTF-8 byte offsets. Their source
    /// bounds, nesting, and conversion to the negotiated LSP position encoding
    /// are validated by the adapter. Invalid symbols are omitted while valid
    /// siblings remain available to the editor.
    fn document_symbols(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
    ) -> Vec<DocumentSymbolInfo> {
        Vec::new()
    }
}

/// Backend for a language described by a validated declarative grammar.
#[derive(Clone, Debug)]
pub struct DeclarativeBackend {
    language: LanguageSpec,
}

impl DeclarativeBackend {
    /// Creates a backend for `language`.
    #[must_use]
    pub fn new(language: LanguageSpec) -> Self {
        Self { language }
    }
}

impl LanguageBackend for DeclarativeBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = parse(source, &self.language);
        BackendDocument::new(source, parsed.clone(), parsed.diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        complete_at(source, &self.language, offset)
    }
}

/// Backend for editing Langmeld's own `.lm` language definitions.
#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageDefinitionBackend;

impl LanguageBackend for LanguageDefinitionBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = parse(source, language_definition_grammar());
        BackendDocument::new(source, parsed.clone(), parsed.diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            formatting: true,
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        complete_at(source, language_definition_grammar(), offset)
    }

    fn format_document(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        options: &FormattingOptions,
    ) -> Option<String> {
        format_language_definition(
            source,
            DefinitionFormatOptions {
                tab_size: options.tab_size,
                insert_spaces: options.insert_spaces,
            },
        )
        .ok()
        .filter(|formatted| formatted != &source.text)
    }
}
