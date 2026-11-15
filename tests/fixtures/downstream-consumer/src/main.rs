use langmeld_core::{SourceFile, Span};
use langmeld_expression::{Associativity, ExpressionGrammar, ExpressionParser, OperatorSpec};
use langmeld_language::{
    AnalysisDocument, AnalysisDocumentParts, CapturedValue, ParsedDocument, ParsedDocumentParts,
    parse, parse_language_definition_project,
};
use langmeld_project::{DirectoryDefinitionResolver, load_language_spec_from_sources};
use langmeld_textmate::{TextMateOptions, generate as generate_textmate};
use langmeld_tree_sitter::{TreeSitterOptions, generate as generate_tree_sitter};
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/langmeld_sources.rs"));

fn main() {
    verify_in_memory_language_project();
    verify_named_result_parts();

    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolver = DirectoryDefinitionResolver::new(project_root)
        .expect("downstream fixture should be a valid project root");
    let root = resolver
        .load(LANGMELD_ROOT_KEY)
        .expect("root definition should load");
    let definition = parse_language_definition_project(root, &resolver);

    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    assert_eq!(resolver.sources().len(), 3);
    let language = definition.language.expect("validated language definition");

    let grammar = generate_textmate(
        &language,
        &TextMateOptions::new("source.mini").file_types(["mini"]),
    )
    .expect("TextMate grammar should be generated");
    assert!(grammar.contains("\"scopeName\": \"source.mini\""));
    assert!(grammar.contains("\"fileTypes\": [\n    \"mini\"\n  ]"));

    let tree_sitter = generate_tree_sitter(&language, &TreeSitterOptions::new("mini"))
        .expect("Tree-sitter grammar should be generated");
    assert!(tree_sitter.contains("name: \"mini\""));
    assert!(tree_sitter.contains("source_file: $ => repeat("));

    let program = SourceFile::new("main.mini", "program { let answer = 42; draw answer; }");
    let parsed = parse(&program, &language);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].children[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
    assert_eq!(parsed.constructs()[0].children[1].name, "draw");

    let operators = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .expect("valid operator declarations");

    let expression_source = SourceFile::new("value.expr", "20 + 4 * 2");
    let expression = ExpressionParser::new(&operators).parse(
        &expression_source,
        Span::new(0, expression_source.text().len()),
    );

    assert!(
        expression.diagnostics().is_empty(),
        "{:?}",
        expression.diagnostics()
    );
    assert!(expression.expression().is_some());

    println!("standalone Langmeld consumer, project imports, and editor exports passed");
}

fn verify_named_result_parts() {
    let source = SourceFile::new("api.mini", "value");
    let parsed = ParsedDocument::new(&source, "syntax tree", Vec::new()).into_parts();
    let ParsedDocumentParts {
        source_identity,
        tree,
        diagnostics,
    } = parsed;

    assert_eq!(source_identity, source.identity());
    assert_eq!(tree, "syntax tree");
    assert!(diagnostics.is_empty());

    let analysis = AnalysisDocument::new(&source, 17, Vec::new()).into_parts();
    let AnalysisDocumentParts {
        source_identity,
        result,
        diagnostics,
    } = analysis;

    assert_eq!(source_identity, source.identity());
    assert_eq!(result, 17);
    assert!(diagnostics.is_empty());
}

fn verify_in_memory_language_project() {
    assert_eq!(LANGMELD_SOURCES.len(), 3);
    let language = load_language_spec_from_sources(LANGMELD_ROOT_KEY, LANGMELD_SOURCES)
        .expect("embedded language definition should load");
    let source = SourceFile::new("main.mini", "program { let answer = 42; draw answer; }");
    let parsed = parse(&source, &language);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].children[1].name, "draw");
}
