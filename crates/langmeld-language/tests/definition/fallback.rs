use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition;

#[test]
fn fallback_is_available_in_declarative_constructs() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"
language "Toy" {
    construct "property" {
        syntax {
            identifier "name";
            literal ":";
            token "value";
        }
        fallback;
    }
}
"#,
    );

    let definition = parse_language_definition(&source);

    assert!(
        definition.diagnostics.is_empty(),
        "{:?}",
        definition.diagnostics
    );
    assert!(definition.language.unwrap().constructs()[0].is_fallback());
}
