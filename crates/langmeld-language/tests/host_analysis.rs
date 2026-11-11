use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, LanguageParser, ParsedDocument};

struct Parser;

impl LanguageParser for Parser {
    type Tree = bool;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument::new(source, true, Vec::new())
    }
}

struct Analyzer;

impl LanguageAnalyzer<Parser> for Analyzer {
    type Analysis = usize;

    fn analyze(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<bool>,
    ) -> AnalysisDocument<Self::Analysis> {
        let diagnostics = if *parsed.tree() {
            Vec::new()
        } else {
            vec![Diagnostic::error(
                "TEST-SEM-001",
                "the parsed program is disabled",
                source,
                Span::new(0, source.text().len()),
            )]
        };
        AnalysisDocument::new(source, usize::from(*parsed.tree()), diagnostics)
    }
}

#[test]
fn analyzer_can_return_a_host_defined_result_for_a_parsed_tree() {
    let source = SourceFile::new("main.test", "enabled");
    let parsed = Parser.parse(&source);

    let analysis = Analyzer.analyze(&source, &parsed);

    assert_eq!(*analysis.result(), 1);
    assert_eq!(analysis.source_identity(), parsed.source_identity());
    assert!(analysis.is_valid());
}

#[test]
fn analysis_document_keeps_semantic_diagnostics() {
    let source = SourceFile::new("main.test", "disabled");
    let parsed = ParsedDocument::new(&source, false, Vec::new());

    let analysis = Analyzer.analyze(&source, &parsed);

    assert_eq!(analysis.diagnostics()[0].code, "TEST-SEM-001");
    assert!(!analysis.is_valid());
}

#[test]
fn analysis_document_can_be_consumed_without_separating_its_source_identity() {
    let source = SourceFile::new("main.test", "disabled");
    let source_identity = source.identity();
    let diagnostic = Diagnostic::error(
        "TEST-SEM-002",
        "example finding",
        &source,
        Span::new(0, source.text().len()),
    );
    let analysis = AnalysisDocument::new(&source, 7, vec![diagnostic]);

    let (analysis_source, result, diagnostics) = analysis.into_parts();

    assert_eq!(analysis_source, source_identity);
    assert_eq!(result, 7);
    assert_eq!(diagnostics[0].code, "TEST-SEM-002");
}
