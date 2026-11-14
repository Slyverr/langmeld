use langmeld_core::SourceFile;
use langmeld_language::{ConstructSpec, LanguageSpec, PatternPart, complete_at, parse};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "Fallback example",
        [
            ConstructSpec::new(
                "edge",
                [
                    PatternPart::identifier("from"),
                    PatternPart::literal("->"),
                    PatternPart::identifier("to"),
                ],
            ),
            ConstructSpec::new(
                "unknown_property",
                [
                    PatternPart::identifier("name"),
                    PatternPart::literal(":"),
                    PatternPart::token("value"),
                ],
            )
            .fallback(),
            ConstructSpec::new("unknown_construct", [PatternPart::token("text")]).fallback(),
        ],
    )
    .expect("regular and fallback patterns may overlap")
}

#[test]
fn regular_construct_wins_over_an_overlapping_fallback() {
    let parsed = parse(
        &langmeld_core::SourceFile::new("main", "left -> right"),
        &language(),
    );

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].name, "edge");
}

#[test]
fn fallback_parses_syntax_without_a_regular_match() {
    let parsed = parse(
        &langmeld_core::SourceFile::new("main", "camera: 2"),
        &language(),
    );

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].name, "unknown_property");
}

#[test]
fn fallback_constructs_are_not_offered_as_completions() {
    let language = LanguageSpec::new(
        "Fallback completion",
        [
            ConstructSpec::new("known", [PatternPart::literal("known")]),
            ConstructSpec::new("unknown", [PatternPart::literal("unknown")]).fallback(),
        ],
    )
    .unwrap();
    let source = SourceFile::new("main", "");

    let completions = complete_at(&source, &language, 0);

    assert_eq!(
        completions
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["known"]
    );
}

#[test]
fn equal_length_fallback_matches_remain_ambiguous() {
    let first = ConstructSpec::new("first", [PatternPart::identifier("name")]).fallback();
    let second = ConstructSpec::new("second", [PatternPart::identifier("name")]).fallback();
    let language = LanguageSpec::new("Ambiguous fallback", [first, second]).unwrap();

    let parsed = parse(&SourceFile::new("main", "item"), &language);

    assert_eq!(parsed.diagnostics()[0].code, "LM-PARSE-006");
}

#[test]
fn longer_partial_fallback_reports_its_missing_field() {
    let language = LanguageSpec::new(
        "Partial fallback",
        [
            ConstructSpec::new(
                "unknown_property",
                [
                    PatternPart::identifier("name"),
                    PatternPart::literal(":"),
                    PatternPart::token("value"),
                ],
            )
            .fallback(),
            ConstructSpec::new("unknown_construct", [PatternPart::token("text")]).fallback(),
        ],
    )
    .unwrap();

    let parsed = parse(&SourceFile::new("main", "camera:"), &language);

    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code, "LM-PARSE-002");
    assert!(parsed.constructs().is_empty());
}

#[test]
fn fallback_does_not_hide_a_malformed_specific_construct() {
    let language = LanguageSpec::new(
        "Specific before fallback",
        [
            ConstructSpec::new(
                "icon",
                [
                    PatternPart::literal("icon"),
                    PatternPart::literal(":"),
                    PatternPart::string("value"),
                ],
            ),
            ConstructSpec::new(
                "unknown_property",
                [
                    PatternPart::identifier("name"),
                    PatternPart::literal(":"),
                    PatternPart::token("value"),
                ],
            )
            .fallback(),
        ],
    )
    .unwrap();

    let parsed = parse(&SourceFile::new("main", "icon: 3"), &language);

    assert_eq!(parsed.diagnostics()[0].code, "LM-PARSE-002");
    assert!(parsed.constructs().is_empty());
}

#[test]
fn fallback_field_does_not_consume_the_enclosing_block_closer() {
    let language = LanguageSpec::new(
        "Fallback block",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["icon", "unknown_property"],
            ),
            ConstructSpec::new(
                "icon",
                [
                    PatternPart::literal("icon"),
                    PatternPart::literal(":"),
                    PatternPart::string("value"),
                ],
            ),
            ConstructSpec::new(
                "unknown_property",
                [
                    PatternPart::identifier("name"),
                    PatternPart::literal(":"),
                    PatternPart::token("value"),
                ],
            )
            .fallback(),
        ],
    )
    .unwrap()
    .with_root_constructs(&["program"])
    .unwrap();

    let parsed = parse(&SourceFile::new("main", "program { icon: }"), &language);

    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code, "LM-PARSE-002");
    assert!(parsed.constructs()[0].children.is_empty());
}
