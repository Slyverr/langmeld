use langmeld_core::{Severity, SourceDatabase, SourceFile, SourceIdentity};
use langmeld_language::{CapturedValue, ConstructSpec, LanguageSpec, PatternPart, parse};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new(
                "let",
                [
                    PatternPart::literal("let"),
                    PatternPart::identifier("name"),
                    PatternPart::literal("="),
                    PatternPart::integer("value"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap()
}

#[test]
fn parses_multiple_declarations_and_captures_values_with_spans() {
    let source = SourceFile::new("main", "let answer = 42; print answer;");
    let result = parse(&source, &language());

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.constructs.len(), 2);
    assert_eq!(result.constructs[0].name, "let");
    assert_eq!(
        result.constructs[0].fields[0].value,
        CapturedValue::Identifier("answer".into())
    );
    assert_eq!(
        result.constructs[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
    assert_eq!(
        &source.text[result.constructs[0].span.start..result.constructs[0].span.end],
        "let answer = 42;"
    );
}

#[test]
fn reports_unmatched_tokens_and_keeps_parsing_after_them() {
    let source = SourceFile::new("main", "? print value;");
    let result = parse(&source, &language());

    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, Severity::Error);
    assert_eq!(result.diagnostics[0].location.unwrap().span.start, 0);
}

#[test]
fn reports_unterminated_string_literals() {
    let grammar = LanguageSpec::new(
        "Strings",
        [ConstructSpec::new(
            "say",
            [
                PatternPart::literal("say"),
                PatternPart::string("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("main", "say \"unfinished");
    let result = parse(&source, &grammar);

    assert_eq!(result.diagnostics.len(), 2);
    assert_eq!(result.diagnostics[0].code, "LM-LEX-001");
    assert_eq!(result.diagnostics[1].code, "LM-PARSE-002");
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
fn rejects_literal_patterns_that_do_not_match_one_lexer_token() {
    let construct = ConstructSpec::new("invalid", [PatternPart::literal("let name")]);

    assert!(matches!(
        LanguageSpec::new("Example", [construct]),
        Err(langmeld_language::SpecError::InvalidLiteral { .. })
    ));
}

#[test]
fn rejects_ambiguous_patterns_instead_of_using_declaration_order() {
    let short = ConstructSpec::new("short", [PatternPart::literal("print")]);
    let long = ConstructSpec::new(
        "long",
        [
            PatternPart::literal("print"),
            PatternPart::identifier("name"),
        ],
    );

    assert!(matches!(
        LanguageSpec::new("Example", [short, long]),
        Err(langmeld_language::SpecError::AmbiguousConstructs(_, _))
    ));
}

#[test]
fn parse_result_and_diagnostics_keep_source_revision() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "? print value;").unwrap();
    let source = sources.get(id).unwrap();
    let result = parse(source, &language());
    let expected = SourceIdentity { id, revision: 1 };

    assert_eq!(result.source, Some(expected));
    assert_eq!(
        result.diagnostics[0].location.unwrap().identity,
        Some(expected)
    );
}

#[test]
fn parses_nested_blocks_and_restricts_children_to_the_declared_scope() {
    let grammar = LanguageSpec::new(
        "Nested",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["when", "print"],
            ),
            ConstructSpec::new(
                "when",
                [
                    PatternPart::literal("when"),
                    PatternPart::identifier("condition"),
                ],
            )
            .block("{", "}", &["print"]),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap();
    let source = SourceFile::new("main", "program { when ready { print item; } }");

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.is_empty());
    let root = &result.constructs[0];
    assert_eq!(root.name, "program");
    assert_eq!(root.children[0].name, "when");
    assert_eq!(root.children[0].children[0].name, "print");
    assert_eq!(root.children[0].children[0].fields[0].name, "name");
}

#[test]
fn reports_disallowed_child_constructs_but_recovers_within_the_block() {
    let grammar = LanguageSpec::new(
        "Scoped",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "let",
                [PatternPart::literal("let"), PatternPart::identifier("name")],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap();
    let source = SourceFile::new("main", "program { let item print value; }");

    let result = parse(&source, &grammar);

    assert!(!result.diagnostics.is_empty());
    assert_eq!(result.constructs[0].children.len(), 1);
    assert_eq!(result.constructs[0].children[0].name, "print");
}

#[test]
fn reports_missing_block_closer_at_end_of_source() {
    let grammar = LanguageSpec::new(
        "Block",
        [ConstructSpec::new("program", [PatternPart::literal("program")]).block("{", "}", &[])],
    )
    .unwrap();
    let source = SourceFile::new("main", "program {");

    let result = parse(&source, &grammar);

    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-004");
    assert_eq!(
        result.diagnostics[0].location.unwrap().span.start,
        source.text.len()
    );
}

#[test]
fn caps_recursive_block_depth_and_recovers_without_overflowing_the_stack() {
    let grammar = LanguageSpec::new(
        "Recursive",
        [ConstructSpec::new("nest", [PatternPart::literal("nest")]).block("{", "}", &["nest"])],
    )
    .unwrap();
    let depth = langmeld_language::MAX_NESTING_DEPTH + 4;
    let mut text = "nest { ".repeat(depth);
    text.push_str(&"} ".repeat(depth));
    let source = SourceFile::new("nested", text);

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.iter().any(|d| d.code == "LM-PARSE-003"));
    assert_eq!(result.constructs.len(), 1);
}

#[test]
fn block_child_names_must_resolve_to_language_constructs() {
    let grammar = ConstructSpec::new("program", [PatternPart::literal("program")]).block(
        "{",
        "}",
        &["missing"],
    );

    assert!(matches!(
        LanguageSpec::new("Example", [grammar]),
        Err(langmeld_language::SpecError::UnknownChildConstruct { .. })
    ));
}
