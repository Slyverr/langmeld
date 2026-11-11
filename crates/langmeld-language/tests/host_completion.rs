use langmeld_core::{SourceDatabase, SourceFile, Span};
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, LanguageCompleter, LanguageParser, ParsedDocument,
    complete_with,
};

struct Parser;

impl LanguageParser for Parser {
    type Tree = String;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument::new(source, source.text().to_owned(), Vec::new())
    }
}

struct Completer;

impl LanguageCompleter<Parser> for Completer {
    type Analysis = usize;

    fn complete(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<String>,
        analysis: Option<&AnalysisDocument<usize>>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        assert_eq!(parsed.tree(), source.text());
        assert!(offset <= source.text().len());
        assert_eq!(analysis.map(|document| *document.result()), Some(3));
        vec![
            CompletionCandidate::new("suggestion", Span::new(0, 0)).with_detail("candidate detail"),
        ]
    }
}

#[test]
fn host_completion_receives_the_existing_parse_and_analysis() {
    let source = SourceFile::new("main.test", "abc");
    let parsed = Parser.parse(&source);
    let analysis = AnalysisDocument::new(&source, 3, Vec::new());

    let candidates = complete_with(&source, &parsed, Some(&analysis), 3, &Completer);

    assert_eq!(candidates[0].text, "suggestion");
    assert_eq!(candidates[0].detail.as_deref(), Some("candidate detail"));
}

#[test]
fn host_completion_can_run_without_semantic_analysis() {
    struct SyntaxOnlyCompleter;

    impl LanguageCompleter<Parser> for SyntaxOnlyCompleter {
        type Analysis = ();

        fn complete(
            &self,
            _source: &SourceFile,
            _parsed: &ParsedDocument<String>,
            analysis: Option<&AnalysisDocument<()>>,
            _offset: usize,
        ) -> Vec<CompletionCandidate> {
            assert!(analysis.is_none());
            Vec::new()
        }
    }

    let source = SourceFile::new("main.test", "abc");
    let parsed = Parser.parse(&source);

    assert!(complete_with::<Parser, _>(&source, &parsed, None, 3, &SyntaxOnlyCompleter).is_empty());
}

#[test]
fn stale_parse_and_analysis_snapshots_are_rejected() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main.test", "abc").unwrap();
    let old_source = sources.get(id).unwrap().clone();
    let old_analysis = AnalysisDocument::new(&old_source, 3, Vec::new());
    sources.update(id, "updated").unwrap();
    let current_source = sources.get(id).unwrap();
    let current_parse = Parser.parse(current_source);

    assert!(
        complete_with(
            current_source,
            &current_parse,
            Some(&old_analysis),
            current_source.text().len(),
            &Completer,
        )
        .is_empty()
    );

    let old_parse = Parser.parse(&old_source);
    assert!(
        complete_with(
            current_source,
            &old_parse,
            None,
            current_source.text().len(),
            &Completer,
        )
        .is_empty()
    );
}

#[test]
fn invalid_offsets_and_replacement_spans_are_rejected() {
    struct InvalidSpanCompleter;

    impl LanguageCompleter<Parser> for InvalidSpanCompleter {
        type Analysis = ();

        fn complete(
            &self,
            _source: &SourceFile,
            _parsed: &ParsedDocument<String>,
            _analysis: Option<&AnalysisDocument<()>>,
            _offset: usize,
        ) -> Vec<CompletionCandidate> {
            [
                Span::new(2, 1),
                Span::new(0, 9),
                Span::new(1, 1),
                Span::new(2, 2),
            ]
            .into_iter()
            .map(|replace| CompletionCandidate::new("suggestion", replace))
            .collect()
        }
    }

    let source = SourceFile::new("main.test", "éx");
    let parsed = Parser.parse(&source);
    let candidates = complete_with(
        &source,
        &parsed,
        None,
        source.text().len(),
        &InvalidSpanCompleter,
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].replace, Span::new(2, 2));
    assert!(complete_with(&source, &parsed, None, 1, &InvalidSpanCompleter,).is_empty());
}
