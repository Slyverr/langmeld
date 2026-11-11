use super::*;

#[test]
fn root_declarations_restrict_top_level_parsing_and_completion() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Rooted" {
            root "program";
            root "module";
            construct "program" {
                syntax { literal "program"; }
                block "{" "}" { allow "statement"; }
            }
            construct "module" {
                syntax { literal "module"; }
            }
            construct "statement" {
                syntax { literal "statement"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    assert_eq!(
        language
            .root_constructs()
            .map(|construct| construct.name())
            .collect::<Vec<_>>(),
        ["program", "module"]
    );

    let completions = complete_at(&SourceFile::new("main", ""), &language, 0);
    assert_eq!(
        completions
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["program", "module"]
    );

    let invalid_root = parse(&SourceFile::new("main", "statement"), &language);
    assert!(!invalid_root.diagnostics().is_empty());
}

#[test]
fn root_declarations_report_unknown_names_at_the_declaration() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Rooted" {
            root "missing";
            construct "program" {
                syntax { literal "program"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-004")
        .expect("unknown roots should be reported");
    let location = diagnostic
        .location
        .as_ref()
        .expect("root diagnostic is located");
    let root_start = source.text().find("root \"missing\"").unwrap();
    assert_eq!(location.span().start, root_start);
    assert!(definition.language.is_none());
}

#[test]
fn duplicate_root_declarations_report_the_later_declaration() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Rooted" {
            root "program";
            root "program";
            construct "program" { syntax { literal "program"; } }
        }"#,
    );

    let definition = parse_language_definition(&source);
    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-004")
        .expect("duplicate roots should be reported");
    let location = diagnostic
        .location
        .as_ref()
        .expect("root diagnostic is located");
    let duplicate_start = source.text().rfind("root \"program\"").unwrap();
    assert_eq!(location.span().start, duplicate_start);
}

#[test]
fn unknown_block_category_has_a_definition_diagnostic() {
    let source = SourceFile::new(
        "language.langmeld",
        r##"language "Toy" {
            construct "program" {
                syntax { literal "program"; }
                block "{" "}" { allow_category "statement"; }
            }
            construct "let" {
                syntax { literal "let"; }
                implements "declaration";
            }
        }"##,
    );

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "LM-DEF-004"
            && diagnostic
                .message
                .contains("category `statement` with no members")
    }));
}
