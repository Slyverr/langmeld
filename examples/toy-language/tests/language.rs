use langmeld_core::SourceFile;
use langmeld_expression::{
    Associativity, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::CapturedValue;
use langmeld_toy_language_example::{Language, load};

fn load_language() -> Language {
    load().expect("definition should produce a language")
}

#[test]
fn imports_constructs_and_parses_the_example_program() {
    let language = load_language();
    let source = SourceFile::new(
        "main.widget",
        "// a line comment\nprogram { /* a block comment */ let title = \"Hello\"; say title; draw circle labeled \"main target\"; emit \"first\", \"second\"; compute 20 + 4 * (2 + 1); }",
    );
    let parsed = language.parse(&source);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:#?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs().len(), 1);
    let root = &parsed.constructs()[0];
    assert_eq!(root.name, "program");
    assert_eq!(
        root.children
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["let", "print", "draw", "emit", "compute"]
    );
    assert_eq!(
        root.children[0].fields[0].value,
        CapturedValue::Identifier("title".into())
    );
    assert_eq!(
        root.children[0].fields[1].value,
        CapturedValue::String("\"Hello\"".into())
    );
    assert_eq!(root.children[1].variant, 1);
    assert_eq!(root.children[1].fields[0].name, "name");
    assert_eq!(root.children[2].fields[0].name, "shape");
    assert_eq!(
        root.children[2].fields[0].value,
        CapturedValue::Token {
            text: "circle".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );
    assert_eq!(root.children[2].fields[1].name, "label");
    assert_eq!(root.children[3].fields.len(), 2);
    assert_eq!(root.children[3].fields[0].name, "message");
    assert_eq!(root.children[3].fields[1].name, "message");

    let expression = &root.children[4].fields[0];
    assert_eq!(expression.name, "value");
    assert_eq!(expression.value, CapturedValue::Expression);
    assert_eq!(
        &source.text()[expression.span.start..expression.span.end],
        "20 + 4 * (2 + 1)"
    );

    let grammar = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .unwrap();
    let parsed_expression = ExpressionParser::new(&grammar).parse(&source, expression.span);
    assert!(parsed_expression.diagnostics().is_empty());
    assert!(matches!(
        parsed_expression.expression().map(|expression| &expression.kind),
        Some(ExpressionKind::Infix { operator, .. }) if operator == "+"
    ));
}

#[test]
fn reports_invalid_construct_syntax() {
    let language = load_language();
    let source = SourceFile::new("invalid.widget", "program { draw triangle; }");
    let parsed = language.parse(&source);

    assert!(!parsed.diagnostics().is_empty());
}

#[test]
fn rejects_a_source_without_a_program_root() {
    let language = load_language();
    let error = language
        .run(&SourceFile::new("empty.widget", ""))
        .expect_err("a source must contain a program root");

    assert!(error.contains("program did not produce a root construct"));
}

#[test]
fn executes_construct_handlers_and_host_defined_expressions() {
    let language = load_language();
    let source = SourceFile::new(
        "main.widget",
        "program { let title = \"Hello\"; say title; draw circle labeled \"main target\"; emit \"first\", \"second\"; compute 20 + 4 * (2 + 1); }",
    );

    assert_eq!(
        language.run(&source).expect("program should execute"),
        "Hello\nDrawing circle labeled \"main target\".\nfirst\nsecond\n32\n"
    );
}

#[test]
fn reuses_one_loaded_language_for_multiple_programs() {
    let language = load_language();
    let first = SourceFile::new(
        "first.widget",
        "program { let message = \"First\"; say message; }",
    );
    let second = SourceFile::new("second.widget", "program { print missing; }");

    assert_eq!(
        language.run(&first).expect("first program should run"),
        "First\n"
    );
    let error = language
        .run(&second)
        .expect_err("each program should start with fresh runtime state");

    assert!(error.contains("unknown variable `missing`"), "{error}");
}

#[test]
fn supports_optional_fields_and_requires_repeated_items() {
    let language = load_language();

    assert_eq!(
        language
            .run(&SourceFile::new(
                "optional.widget",
                "program { draw square; }",
            ))
            .expect("draw label is optional"),
        "Drawing square.\n"
    );

    let error = language
        .run(&SourceFile::new("empty.widget", "program { emit; }"))
        .expect_err("one_or_more must contain at least one string");
    assert!(error.contains("expected"));
}

#[test]
fn reports_language_errors_at_the_source_location() {
    let language = load_language();
    let source = SourceFile::new("missing.widget", "program { print absent; }");
    let error = language
        .run(&source)
        .expect_err("an unresolved variable should fail");

    assert!(error.contains("unknown variable `absent`"));
    assert!(error.contains("missing.widget:1:"));
}
