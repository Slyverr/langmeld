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
        let diagnostics = if parsed.tree {
            Vec::new()
        } else {
            vec![Diagnostic::error(
                "TEST-SEM-001",
                "the parsed program is disabled",
                source,
                Span::new(0, source.text().len()),
            )]
        };
        AnalysisDocument::new(source, usize::from(parsed.tree), diagnostics)
    }
}

#[test]
fn analyzer_can_return_a_host_defined_result_for_a_parsed_tree() {
    let source = SourceFile::new("main.test", "enabled");
    let parsed = Parser.parse(&source);

    let analysis = Analyzer.analyze(&source, &parsed);

    assert_eq!(analysis.result, 1);
    assert_eq!(analysis.source_identity(), parsed.source_identity());
    assert!(analysis.is_valid());
}

#[test]
fn analysis_document_keeps_semantic_diagnostics() {
    let source = SourceFile::new("main.test", "disabled");
    let parsed = ParsedDocument::new(&source, false, Vec::new());

    let analysis = Analyzer.analyze(&source, &parsed);

    assert_eq!(analysis.diagnostics[0].code, "TEST-SEM-001");
    assert!(!analysis.is_valid());
}
