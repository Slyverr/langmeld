use langmeld_core::{SourceDatabase, SourceFile, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, LanguageNavigator, LanguageParser, ParsedDocument, definition_at,
    references_at,
};

struct Parser;

impl LanguageParser for Parser {
    type Tree = String;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument::new(source, source.text().to_owned(), Vec::new())
    }
}

struct Navigator;

impl LanguageNavigator<Parser> for Navigator {
    type Analysis = ();

    fn definition(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<String>,
        analysis: Option<&AnalysisDocument<()>>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        assert!(analysis.is_some());
        assert_eq!(offset, source.text().len());
        vec![location(source, Span::new(0, 1))]
    }

    fn references(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<String>,
        _analysis: Option<&AnalysisDocument<()>>,
        _offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        if include_declaration {
            vec![location(source, Span::new(0, 1))]
        } else {
            Vec::new()
        }
    }
}

fn location(source: &SourceFile, span: Span) -> SourceLocation {
    SourceLocation::new(source, span)
}

#[test]
fn navigation_reuses_parse_and_analysis_for_definition_and_references() {
    let source = SourceFile::new("main.test", "x");
    let parsed = Parser.parse(&source);
    let analysis = AnalysisDocument::new(&source, (), Vec::new());

    assert_eq!(
        definition_at(&source, &parsed, Some(&analysis), 1, &Navigator),
        [location(&source, Span::new(0, 1))]
    );
    assert_eq!(
        references_at(&source, &parsed, Some(&analysis), 0, true, &Navigator),
        [location(&source, Span::new(0, 1))]
    );
    assert!(references_at(&source, &parsed, Some(&analysis), 0, false, &Navigator).is_empty());
}

#[test]
fn navigation_rejects_stale_database_managed_snapshots() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main.test", "x").unwrap();
    let old_source = sources.get(id).unwrap().clone();
    let old_parsed = Parser.parse(&old_source);
    let old_analysis = AnalysisDocument::new(&old_source, (), Vec::new());
    sources.update(id, "new").unwrap();
    let current_source = sources.get(id).unwrap();
    let current_parsed = Parser.parse(current_source);

    assert!(
        definition_at(
            current_source,
            &current_parsed,
            Some(&old_analysis),
            current_source.text().len(),
            &Navigator,
        )
        .is_empty()
    );
    assert!(
        references_at(
            current_source,
            &old_parsed,
            None,
            current_source.text().len(),
            true,
            &Navigator,
        )
        .is_empty()
    );
}

#[test]
fn navigation_rejects_invalid_current_file_locations_but_keeps_other_files() {
    struct InvalidNavigator;

    impl LanguageNavigator<Parser> for InvalidNavigator {
        type Analysis = ();

        fn definition(
            &self,
            source: &SourceFile,
            _parsed: &ParsedDocument<String>,
            _analysis: Option<&AnalysisDocument<()>>,
            _offset: usize,
        ) -> Vec<SourceLocation> {
            let other_source = SourceFile::new("other.test", "");

            vec![
                location(source, Span::new(1, 1)),
                location(source, Span::new(0, 9)),
                SourceLocation::new(&other_source, Span::new(100, 120)),
            ]
        }

        fn references(
            &self,
            _source: &SourceFile,
            _parsed: &ParsedDocument<String>,
            _analysis: Option<&AnalysisDocument<()>>,
            _offset: usize,
            _include_declaration: bool,
        ) -> Vec<SourceLocation> {
            Vec::new()
        }
    }

    let source = SourceFile::new("main.test", "é");
    let parsed = Parser.parse(&source);

    let locations = definition_at(&source, &parsed, None, 2, &InvalidNavigator);

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].source_name(), "other.test");
}
