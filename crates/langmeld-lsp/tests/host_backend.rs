use langmeld_core::{SourceDatabase, SourceFile, SourceIdentity, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, LanguageAnalyzer, LanguageCompleter, LanguageNavigator,
    LanguageParser, LanguageWorkspaceAnalyzer, ParsedDocument,
};
use langmeld_lsp::{BackendDocument, HostLanguageBackend, LanguageBackend};

struct TestParser {
    reported_source: Option<SourceIdentity>,
}

impl LanguageParser for TestParser {
    type Tree = ();

    fn parse(&self, _source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument {
            source: self.reported_source,
            tree: (),
            diagnostics: Vec::new(),
        }
    }
}

struct TestAnalyzer {
    reported_source: Option<SourceIdentity>,
}

impl LanguageAnalyzer<TestParser> for TestAnalyzer {
    type Analysis = ();

    fn analyze(
        &self,
        _source: &SourceFile,
        _parsed: &ParsedDocument<()>,
    ) -> AnalysisDocument<Self::Analysis> {
        AnalysisDocument {
            source: self.reported_source,
            result: (),
            diagnostics: Vec::new(),
        }
    }
}

struct TestWorkspaceAnalyzer {
    reported_source: Option<SourceIdentity>,
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
            .map(|_| AnalysisDocument {
                source: self.reported_source,
                result: (),
                diagnostics: Vec::new(),
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
    SourceLocation {
        source_name: source.name.clone(),
        identity: source.identity(),
        span: Span::new(0, 0),
    }
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
    parser_identity: Option<SourceIdentity>,
    analyzer_identity: Option<SourceIdentity>,
) -> HostLanguageBackend<TestParser, TestAnalyzer> {
    HostLanguageBackend::new(TestParser {
        reported_source: parser_identity,
    })
    .with_analyzer(TestAnalyzer {
        reported_source: analyzer_identity,
    })
    .with_completer(TestCompleter)
    .with_navigator(TestNavigator)
}

#[test]
fn parser_source_mismatch_is_diagnosed_and_suppresses_editor_queries() {
    let (source, other_identity, _) = managed_source_and_other_identity();
    let backend = backend(Some(other_identity), source.identity());

    let document = backend.analyze(&source);

    assert!(
        document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-HOST-001")
    );
    assert!(backend.complete(&source, &document, 0).is_empty());
    assert!(backend.definition(&source, &document, 0).is_empty());
    assert!(backend.references(&source, &document, 0, true).is_empty());
}

#[test]
fn analysis_source_mismatch_is_diagnosed_and_suppresses_editor_queries() {
    let (source, other_identity, _) = managed_source_and_other_identity();
    let backend = backend(source.identity(), Some(other_identity));

    let document = backend.analyze(&source);

    assert!(
        document
            .diagnostics
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
    let backend = backend(source.identity(), source.identity());

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
    let backend = backend(source.identity(), source.identity());

    let document = backend.analyze(&source);
    let mismatched_document = BackendDocument::new(&other_source, document.state, Vec::new());

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
    let (source, other_identity, _) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: source.identity(),
    })
    .with_analyzer(TestAnalyzer {
        reported_source: source.identity(),
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: Some(other_identity),
        output_count: None,
    });
    let sources = [source.clone(), source.clone()];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("configured workspace analysis returns a complete result");

    assert_eq!(documents.len(), sources.len());
    for (document, source) in documents.iter().zip(&sources) {
        assert_eq!(document.source, source.identity());
        assert_eq!(document.state.parsed.source, source.identity());
        assert_eq!(document.state.analysis.source, source.identity());
    }
}

#[test]
fn incomplete_workspace_results_fall_back_to_every_source() {
    let (source, _, _) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: source.identity(),
    })
    .with_analyzer(TestAnalyzer {
        reported_source: source.identity(),
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: source.identity(),
        output_count: Some(1),
    });
    let sources = [source.clone(), source.clone()];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("configured workspace analysis returns a complete result");

    assert_eq!(documents.len(), sources.len());
    for (document, source) in documents.iter().zip(&sources) {
        assert_eq!(document.source, source.identity());
        assert_eq!(document.state.parsed.source, source.identity());
        assert_eq!(document.state.analysis.source, source.identity());
    }
}

#[test]
fn mismatched_parser_results_keep_workspace_documents_in_sync() {
    let (source, other_identity, _) = managed_source_and_other_identity();
    let backend = HostLanguageBackend::new(TestParser {
        reported_source: Some(other_identity),
    })
    .with_analyzer(TestAnalyzer {
        reported_source: source.identity(),
    })
    .with_workspace_analyzer(TestWorkspaceAnalyzer {
        reported_source: source.identity(),
        output_count: None,
    });
    let sources = [source.clone(), source.clone()];

    let documents = backend
        .analyze_workspace(&sources)
        .expect("parser identity failures still produce per-source documents");

    assert_eq!(documents.len(), sources.len());
    for document in &documents {
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "LM-HOST-001")
        );
    }
}
