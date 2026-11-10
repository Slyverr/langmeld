use langmeld_core::{SourceFile, SourceLocation};
use langmeld_language::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageWorkspaceAnalyzer, ParsedDocument,
};

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

/// Connects host parser and analysis contracts to an LSP backend.
///
/// Parser and semantic diagnostics are always published. Completion and
/// navigation capabilities are enabled only when their providers are attached.
/// Workspace analysis, rename, formatting, hover, and other queries remain
/// available through a custom [`LanguageBackend`] implementation.
pub struct HostLanguageBackend<P, A = NoAnalyzer>
where
    P: LanguageParser,
    A: LanguageAnalyzer<P>,
{
    parser: P,
    analyzer: A,
    workspace_analyzer: Option<Box<dyn LanguageWorkspaceAnalyzer<P, A> + Send + Sync>>,
    completer: Option<Box<dyn LanguageCompleter<P, Analysis = A::Analysis> + Send + Sync>>,
    navigator: Option<Box<dyn LanguageNavigator<P, Analysis = A::Analysis> + Send + Sync>>,
}

impl<P> HostLanguageBackend<P, NoAnalyzer>
where
    P: LanguageParser,
{
    /// Creates a backend that publishes parser diagnostics only.
    #[must_use]
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            analyzer: NoAnalyzer,
            workspace_analyzer: None,
            completer: None,
            navigator: None,
        }
    }

    /// Adds host semantic analysis to this backend.
    #[must_use]
    pub fn with_analyzer<A>(self, analyzer: A) -> HostLanguageBackend<P, A>
    where
        A: LanguageAnalyzer<P>,
    {
        HostLanguageBackend {
            parser: self.parser,
            analyzer,
            workspace_analyzer: None,
            completer: None,
            navigator: None,
        }
    }
}

impl<P, A> HostLanguageBackend<P, A>
where
    P: LanguageParser,
    A: LanguageAnalyzer<P>,
{
    /// Adds semantic analysis across the currently open parsed documents.
    #[must_use]
    pub fn with_workspace_analyzer<W>(mut self, analyzer: W) -> Self
    where
        W: LanguageWorkspaceAnalyzer<P, A> + Send + Sync + 'static,
    {
        self.workspace_analyzer = Some(Box::new(analyzer));
        self
    }

    /// Adds completion using the retained parse and analysis results.
    #[must_use]
    pub fn with_completer<C>(mut self, completer: C) -> Self
    where
        C: LanguageCompleter<P, Analysis = A::Analysis> + Send + Sync + 'static,
    {
        self.completer = Some(Box::new(completer));
        self
    }

    /// Adds definition and references queries using retained parse and analysis results.
    #[must_use]
    pub fn with_navigator<N>(mut self, navigator: N) -> Self
    where
        N: LanguageNavigator<P, Analysis = A::Analysis> + Send + Sync + 'static,
    {
        self.navigator = Some(Box::new(navigator));
        self
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

    fn analyze_workspace(
        &self,
        sources: &[SourceFile],
    ) -> Option<Vec<BackendDocument<Self::Document>>> {
        let workspace_analyzer = self.workspace_analyzer.as_ref()?;
        let parsed = sources
            .iter()
            .map(|source| self.parser.parse(source))
            .collect::<Vec<_>>();
        if parsed
            .iter()
            .zip(sources)
            .any(|(parsed, source)| parsed.source != source.identity())
        {
            return None;
        }
        let documents = sources.iter().zip(&parsed).collect::<Vec<_>>();
        let analyses = workspace_analyzer.analyze_workspace(&self.analyzer, &documents);
        if analyses.len() != sources.len()
            || analyses
                .iter()
                .zip(sources)
                .any(|(analysis, source)| analysis.source != source.identity())
        {
            return None;
        }

        Some(
            sources
                .iter()
                .zip(parsed)
                .zip(analyses)
                .map(|((source, parsed), analysis)| {
                    let diagnostics = parsed
                        .diagnostics
                        .iter()
                        .chain(&analysis.diagnostics)
                        .cloned()
                        .collect();
                    BackendDocument::new(source, HostDocument { parsed, analysis }, diagnostics)
                })
                .collect(),
        )
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: self.completer.is_some(),
            definition: self.navigator.is_some(),
            references: self.navigator.is_some(),
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<langmeld_language::CompletionCandidate> {
        self.completer.as_ref().map_or_else(Vec::new, |completer| {
            completer.complete(
                source,
                &document.state.parsed,
                Some(&document.state.analysis),
                offset,
            )
        })
    }

    fn definition(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        self.navigator.as_ref().map_or_else(Vec::new, |navigator| {
            navigator.definition(
                source,
                &document.state.parsed,
                Some(&document.state.analysis),
                offset,
            )
        })
    }

    fn references(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        self.navigator.as_ref().map_or_else(Vec::new, |navigator| {
            navigator.references(
                source,
                &document.state.parsed,
                Some(&document.state.analysis),
                offset,
                include_declaration,
            )
        })
    }
}
