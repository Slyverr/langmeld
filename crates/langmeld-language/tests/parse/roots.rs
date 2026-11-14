use super::*;

#[test]
fn root_construct_allowlist_preserves_nested_constructs() {
    let grammar = LanguageSpec::new(
        "ScopedRoot",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["program"])
    .unwrap();
    let source = SourceFile::new("main", "print outside program { print inside }");

    let result = parse(&source, &grammar);

    assert_eq!(result.constructs().len(), 1);
    assert_eq!(result.constructs()[0].name, "program");
    assert_eq!(result.constructs()[0].children[0].name, "print");
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-001");
}

#[test]
fn parses_multiple_explicit_roots_with_and_without_blocks() {
    let grammar = LanguageSpec::new(
        "MixedRoots",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { node title } scenario startup");

    let result = parse(&source, &grammar);

    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
    assert_eq!(
        result
            .constructs()
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert_eq!(result.constructs()[0].children[0].name, "node");
    assert_eq!(result.constructs()[1].children.len(), 0);
    assert_eq!(
        &source.text()[result.constructs()[1].span.start..result.constructs()[1].span.end],
        "scenario startup"
    );
}

#[test]
fn recovers_at_a_later_root_through_unclosed_nested_blocks() {
    let grammar = LanguageSpec::new(
        "RootRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["group"]),
            ConstructSpec::new(
                "group",
                [
                    PatternPart::literal("group"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new(
        "main",
        "visual board { group panel { node title scenario setup",
    );

    let result = parse(&source, &grammar);

    assert_eq!(
        result
            .constructs()
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert_eq!(result.constructs()[0].children[0].name, "group");
    assert_eq!(result.constructs()[0].children[0].children[0].name, "node");
    let missing_closers = result
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code == "LM-PARSE-004")
        .collect::<Vec<_>>();
    let scenario_start = source.text().find("scenario").unwrap();
    assert!(result.constructs()[0].span.end <= scenario_start);
    assert!(result.constructs()[1].span.start >= scenario_start);
    assert_eq!(missing_closers.len(), 2, "{:?}", result.diagnostics());
    assert!(missing_closers.iter().all(|diagnostic| {
        diagnostic.location.as_ref().unwrap().span() == Span::new(scenario_start, scenario_start)
    }));
    assert_eq!(result.diagnostics().len(), 2, "{:?}", result.diagnostics());
}

#[test]
fn root_recovery_keeps_a_root_construct_allowed_as_a_block_child() {
    let grammar = LanguageSpec::new(
        "RootChildRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["scenario"]),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { scenario nested");

    let result = parse(&source, &grammar);

    assert_eq!(result.constructs().len(), 1);
    assert_eq!(result.constructs()[0].children.len(), 1);
    assert_eq!(result.constructs()[0].children[0].name, "scenario");
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-004");
}

#[test]
fn fallback_child_does_not_hide_a_later_root_during_recovery() {
    let grammar = LanguageSpec::new(
        "RootRecoveryWithFallback",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["unknown"],
            ),
            ConstructSpec::new("config", [PatternPart::literal("config")]),
            ConstructSpec::new("unknown", [PatternPart::token("text")])
                .implements(&["unknown_child"])
                .fallback(),
        ],
    )
    .unwrap()
    .with_root_constructs(&["program", "config"])
    .unwrap();
    let source = SourceFile::new("main", "program { config");

    let result = parse(&source, &grammar);

    assert_eq!(
        result
            .constructs()
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["program", "config"]
    );
    assert_eq!(result.diagnostics().len(), 1, "{:?}", result.diagnostics());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-004");
}

#[test]
fn resumes_at_a_root_after_a_malformed_block_child() {
    let grammar = LanguageSpec::new(
        "RootRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { node broken scenario setup");

    let result = parse(&source, &grammar);

    assert_eq!(
        result
            .constructs()
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "LM-PARSE-002" && diagnostic.message.contains("construct `node`")
    }));
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "LM-PARSE-004" && diagnostic.message.contains("closing token `}`")
    }));
}

#[test]
fn root_construct_allowlist_rejects_unknown_and_duplicate_names() {
    let grammar = LanguageSpec::new(
        "ScopedRoot",
        [ConstructSpec::new(
            "program",
            [PatternPart::literal("program")],
        )],
    )
    .unwrap();

    assert!(matches!(
        grammar.clone().with_root_constructs(&["missing"]),
        Err(langmeld_language::SpecError::UnknownRootConstruct(name)) if name == "missing"
    ));
    assert!(matches!(
        grammar.with_root_constructs(&["program", "program"]),
        Err(langmeld_language::SpecError::DuplicateRootConstruct(name)) if name == "program"
    ));
}
