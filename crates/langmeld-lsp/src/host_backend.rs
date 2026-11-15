use std::collections::BTreeMap;

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageWorkspaceAnalyzer, ParsedDocument, complete_with, definition_at, references_at,
};
use lsp_types::{CodeActionParams, FormattingOptions};
use serde_json::Value;

use crate::{
    BackendCapabilities, BackendDocument, BackendSource, BackendWorkspaceDiagnostics,
    CodeActionInfo, DocumentSymbolInfo, HoverInfo, LanguageBackend, LanguageServerInfo,
    RenamePreparation, SemanticToken, SemanticTokenLegend, WorkspaceSymbolInfo,
};

const PARSER_SOURCE_MISMATCH_CODE: &str = "LM-HOST-001";
const ANALYZER_SOURCE_MISMATCH_CODE: &str = "LM-HOST-002";
const WORKSPACE_ANALYZER_RESULT_CODE: &str = "LM-HOST-003";

type SourceResolver<'backend> = dyn Fn(&SourceLocation) -> Option<BackendSource> + 'backend;
type WorkspaceSymbolProvider<'backend> = dyn Fn(&str) -> Vec<WorkspaceSymbolInfo> + 'backend;
type WorkspaceDiagnosticsProvider<'backend> =
    dyn Fn(&[SourceFile]) -> Option<Vec<BackendWorkspaceDiagnostics>> + 'backend;
type DocumentCloseHandler<'backend> = dyn Fn(&SourceFile) + 'backend;
type Formatter<'backend, Tree, Analysis> = dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>, &FormattingOptions) -> Option<String>
    + 'backend;
type HoverProvider<'backend, Tree, Analysis> =
    dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>, usize) -> Option<HoverInfo> + 'backend;
type SemanticTokenProvider<'backend, Tree, Analysis> =
    dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>) -> Vec<SemanticToken> + 'backend;
type DocumentSymbolProvider<'backend, Tree, Analysis> =
    dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>) -> Vec<DocumentSymbolInfo> + 'backend;
type CodeActionProvider<'backend, Tree, Analysis> = dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>, &CodeActionParams, Span) -> Vec<CodeActionInfo>
    + 'backend;
type RenameProvider<'backend, Tree, Analysis> = dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>, usize, &str) -> Option<Vec<SourceLocation>>
    + 'backend;
type PrepareRenameProvider<'backend, Tree, Analysis> = dyn Fn(&SourceFile, &HostDocument<Tree, Analysis>, usize) -> Option<RenamePreparation>
    + 'backend;
type ExecuteCommandHandler<'backend> = dyn Fn(&[Value]) -> Result<Option<Value>, String> + 'backend;

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
        document_source: SourceIdentity,
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
/// Workspace analysis is available through [`Self::with_workspace_analyzer`].
/// Workspace-analysis batches with an invalid result count or source identity
/// are diagnosed and fall back to per-source analysis.
/// The backend and its providers may borrow single-threaded host state; this
/// adapter invokes them synchronously on the LSP connection thread.
/// Optional providers attach formatting, hover, semantic tokens, document
/// symbols, code actions, and rename without implementing [`LanguageBackend`]
/// directly. Server commands can be registered with
/// [`Self::with_execute_command`]. Hosts that need additional protocol or
/// workspace behavior can still implement [`LanguageBackend`] themselves.
pub struct HostLanguageBackend<'backend, P, A = NoAnalyzer>
where
    P: LanguageParser + 'backend,
    A: LanguageAnalyzer<P> + 'backend,
{
    parser: P,
    analyzer: A,
    workspace_analyzer: Option<Box<dyn LanguageWorkspaceAnalyzer<P, A> + 'backend>>,
    completer: Option<Box<dyn LanguageCompleter<P, Analysis = A::Analysis> + 'backend>>,
    navigator: Option<Box<dyn LanguageNavigator<P, Analysis = A::Analysis> + 'backend>>,
    source_resolver: Option<Box<SourceResolver<'backend>>>,
    workspace_symbol_provider: Option<Box<WorkspaceSymbolProvider<'backend>>>,
    workspace_diagnostics_provider: Option<Box<WorkspaceDiagnosticsProvider<'backend>>>,
    document_close_handler: Option<Box<DocumentCloseHandler<'backend>>>,
    formatter: Option<Box<Formatter<'backend, P::Tree, A::Analysis>>>,
    hover_provider: Option<Box<HoverProvider<'backend, P::Tree, A::Analysis>>>,
    semantic_token_legend: Option<SemanticTokenLegend>,
    semantic_token_provider: Option<Box<SemanticTokenProvider<'backend, P::Tree, A::Analysis>>>,
    document_symbol_provider: Option<Box<DocumentSymbolProvider<'backend, P::Tree, A::Analysis>>>,
    code_action_provider: Option<Box<CodeActionProvider<'backend, P::Tree, A::Analysis>>>,
    rename_provider: Option<Box<RenameProvider<'backend, P::Tree, A::Analysis>>>,
    prepare_rename_provider: Option<Box<PrepareRenameProvider<'backend, P::Tree, A::Analysis>>>,
    execute_command_handlers: BTreeMap<String, Box<ExecuteCommandHandler<'backend>>>,
    server_info: LanguageServerInfo,
}

impl<'backend, P> HostLanguageBackend<'backend, P, NoAnalyzer>
where
    P: LanguageParser + 'backend,
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
            workspace_diagnostics_provider: None,
            document_close_handler: None,
            formatter: None,
            hover_provider: None,
            semantic_token_legend: None,
            semantic_token_provider: None,
            document_symbol_provider: None,
            code_action_provider: None,
            rename_provider: None,
            prepare_rename_provider: None,
            execute_command_handlers: BTreeMap::new(),
            server_info: LanguageServerInfo::default(),
        }
    }
}

impl<'backend, P, A> HostLanguageBackend<'backend, P, A>
where
    P: LanguageParser + 'backend,
    A: LanguageAnalyzer<P> + 'backend,
{
    /// Sets the name and version shown by the editor for this server.
    #[must_use]
    pub fn with_server_info(mut self, server_info: LanguageServerInfo) -> Self {
        self.server_info = server_info;
        self
    }

    /// Creates a backend with host-defined semantic analysis.
    ///
    /// Choose the analyzer before attaching completion, navigation, or
    /// workspace-analysis providers so those providers are checked against
    /// the correct analysis type.
    #[must_use]
    pub fn new_with_analyzer(parser: P, analyzer: A) -> Self {
        Self {
            parser,
            analyzer,
            workspace_analyzer: None,
            completer: None,
            navigator: None,
            source_resolver: None,
            workspace_symbol_provider: None,
            workspace_diagnostics_provider: None,
            document_close_handler: None,
            formatter: None,
            hover_provider: None,
            semantic_token_legend: None,
            semantic_token_provider: None,
            document_symbol_provider: None,
            code_action_provider: None,
            rename_provider: None,
            prepare_rename_provider: None,
            execute_command_handlers: BTreeMap::new(),
            server_info: LanguageServerInfo::default(),
        }
    }

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

    fn analyze_individually(
        &self,
        sources: &[SourceFile],
        parsed: Vec<ParsedDocument<P::Tree>>,
        workspace_diagnostic: Option<&str>,
    ) -> Vec<BackendDocument<HostDocument<P::Tree, A::Analysis>>> {
        sources
            .iter()
            .zip(parsed)
            .map(|(source, parsed)| {
                let document = self.analyze_parsed(source, parsed);
                let Some(message) = workspace_diagnostic else {
                    return document;
                };

                let parts = document.into_parts();
                let mut diagnostics = parts.diagnostics;
                diagnostics.push(workspace_analysis_diagnostic(source, message));

                BackendDocument::new(source, parts.state, diagnostics)
            })
            .collect()
    }

    /// Adds semantic analysis across the currently open parsed documents.
    ///
    /// If the workspace analyzer returns a different number of results or
    /// associates a result with the wrong source snapshot, the batch is
    /// discarded, an `LM-HOST-003` diagnostic is added to each returned
    /// document, and per-source analysis is used instead.
    /// For a valid batch, the workspace analyzer's results replace the
    /// per-source analyzer's results. Call the supplied per-source analyzer
    /// from the workspace analyzer when its checks should also run.
    #[must_use]
    pub fn with_workspace_analyzer<W>(mut self, analyzer: W) -> Self
    where
        W: LanguageWorkspaceAnalyzer<P, A> + 'backend,
    {
        self.workspace_analyzer = Some(Box::new(analyzer));
        self
    }

    /// Adds a provider for source-located diagnostics in closed workspace files.
    ///
    /// The callback runs after initialization and document or workspace
    /// refreshes. It receives the current snapshots for open documents so it
    /// can analyze unsaved edits alongside closed files. Return `Some` with
    /// the complete findings set after a successful analysis; files omitted
    /// from it have stale findings cleared. Return `None` when analysis is
    /// unavailable so existing findings remain.
    #[must_use]
    pub fn with_workspace_diagnostics(
        mut self,
        provider: impl Fn(&[SourceFile]) -> Option<Vec<BackendWorkspaceDiagnostics>> + 'backend,
    ) -> Self {
        self.workspace_diagnostics_provider = Some(Box::new(provider));
        self
    }

    /// Adds completion using the retained parse and analysis results.
    #[must_use]
    pub fn with_completer<C>(mut self, completer: C) -> Self
    where
        C: LanguageCompleter<P, Analysis = A::Analysis> + 'backend,
    {
        self.completer = Some(Box::new(completer));
        self
    }

    /// Adds definition and references queries using retained parse and analysis results.
    #[must_use]
    pub fn with_navigator<N>(mut self, navigator: N) -> Self
    where
        N: LanguageNavigator<P, Analysis = A::Analysis> + 'backend,
    {
        self.navigator = Some(Box::new(navigator));
        self
    }

    /// Adds a resolver for navigation targets in closed project files.
    #[must_use]
    pub fn with_source_resolver(
        mut self,
        resolver: impl Fn(&SourceLocation) -> Option<BackendSource> + 'backend,
    ) -> Self {
        self.source_resolver = Some(Box::new(resolver));
        self
    }

    /// Adds project-wide symbol search using a host-owned index.
    #[must_use]
    pub fn with_workspace_symbol_provider(
        mut self,
        provider: impl Fn(&str) -> Vec<WorkspaceSymbolInfo> + 'backend,
    ) -> Self {
        self.workspace_symbol_provider = Some(Box::new(provider));
        self
    }

    /// Registers a callback for removing or refreshing an editor-buffer entry
    /// when its document closes.
    #[must_use]
    pub fn with_document_close_handler(mut self, handler: impl Fn(&SourceFile) + 'backend) -> Self {
        self.document_close_handler = Some(Box::new(handler));
        self
    }

    /// Adds host-defined formatting using the retained parse and analysis results.
    #[must_use]
    pub fn with_formatter(
        mut self,
        formatter: impl Fn(
            &SourceFile,
            &HostDocument<P::Tree, A::Analysis>,
            &FormattingOptions,
        ) -> Option<String>
        + 'backend,
    ) -> Self {
        self.formatter = Some(Box::new(formatter));
        self
    }

    /// Adds host-defined hover information using the retained parse and analysis results.
    #[must_use]
    pub fn with_hover_provider(
        mut self,
        provider: impl Fn(&SourceFile, &HostDocument<P::Tree, A::Analysis>, usize) -> Option<HoverInfo>
        + 'backend,
    ) -> Self {
        self.hover_provider = Some(Box::new(provider));
        self
    }

    /// Adds host-classified semantic tokens and the legend advertised to editors.
    #[must_use]
    pub fn with_semantic_tokens_provider(
        mut self,
        legend: SemanticTokenLegend,
        provider: impl Fn(&SourceFile, &HostDocument<P::Tree, A::Analysis>) -> Vec<SemanticToken>
        + 'backend,
    ) -> Self {
        self.semantic_token_legend = Some(legend);
        self.semantic_token_provider = Some(Box::new(provider));
        self
    }

    /// Adds document outline symbols using the retained parse and analysis results.
    #[must_use]
    pub fn with_document_symbol_provider(
        mut self,
        provider: impl Fn(&SourceFile, &HostDocument<P::Tree, A::Analysis>) -> Vec<DocumentSymbolInfo>
        + 'backend,
    ) -> Self {
        self.document_symbol_provider = Some(Box::new(provider));
        self
    }

    /// Adds code actions using the retained parse and analysis results.
    #[must_use]
    pub fn with_code_action_provider(
        mut self,
        provider: impl Fn(
            &SourceFile,
            &HostDocument<P::Tree, A::Analysis>,
            &CodeActionParams,
            Span,
        ) -> Vec<CodeActionInfo>
        + 'backend,
    ) -> Self {
        self.code_action_provider = Some(Box::new(provider));
        self
    }

    /// Adds symbol rename edits using the retained parse and analysis results.
    #[must_use]
    pub fn with_rename_provider(
        mut self,
        provider: impl Fn(
            &SourceFile,
            &HostDocument<P::Tree, A::Analysis>,
            usize,
            &str,
        ) -> Option<Vec<SourceLocation>>
        + 'backend,
    ) -> Self {
        self.rename_provider = Some(Box::new(provider));
        self
    }

    /// Adds validation and selection information before a rename request.
    #[must_use]
    pub fn with_prepare_rename_provider(
        mut self,
        provider: impl Fn(
            &SourceFile,
            &HostDocument<P::Tree, A::Analysis>,
            usize,
        ) -> Option<RenamePreparation>
        + 'backend,
    ) -> Self {
        self.prepare_rename_provider = Some(Box::new(provider));
        self
    }

    /// Registers a server command that can be invoked by code actions or
    /// `workspace/executeCommand` requests.
    ///
    /// Registered command identifiers are advertised automatically. Registering
    /// the same identifier again replaces its previous handler.
    #[must_use]
    pub fn with_execute_command(
        mut self,
        command: impl Into<String>,
        handler: impl Fn(&[Value]) -> Result<Option<Value>, String> + 'backend,
    ) -> Self {
        self.execute_command_handlers
            .insert(command.into(), Box::new(handler));
        self
    }

    fn host_document<'document>(
        source: &SourceFile,
        document: &'document BackendDocument<HostDocument<P::Tree, A::Analysis>>,
    ) -> Option<&'document HostDocument<P::Tree, A::Analysis>> {
        document.matches_source(source).then(|| document.state())
    }
}

impl<'backend, P, A> LanguageBackend for HostLanguageBackend<'backend, P, A>
where
    P: LanguageParser + 'backend,
    A: LanguageAnalyzer<P> + 'backend,
{
    type Document = HostDocument<P::Tree, A::Analysis>;

    fn server_info(&self) -> LanguageServerInfo {
        self.server_info.clone()
    }

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
            return Some(self.analyze_individually(sources, parsed, None));
        }
        let documents = sources.iter().zip(&parsed).collect::<Vec<_>>();
        let analyses = workspace_analyzer.analyze_workspace(&self.analyzer, &documents);
        let invalid_batch = if analyses.len() != sources.len() {
            Some(format!(
                "host workspace analyzer returned {} results for {} documents; batch results were discarded and per-source analysis was used",
                analyses.len(),
                sources.len(),
            ))
        } else {
            analyses
                .iter()
                .zip(sources)
                .position(|(analysis, source)| analysis.source_identity() != source.identity())
                .map(|index| {
                    format!(
                        "host workspace analyzer returned a result for a different source snapshot at document {}; batch results were discarded and per-source analysis was used",
                        index + 1,
                    )
                })
        };

        if let Some(message) = invalid_batch {
            return Some(self.analyze_individually(sources, parsed, Some(&message)));
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

    fn workspace_diagnostics(
        &self,
        open_sources: &[SourceFile],
    ) -> Option<Vec<BackendWorkspaceDiagnostics>> {
        self.workspace_diagnostics_provider
            .as_ref()
            .and_then(|provider| provider(open_sources))
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: self.completer.is_some(),
            definition: self.navigator.is_some(),
            references: self.navigator.is_some(),
            semantic_tokens: self.semantic_token_provider.is_some(),
            formatting: self.formatter.is_some(),
            hover: self.hover_provider.is_some(),
            code_actions: self.code_action_provider.is_some(),
            rename: self.rename_provider.is_some(),
            prepare_rename: self.rename_provider.is_some()
                && self.prepare_rename_provider.is_some(),
            document_symbols: self.document_symbol_provider.is_some(),
            workspace_symbols: self.workspace_symbol_provider.is_some(),
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

    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        self.semantic_token_legend.clone().unwrap_or_default()
    }

    fn semantic_tokens(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<SemanticToken> {
        let Some(host_document) = Self::host_document(source, document) else {
            return Vec::new();
        };

        self.semantic_token_provider
            .as_ref()
            .map_or_else(Vec::new, |provider| provider(source, host_document))
    }

    fn format_document(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        options: &FormattingOptions,
    ) -> Option<String> {
        let host_document = Self::host_document(source, document)?;
        let formatter = self.formatter.as_ref()?;

        formatter(source, host_document, options)
    }

    fn hover(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Option<HoverInfo> {
        let host_document = Self::host_document(source, document)?;
        let provider = self.hover_provider.as_ref()?;

        provider(source, host_document, offset)
    }

    fn code_actions(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        params: &CodeActionParams,
        range: Span,
    ) -> Vec<CodeActionInfo> {
        let Some(host_document) = Self::host_document(source, document) else {
            return Vec::new();
        };

        self.code_action_provider
            .as_ref()
            .map_or_else(Vec::new, |provider| {
                provider(source, host_document, params, range)
            })
    }

    fn rename(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        new_name: &str,
    ) -> Option<Vec<SourceLocation>> {
        let host_document = Self::host_document(source, document)?;
        let provider = self.rename_provider.as_ref()?;

        provider(source, host_document, offset, new_name)
    }

    fn prepare_rename(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Option<RenamePreparation> {
        let host_document = Self::host_document(source, document)?;
        let provider = self.prepare_rename_provider.as_ref()?;

        provider(source, host_document, offset)
    }

    fn document_symbols(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<DocumentSymbolInfo> {
        let Some(host_document) = Self::host_document(source, document) else {
            return Vec::new();
        };

        self.document_symbol_provider
            .as_ref()
            .map_or_else(Vec::new, |provider| provider(source, host_document))
    }

    fn execute_command_ids(&self) -> Vec<String> {
        self.execute_command_handlers.keys().cloned().collect()
    }

    fn execute_command(&self, command: &str, arguments: &[Value]) -> Result<Option<Value>, String> {
        self.execute_command_handlers
            .get(command)
            .ok_or_else(|| format!("unsupported command `{command}`"))?(arguments)
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

fn workspace_analysis_diagnostic(source: &SourceFile, message: &str) -> Diagnostic {
    Diagnostic::error(
        WORKSPACE_ANALYZER_RESULT_CODE,
        message,
        source,
        Span::new(0, source.text().len()),
    )
}
