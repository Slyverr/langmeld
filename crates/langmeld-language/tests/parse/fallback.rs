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
fn overlapping_fallback_constructs_remain_an_error() {
    let first = ConstructSpec::new("first", [PatternPart::identifier("name")]).fallback();
    let second = ConstructSpec::new("second", [PatternPart::identifier("name")]).fallback();

    assert!(LanguageSpec::new("Ambiguous fallback", [first, second]).is_err());
}
