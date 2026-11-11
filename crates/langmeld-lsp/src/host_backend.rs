use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageWorkspaceAnalyzer, ParsedDocument, complete_with, definition_at, references_at,
};

use crate::{
    BackendCapabilities, BackendDocument, BackendSource, LanguageBackend, WorkspaceSymbolInfo,
};

const PARSER_SOURCE_MISMATCH_CODE: &str = "LM-HOST-001";
const ANALYZER_SOURCE_MISMATCH_CODE: &str = "LM-HOST-002";

type SourceResolver = dyn Fn(&SourceLocation) -> Option<BackendSource> + Send + Sync;
type WorkspaceSymbolProvider = dyn Fn(&str) -> Vec<WorkspaceSymbolInfo> + Send + Sync;
type DocumentCloseHandler = dyn Fn(&SourceFile) + Send + Sync;

/// Validated parse and analysis results produced by a [`HostLanguageBackend`].
/// Results tied to another source snapshot are discarded and exposed as `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostDocument<Tree, Analysis> {
    /// Validated host-owned parse tree and parser diagnostics.
    parsed: Option<ParsedDocument<Tree>>,
    /// Validated host-owned semantic result and diagnostics.
    analysis: Option<AnalysisDocument<Analysis>>,
}

impl<Tree, Analysis> HostDocument<Tree, Analysis> {
    /// Returns the parsed syntax tree and parser diagnostics when the parser
    /// associated the result with the requested source snapshot.
    #[must_use]
    pub const fn parsed(&self) -> Option<&ParsedDocument<Tree>> {
        self.parsed.as_ref()
    }

    /// Returns the semantic result and analyzer diagnostics when analysis
    /// associated the result with the requested source snapshot.
    #[must_use]
    pub const fn analysis(&self) -> Option<&AnalysisDocument<Analysis>> {
        self.analysis.as_ref()
    }

    fn results_for(
        &self,
        document_source: Option<SourceIdentity>,
        source: &SourceFile,
    ) -> Option<(&ParsedDocument<Tree>, &AnalysisDocument<Analysis>)> {
        let source_identity = source.identity();

        if document_source != source_identity {
            return None;
        }

        let parsed = self.parsed.as_ref()?;
        let analysis = self.analysis.as_ref()?;

        (parsed.source_identity() == source_identity
            && analysis.source_identity() == source_identity)
            .then_some((parsed, analysis))
    }
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
/// Diagnostics from source-matched parser and analysis results are published.
/// Completion and navigation capabilities are enabled only when their
/// providers are attached. Parser and analyzer results with identities that
/// differ from the input are diagnosed and discarded. Editor queries are
/// suppressed unless both validated results and the enclosing backend document
/// belong to the requested source snapshot.
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
    source_resolver: Option<Box<SourceResolver>>,
    workspace_symbol_provider: Option<Box<WorkspaceSymbolProvider>>,
    document_close_handler: Option<Box<DocumentCloseHandler>>,
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
            source_resolver: None,
            workspace_symbol_provider: None,
            document_close_handler: None,
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
            source_resolver: self.source_resolver,
            workspace_symbol_provider: self.workspace_symbol_provider,
            document_close_handler: self.document_close_handler,
        }
    }
}

impl<P, A> HostLanguageBackend<P, A>
where
    P: LanguageParser,
    A: LanguageAnalyzer<P>,
{
    fn analyze_parsed(
        &self,
        source: &SourceFile,
        parsed: ParsedDocument<P::Tree>,
    ) -> BackendDocument<HostDocument<P::Tree, A::Analysis>> {
        if parsed.source_identity() != source.identity() {
            return BackendDocument::new(
                source,
                HostDocument {
                    parsed: None,
                    analysis: None,
                },
                vec![source_identity_diagnostic(
                    source,
                    PARSER_SOURCE_MISMATCH_CODE,
                    "parser",
                )],
            );
        }

        let mut diagnostics = parsed.diagnostics().to_vec();
        let analysis = self.analyzer.analyze(source, &parsed);
        if analysis.source_identity() != source.identity() {
            diagnostics.push(source_identity_diagnostic(
                source,
                ANALYZER_SOURCE_MISMATCH_CODE,
                "analyzer",
            ));
        } else {
            diagnostics.extend(analysis.diagnostics().iter().cloned());
        }

        let analysis = (analysis.source_identity() == source.identity()).then_some(analysis);

        BackendDocument::new(
            source,
            HostDocument {
                parsed: Some(parsed),
                analysis,
            },
            diagnostics,
        )
    }

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

    /// Adds a resolver for navigation targets in closed project files.
    #[must_use]
    pub fn with_source_resolver(
        mut self,
        resolver: impl Fn(&SourceLocation) -> Option<BackendSource> + Send + Sync + 'static,
    ) -> Self {
        self.source_resolver = Some(Box::new(resolver));
        self
    }

    /// Adds project-wide symbol search using a host-owned index.
    #[must_use]
    pub fn with_workspace_symbol_provider(
        mut self,
        provider: impl Fn(&str) -> Vec<WorkspaceSymbolInfo> + Send + Sync + 'static,
    ) -> Self {
        self.workspace_symbol_provider = Some(Box::new(provider));
        self
    }

    /// Registers a callback for removing or refreshing an editor-buffer entry
    /// when its document closes.
    #[must_use]
    pub fn with_document_close_handler(
        mut self,
        handler: impl Fn(&SourceFile) + Send + Sync + 'static,
    ) -> Self {
        self.document_close_handler = Some(Box::new(handler));
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
        self.analyze_parsed(source, self.parser.parse(source))
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
            .any(|(parsed, source)| parsed.source_identity() != source.identity())
        {
            return Some(
                sources
                    .iter()
                    .zip(parsed)
                    .map(|(source, parsed)| self.analyze_parsed(source, parsed))
                    .collect(),
            );
        }
        let documents = sources.iter().zip(&parsed).collect::<Vec<_>>();
        let analyses = workspace_analyzer.analyze_workspace(&self.analyzer, &documents);
        if analyses.len() != sources.len()
            || analyses
                .iter()
                .zip(sources)
                .any(|(analysis, source)| analysis.source_identity() != source.identity())
        {
            return Some(
                sources
                    .iter()
                    .zip(parsed)
                    .map(|(source, parsed)| self.analyze_parsed(source, parsed))
                    .collect(),
            );
        }

        Some(
            sources
                .iter()
                .zip(parsed)
                .zip(analyses)
                .map(|((source, parsed), analysis)| {
                    let diagnostics = parsed
                        .diagnostics()
                        .iter()
                        .chain(analysis.diagnostics())
                        .cloned()
                        .collect();
                    BackendDocument::new(
                        source,
                        HostDocument {
                            parsed: Some(parsed),
                            analysis: Some(analysis),
                        },
                        diagnostics,
                    )
                })
                .collect(),
        )
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: self.completer.is_some(),
            definition: self.navigator.is_some(),
            references: self.navigator.is_some(),
            workspace_symbols: self.workspace_symbol_provider.is_some(),
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<langmeld_language::CompletionCandidate> {
        let Some((parsed, analysis)) = document
            .state()
            .results_for(document.source_identity(), source)
        else {
            return Vec::new();
        };

        self.completer.as_ref().map_or_else(Vec::new, |completer| {
            complete_with::<P, _>(source, parsed, Some(analysis), offset, completer.as_ref())
        })
    }

    fn definition(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        let Some((parsed, analysis)) = document
            .state()
            .results_for(document.source_identity(), source)
        else {
            return Vec::new();
        };

        self.navigator.as_ref().map_or_else(Vec::new, |navigator| {
            definition_at::<P, _>(source, parsed, Some(analysis), offset, navigator.as_ref())
        })
    }

    fn references(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        let Some((parsed, analysis)) = document
            .state()
            .results_for(document.source_identity(), source)
        else {
            return Vec::new();
        };

        self.navigator.as_ref().map_or_else(Vec::new, |navigator| {
            references_at::<P, _>(
                source,
                parsed,
                Some(analysis),
                offset,
                include_declaration,
                navigator.as_ref(),
            )
        })
    }

    fn source_for_location(&self, location: &SourceLocation) -> Option<BackendSource> {
        self.source_resolver.as_ref()?(location)
    }

    fn did_close(&self, source: &SourceFile, _document: &BackendDocument<Self::Document>) {
        if let Some(handler) = &self.document_close_handler {
            handler(source);
        }
    }

    fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbolInfo> {
        self.workspace_symbol_provider
            .as_ref()
            .map_or_else(Vec::new, |provider| provider(query))
    }
}

fn source_identity_diagnostic(source: &SourceFile, code: &str, component: &str) -> Diagnostic {
    Diagnostic::error(
        code,
        format!("host {component} returned data for a different source snapshot"),
        source,
        Span::new(0, source.text().len()),
    )
}
