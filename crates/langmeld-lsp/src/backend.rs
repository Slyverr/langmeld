use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, SourceLocation};
use langmeld_language::{CompletionCandidate, LanguageSpec, ParseResult, complete_at, parse};

use crate::semantic::{SemanticToken, SemanticTokenLegend};

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
