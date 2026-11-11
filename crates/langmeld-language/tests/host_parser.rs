use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

#[derive(Debug, PartialEq, Eq)]
struct Assignment {
    name: String,
    value: i64,
}

struct AssignmentParser;

impl LanguageParser for AssignmentParser {
    type Tree = Vec<Assignment>;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let mut assignments = Vec::new();
        let mut diagnostics = Vec::new();
        for (line_number, line) in source.text().lines().enumerate() {
            let line_start = source
                .text()
                .split_inclusive('\n')
                .take(line_number)
                .map(str::len)
                .sum::<usize>();
            let Some((name, value)) = line.trim().split_once('=') else {
                diagnostics.push(Diagnostic::error(
                    "HOST-PARSE-001",
                    "expected an assignment",
                    source,
                    Span::new(line_start, line_start + line.len()),
                ));
                continue;
            };
            let Some(value) = value.trim().parse::<i64>().ok() else {
                diagnostics.push(Diagnostic::error(
                    "HOST-PARSE-002",
                    "expected an integer value",
                    source,
                    Span::new(line_start, line_start + line.len()),
                ));
                continue;
            };
            assignments.push(Assignment {
                name: name.trim().to_owned(),
                value,
            });
        }
        ParsedDocument::new(source, assignments, diagnostics)
    }
}

#[test]
fn host_parser_keeps_its_own_ast_and_source_identity() {
    let source = SourceFile::new("assignments", "width = 20\nheight = 10\n");

    let parsed = AssignmentParser.parse(&source);

    assert!(parsed.is_valid());
    assert_eq!(parsed.tree().len(), 2);
    assert_eq!(parsed.tree()[0].name, "width");
    assert_eq!(parsed.tree()[1].value, 10);
    assert_eq!(parsed.source_identity(), source.identity());
}

#[test]
fn host_parser_can_return_recovered_tree_and_diagnostics_together() {
    let source = SourceFile::new("assignments", "width = 20\nheight = nope\n");

    let parsed = AssignmentParser.parse(&source);

    assert!(!parsed.is_valid());
    assert_eq!(parsed.tree().len(), 1);
    assert_eq!(parsed.diagnostics()[0].code, "HOST-PARSE-002");
    assert!(parsed.diagnostics()[0].location.is_some());
}

#[test]
fn parsed_document_can_be_consumed_without_separating_its_source_identity() {
    let source = SourceFile::new("assignments", "width = 20");
    let source_identity = source.identity();
    let diagnostic = Diagnostic::error(
        "HOST-PARSE-003",
        "example finding",
        &source,
        Span::new(0, source.text().len()),
    );
    let parsed = ParsedDocument::new(&source, vec!["width"], vec![diagnostic]);

    let (parsed_source, tree, diagnostics) = parsed.into_parts();

    assert_eq!(parsed_source, source_identity);
    assert_eq!(tree, ["width"]);
    assert_eq!(diagnostics[0].code, "HOST-PARSE-003");
}
