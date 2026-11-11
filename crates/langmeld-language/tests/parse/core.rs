use super::*;

#[test]
fn parses_multiple_declarations_and_captures_values_with_spans() {
    let source = SourceFile::new("main", "let answer = 42; print answer;");
    let result = parse(&source, &language());

    assert!(result.is_valid());
    assert!(result.diagnostics().is_empty());
    assert_eq!(result.constructs().len(), 2);
    assert_eq!(result.constructs()[0].name, "let");
    assert_eq!(
        result.constructs()[0].fields[0].value,
        CapturedValue::Identifier("answer".into())
    );
    assert_eq!(
        result.constructs()[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
    assert_eq!(
        &source.text()[result.constructs()[0].span.start..result.constructs()[0].span.end],
        "let answer = 42;"
    );
}

#[test]
fn resumes_after_a_missing_terminator_before_a_valid_construct() {
    let statements = language();
    let constructs = std::iter::once(
        ConstructSpec::new("program", [PatternPart::literal("program")]).block(
            "{",
            "}",
            &["let", "print"],
        ),
    )
    .chain(statements.constructs().iter().cloned());
    let grammar = LanguageSpec::new("Statements", constructs)
        .unwrap()
        .with_root_constructs(&["program"])
        .unwrap();
    let source = SourceFile::new(
        "main",
        "program { let first = 1 print second; let third = 3; }",
    );

    let result = parse(&source, &grammar);

    assert!(!result.is_valid());
    assert_eq!(
        result.constructs()[0]
            .children
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["print", "let"]
    );
    let missing_terminator = result
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-PARSE-002")
        .expect("missing semicolon diagnostic");
    let next_construct = source.text().find("print").unwrap();
    assert_eq!(
        missing_terminator.location.as_ref().unwrap().span(),
        Span::new(next_construct, next_construct + "print".len())
    );
    assert_eq!(result.diagnostics().len(), 1, "{:?}", result.diagnostics());
}

#[test]
fn captures_expression_span_and_consumes_its_top_level_terminator() {
    let grammar = LanguageSpec::new(
        "Expressions",
        [ConstructSpec::new(
            "print",
            [
                PatternPart::literal("print"),
                PatternPart::expression("value", ";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("main", "print call(a + nested(b)) + { let x = 3; x };");

    let result = parse(&source, &grammar);

    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
    let construct = &result.constructs()[0];
    let field = &construct.fields[0];
    assert_eq!(field.value, CapturedValue::Expression);
    assert_eq!(
        &source.text()[field.span.start..field.span.end],
        "call(a + nested(b)) + { let x = 3; x }"
    );
    assert_eq!(
        &source.text()[construct.span.start..construct.span.end],
        "print call(a + nested(b)) + { let x = 3; x };"
    );
}

#[test]
fn expression_capture_rejects_empty_or_unterminated_expressions() {
    let grammar = LanguageSpec::new(
        "Expressions",
        [ConstructSpec::new(
            "print",
            [
                PatternPart::literal("print"),
                PatternPart::expression("value", ";"),
            ],
        )],
    )
    .unwrap();

    for input in ["print ;", "print call(1)"] {
        let result = parse(&SourceFile::new("main", input), &grammar);
        assert!(result.constructs().is_empty(), "accepted {input:?}");
        assert!(
            !result.diagnostics().is_empty(),
            "no diagnostic for {input:?}"
        );
    }
}

#[test]
fn rejects_expression_terminators_that_are_not_single_tokens() {
    let construct = ConstructSpec::new("print", [PatternPart::expression("value", "; ;")]);

    assert!(matches!(
        LanguageSpec::new("Expressions", [construct]),
        Err(langmeld_language::SpecError::InvalidExpressionTerminator { .. })
    ));
}

#[test]
fn validates_duplicate_declarations_and_fields() {
    let repeated = ConstructSpec::new(
        "let",
        [PatternPart::identifier("x"), PatternPart::identifier("x")],
    );
    assert!(LanguageSpec::new("Example", [repeated]).is_err());
    let first = ConstructSpec::new("same", [PatternPart::literal("first")]);
    let second = ConstructSpec::new("same", [PatternPart::literal("second")]);
    assert!(matches!(
        LanguageSpec::new("Example", [first, second]),
        Err(langmeld_language::SpecError::DuplicateConstruct(_))
    ));
}

#[test]
fn parse_result_and_diagnostics_keep_source_revision() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "? print value;").unwrap();
    let source = sources.get(id).unwrap();
    let result = parse(source, &language());
    let expected = source.identity();

    assert_eq!(result.source_identity(), expected);
    assert_eq!(
        result.diagnostics()[0]
            .location
            .as_ref()
            .unwrap()
            .source_identity(),
        expected
    );
}

#[test]
fn parse_result_can_be_consumed_without_separating_its_source_identity() {
    let mut sources = SourceDatabase::default();
    let id = sources
        .insert("main", "let answer = 42; print answer;")
        .unwrap();
    let source = sources.get(id).unwrap();
    let expected_identity = source.identity();
    let result = parse(source, &language());

    assert!(result.is_valid());

    let parts = result.into_parts();

    assert_eq!(parts.source_identity, expected_identity);
    assert_eq!(parts.constructs.len(), 2);
    assert_eq!(
        parts.syntax_tree.root.span,
        Span::new(0, source.text().len())
    );
    assert!(parts.diagnostics.is_empty());
    assert!(!parts.resource_limited);
}
