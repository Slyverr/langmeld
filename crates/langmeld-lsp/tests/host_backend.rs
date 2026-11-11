use langmeld_core::{SourceDatabase, SourceFile, SourceIdentity, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, LanguageAnalyzer, LanguageCompleter, LanguageNavigator,
    LanguageParser, LanguageWorkspaceAnalyzer, ParsedDocument,
};
use langmeld_lsp::{BackendDocument, HostLanguageBackend, LanguageBackend};

struct TestParser {
    reported_source: Option<SourceFile>,
}

impl LanguageParser for TestParser {
    type Tree = ();

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let reported_source = self.reported_source.as_ref().unwrap_or(source);

        ParsedDocument::new(reported_source, (), Vec::new())
    }
}

struct TestAnalyzer {
    reported_source: Option<SourceFile>,
}

impl LanguageAnalyzer<TestParser> for TestAnalyzer {
    type Analysis = ();

    fn analyze(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<()>,
    ) -> AnalysisDocument<Self::Analysis> {
        let reported_source = self.reported_source.as_ref().unwrap_or(source);

        AnalysisDocument::new(reported_source, (), Vec::new())
    }
}

struct TestWorkspaceAnalyzer {
    reported_source: Option<SourceFile>,
    output_count: Option<usize>,
}

impl LanguageWorkspaceAnalyzer<TestParser, TestAnalyzer> for TestWorkspaceAnalyzer {
    fn analyze_workspace(
        &self,
        _analyzer: &TestAnalyzer,
        documents: &[(&SourceFile, &ParsedDocument<()>)],
    ) -> Vec<AnalysisDocument<()>> {
        documents
            .iter()
            .take(self.output_count.unwrap_or(documents.len()))
            .map(|(source, _)| {
                AnalysisDocument::new(
                    self.reported_source.as_ref().unwrap_or(source),
                    (),
                    Vec::new(),
                )
            })
            .collect()
    }
}

struct TestCompleter;

impl LanguageCompleter<TestParser> for TestCompleter {
    type Analysis = ();

    fn complete(
        &self,
        _source: &SourceFile,
        _parsed: &ParsedDocument<()>,
        _analysis: Option<&AnalysisDocument<()>>,
        _offset: usize,
    ) -> Vec<CompletionCandidate> {
        vec![CompletionCandidate::new("suggestion", Span::new(0, 0))]
    }
}

struct TestNavigator;

impl LanguageNavigator<TestParser> for TestNavigator {
    type Analysis = ();

    fn definition(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<()>,
        _analysis: Option<&AnalysisDocument<()>>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        vec![location(source)]
    }

    fn references(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<()>,
        _analysis: Option<&AnalysisDocument<()>>,
        _offset: usize,
        _include_declaration: bool,
    ) -> Vec<SourceLocation> {
        vec![location(source)]
    }
}

fn location(source: &SourceFile) -> SourceLocation {
    SourceLocation::new(source, Span::new(0, 0))
}

fn managed_source_and_other_identity() -> (SourceFile, SourceIdentity, SourceFile) {
    let mut database = SourceDatabase::default();
    let source_id = database
        .insert("main.lang", "source")
        .expect("source inserts");
    let other_id = database
        .insert("other.lang", "other")
        .expect("source inserts");
    let source = database.get(source_id).expect("source exists").clone();
    let other_source = database.get(other_id).expect("other source exists").clone();
    let other_identity = other_source
        .identity()
        .expect("managed source has an identity");

    (source, other_identity, other_source)
}

fn backend(
    parser_source: Option<SourceFile>,
    analyzer_source: Option<SourceFile>,
) -> HostLanguageBackend<TestParser, TestAnalyzer> {
    HostLanguageBackend::new(TestParser {
        reported_source: parser_source,
    })
    .with_analyzer(TestAnalyzer {
        reported_source: analyzer_source,
    })
    .with_completer(TestCompleter)
    .with_navigator(TestNavigator)
}

#[test]
fn parser_source_mismatch_is_diagnosed_and_suppresses_editor_queries() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = backend(Some(other_source), None);

    let document = backend.analyze(&source);

    assert!(
        document
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-HOST-001")
    );
    assert!(backend.complete(&source, &document, 0).is_empty());
    assert!(backend.definition(&source, &document, 0).is_empty());
    assert!(backend.references(&source, &document, 0, true).is_empty());
}

#[test]
fn analysis_source_mismatch_is_diagnosed_and_suppresses_editor_queries() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = backend(None, Some(other_source));

    let document = backend.analyze(&source);

    assert!(
        document
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-HOST-002")
    );
    assert!(backend.complete(&source, &document, 0).is_empty());
    assert!(backend.definition(&source, &document, 0).is_empty());
    assert!(backend.references(&source, &document, 0, true).is_empty());
}

#[test]
fn editor_queries_reject_a_source_different_from_the_analyzed_snapshot() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = backend(None, None);

    let document = backend.analyze(&source);

    assert!(!backend.complete(&source, &document, 0).is_empty());
    assert!(!backend.definition(&source, &document, 0).is_empty());
    assert!(!backend.references(&source, &document, 0, true).is_empty());

    assert!(backend.complete(&other_source, &document, 0).is_empty());
    assert!(backend.definition(&other_source, &document, 0).is_empty());
    assert!(
        backend
            .references(&other_source, &document, 0, true)
            .is_empty()
    );
}

#[test]
fn editor_queries_reject_a_document_with_a_different_outer_source_identity() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = backend(None, None);

    let document = backend.analyze(&source);
    let (_, state, _) = document.into_parts();
    let mismatched_document = BackendDocument::new(&other_source, state, Vec::new());

    assert!(
        backend
            .complete(&source, &mismatched_document, 0)
            .is_empty()
    );
    assert!(
        backend
            .definition(&source, &mismatched_document, 0)
            .is_empty()
    );
    assert!(
        backend
            .references(&source, &mismatched_document, 0, true)
            .is_empty()
    );
}

#[test]
fn invalid_workspace_results_fall_back_to_per_source_analysis() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: None,
    })
    .with_analyzer(TestAnalyzer {
        reported_source: None,
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: Some(other_source.clone()),
        output_count: None,
    });
    let sources = [source.clone(), other_source];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("configured workspace analysis returns a complete result");

    assert_eq!(documents.len(), sources.len());
    for (document, source) in documents.iter().zip(&sources) {
        assert_eq!(document.source_identity(), source.identity());
        assert_eq!(
            document.state().parsed().source_identity(),
            source.identity()
        );
        assert_eq!(
            document.state().analysis().source_identity(),
            source.identity()
        );
    }
}

#[test]
fn incomplete_workspace_results_fall_back_to_every_source() {
    let (source, _, other_source) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: None,
    })
    .with_analyzer(TestAnalyzer {
        reported_source: None,
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: None,
        output_count: Some(1),
    });
    let sources = [source.clone(), other_source];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("configured workspace analysis returns a complete result");

    assert_eq!(documents.len(), sources.len());
    for (document, source) in documents.iter().zip(&sources) {
        assert_eq!(document.source_identity(), source.identity());
        assert_eq!(
            document.state().parsed().source_identity(),
            source.identity()
        );
        assert_eq!(
            document.state().analysis().source_identity(),
            source.identity()
        );
    }
}

#[test]
fn mismatched_parser_results_keep_workspace_documents_in_sync() {
    let (source, other_identity, other_source) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: Some(other_source.clone()),
    })
    .with_analyzer(TestAnalyzer {
        reported_source: None,
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: None,
        output_count: None,
    });
    let sources = [source.clone(), other_source];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("parser identity failures still produce per-source documents");

    assert_eq!(documents.len(), sources.len());
    assert_eq!(documents[0].source_identity(), source.identity());
    assert_eq!(
        documents[0].state().parsed().source_identity(),
        Some(other_identity)
    );
    assert!(
        documents[0]
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-HOST-001")
    );
    assert_eq!(documents[1].source_identity(), Some(other_identity));
    assert_eq!(
        documents[1].state().parsed().source_identity(),
        Some(other_identity)
    );
}
