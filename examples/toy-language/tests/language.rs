use langmeld_core::SourceFile;
use langmeld_expression::{
    Associativity, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::{CapturedValue, parse};
use langmeld_toy_language_example::{load, run};

fn load_language() -> langmeld_language::LanguageSpec {
    load().expect("definition should produce a language")
}

#[test]
fn imports_constructs_and_parses_the_example_program() {
    let language = load_language();
    let source = SourceFile::new(
        "main.widget",
        "// a comment before the program\nprogram { let title = \"Hello\"; draw circle; compute 20 + 4 * (2 + 1); }",
    );
    let parsed = parse(&source, &language);

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.constructs.len(), 1);
    let root = &parsed.constructs[0];
    assert_eq!(root.name, "program");
    assert_eq!(
        root.children
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["let", "draw", "compute"]
    );
    assert_eq!(
        root.children[0].fields[0].value,
        CapturedValue::Identifier("title".into())
    );
    assert_eq!(
        root.children[0].fields[1].value,
        CapturedValue::String("\"Hello\"".into())
    );
    assert_eq!(root.children[1].fields[0].name, "shape");
    assert_eq!(
        root.children[1].fields[0].value,
        CapturedValue::Token {
            text: "circle".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );
    let expression = &root.children[2].fields[0];
    assert_eq!(expression.name, "value");
    assert_eq!(expression.value, CapturedValue::Expression);
    assert_eq!(
        &source.text[expression.span.start..expression.span.end],
        "20 + 4 * (2 + 1)"
    );

    let grammar = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .unwrap();
    let parsed_expression = ExpressionParser::new(&grammar).parse(&source, expression.span);
    assert!(
        parsed_expression.diagnostics.is_empty(),
        "{:?}",
        parsed_expression.diagnostics
    );
    assert!(matches!(
        parsed_expression.expression.map(|expression| expression.kind),
        Some(ExpressionKind::Infix { operator, .. }) if operator == "+"
    ));
}

#[test]
fn reports_invalid_construct_syntax() {
    let language = load_language();
    let source = SourceFile::new("invalid.widget", "program { draw triangle; }");
    let parsed = parse(&source, &language);

    assert!(!parsed.diagnostics.is_empty());
}

#[test]
fn executes_construct_handlers_and_host_defined_expressions() {
    let source = SourceFile::new(
        "main.widget",
        "program { let title = \"Hello\"; print title; draw circle; compute 20 + 4 * (2 + 1); }",
    );

    assert_eq!(
        run(&source).expect("program should execute"),
        "Hello\nDrawing circle.\n32\n"
    );
}

#[test]
fn reports_language_errors_at_the_source_location() {
    let source = SourceFile::new("missing.widget", "program { print absent; }");
    let error = run(&source).expect_err("an unresolved variable should fail");

    assert!(error.contains("unknown variable `absent`"));
    assert!(error.contains("missing.widget:1:"));
}
