use super::*;

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

    assert!(result.diagnostics().is_empty());
    let root = &result.constructs()[0];
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

    assert!(!result.diagnostics().is_empty());
    assert_eq!(result.constructs()[0].children.len(), 1);
    assert_eq!(result.constructs()[0].children[0].name, "print");
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

    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-004");
    assert_eq!(
        result.diagnostics()[0]
            .location
            .as_ref()
            .unwrap()
            .span()
            .start,
        source.text().len()
    );
}

#[test]
fn optional_blocks_can_be_omitted_or_parsed_when_present() {
    let grammar = LanguageSpec::new(
        "Optional block",
        [
            ConstructSpec::new(
                "move",
                [
                    PatternPart::literal("move"),
                    PatternPart::identifier("target"),
                ],
            )
            .optional_block("{", "}", &["print"]),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("value"),
                ],
            ),
        ],
    )
    .unwrap();

    let without_block = parse(&SourceFile::new("main", "move item print done"), &grammar);
    assert!(without_block.diagnostics().is_empty());
    assert_eq!(without_block.constructs().len(), 2);
    assert_eq!(without_block.constructs()[0].name, "move");
    assert_eq!(without_block.constructs()[0].block_span, None);
    assert!(without_block.constructs()[0].children.is_empty());

    let with_block = parse(
        &SourceFile::new("main", "move item { print done }"),
        &grammar,
    );
    assert!(with_block.diagnostics().is_empty());
    let movement = &with_block.constructs()[0];
    assert_eq!(movement.name, "move");
    assert!(movement.block_span.is_some());
    assert_eq!(movement.children.len(), 1);
    assert_eq!(movement.children[0].name, "print");

    let empty_block = parse(&SourceFile::new("main", "move item {}"), &grammar);
    assert!(empty_block.diagnostics().is_empty());
    assert!(empty_block.constructs()[0].block_span.is_some());
    assert!(empty_block.constructs()[0].children.is_empty());
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

#[test]
fn category_members_expand_into_block_children_in_construct_order() {
    let grammar =
        LanguageSpec::new(
            "Grouped",
            [
                ConstructSpec::new("program", [PatternPart::literal("program")])
                    .block_with_categories("{", "}", &["standalone"], &["statement"]),
                ConstructSpec::new("let", [PatternPart::literal("let")]).implements(&["statement"]),
                ConstructSpec::new("standalone", [PatternPart::literal("standalone")]),
                ConstructSpec::new("print", [PatternPart::literal("print")])
                    .implements(&["statement"]),
            ],
        )
        .unwrap();

    let block = grammar.constructs()[0].block_spec().unwrap();
    assert_eq!(block.categories(), ["statement"]);
    assert_eq!(block.constructs(), ["standalone", "let", "print"]);
    assert_eq!(grammar.constructs()[1].categories(), ["statement"]);

    let parsed = parse(
        &SourceFile::new("main", "program { standalone let print }"),
        &grammar,
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0]
            .children
            .iter()
            .map(|child| child.name.as_str())
            .collect::<Vec<_>>(),
        ["standalone", "let", "print"]
    );
}

#[test]
fn rejects_block_categories_without_members() {
    let grammar = ConstructSpec::new("program", [PatternPart::literal("program")])
        .block_with_categories("{", "}", &[], &["statement"]);

    assert!(matches!(
        LanguageSpec::new("Grouped", [grammar]),
        Err(langmeld_language::SpecError::UnknownBlockCategory {
            category,
            ..
        }) if category == "statement"
    ));
}
