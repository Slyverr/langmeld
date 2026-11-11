use langmeld_core::{SourceFile, Span};
use langmeld_expression::{Associativity, ExpressionGrammar, ExpressionParser, OperatorSpec};
use langmeld_language::{CapturedValue, parse, parse_language_definition_project};
use langmeld_project::DirectoryDefinitionResolver;
use langmeld_textmate::{TextMateOptions, generate as generate_textmate};
use langmeld_tree_sitter::{TreeSitterOptions, generate as generate_tree_sitter};
use std::path::Path;

fn main() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolver = DirectoryDefinitionResolver::new(project_root)
        .expect("downstream fixture should be a valid project root");
    let root = resolver
        .load("language/main.langmeld")
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

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].children[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
    assert_eq!(parsed.constructs[0].children[1].name, "draw");

    let operators = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .expect("valid operator declarations");

    let expression_source = SourceFile::new("value.expr", "20 + 4 * 2");
    let expression = ExpressionParser::new(&operators).parse(
        &expression_source,
        Span::new(0, expression_source.text.len()),
    );

    assert!(
        expression.diagnostics.is_empty(),
        "{:?}",
        expression.diagnostics
    );
    assert!(expression.expression.is_some());

    println!("standalone Langmeld consumer, project imports, and editor exports passed");
}
