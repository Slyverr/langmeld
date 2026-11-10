use std::path::Path;

use langmeld_core::SourceFile;
use langmeld_language::{CapturedValue, parse, parse_language_definition_project};
use langmeld_project::DirectoryDefinitionResolver;

fn load_language() -> langmeld_language::LanguageSpec {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolver = DirectoryDefinitionResolver::new(project_root).unwrap();
    let root = resolver.load("language/toy.lm").unwrap();
    let definition = parse_language_definition_project(root, &resolver);
    assert!(
        definition.diagnostics.is_empty(),
        "{:#?}",
        definition.diagnostics
    );
    definition
        .language
        .expect("definition should produce a language")
}

#[test]
fn imports_constructs_and_parses_the_example_program() {
    let language = load_language();
    let source = SourceFile::new(
        "main.widget",
        "program { let title = \"Hello\"; draw title; }",
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
        ["let", "draw"]
    );
    assert_eq!(
        root.children[0].fields[0].value,
        CapturedValue::Identifier("title".into())
    );
    assert_eq!(
        root.children[0].fields[1].value,
        CapturedValue::String("\"Hello\"".into())
    );
}

#[test]
fn reports_invalid_construct_syntax() {
    let language = load_language();
    let source = SourceFile::new("invalid.widget", "program { draw ; }");
    let parsed = parse(&source, &language);

    assert!(!parsed.diagnostics.is_empty());
}
