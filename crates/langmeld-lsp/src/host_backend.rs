use langmeld_core::SourceFile;
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, LanguageParser, ParsedDocument};

use crate::{BackendCapabilities, BackendDocument, LanguageBackend};

/// Retained parse and analysis results produced by a [`HostLanguageBackend`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostDocument<Tree, Analysis> {
    /// Host-owned parse tree and parser diagnostics.
    pub parsed: ParsedDocument<Tree>,
    /// Host-owned semantic result and diagnostics.
    pub analysis: AnalysisDocument<Analysis>,
}

/// No-op analyzer used when only parser diagnostics are needed.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoAnalyzer;

impl<P: LanguageParser> LanguageAnalyzer<P> for NoAnalyzer {
    type Analysis = ();

    fn analyze(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<P::Tree>,
    ) -> AnalysisDocument<Self::Analysis> {
        AnalysisDocument::new(source, (), Vec::new())
    }
}

/// Connects a host parser and optional analyzer to LSP diagnostics.
///
/// This adapter retains both results and publishes their diagnostics. It
/// advertises no completion, navigation, or other editor queries; implement
/// [`LanguageBackend`] directly when a language provides those features.
pub struct HostLanguageBackend<P, A = NoAnalyzer> {
    parser: P,
    analyzer: A,
}

impl<P> HostLanguageBackend<P, NoAnalyzer> {
    /// Creates a backend that publishes parser diagnostics only.
    #[must_use]
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            analyzer: NoAnalyzer,
        }
    }
}

impl<P, A> HostLanguageBackend<P, A> {
    /// Adds host semantic analysis to this backend.
    #[must_use]
    pub fn with_analyzer<Next>(self, analyzer: Next) -> HostLanguageBackend<P, Next> {
        HostLanguageBackend {
            parser: self.parser,
            analyzer,
        }
    }
}

impl<P, A> LanguageBackend for HostLanguageBackend<P, A>
where
    P: LanguageParser + Send + Sync + 'static,
    P::Tree: Send + Sync + 'static,
    A: LanguageAnalyzer<P> + Send + Sync + 'static,
    A::Analysis: Send + Sync + 'static,
{
    type Document = HostDocument<P::Tree, A::Analysis>;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = self.parser.parse(source);
        let analysis = self.analyzer.analyze(source, &parsed);
        let diagnostics = parsed
            .diagnostics
            .iter()
            .chain(&analysis.diagnostics)
            .cloned()
            .collect();
        BackendDocument::new(source, HostDocument { parsed, analysis }, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}
