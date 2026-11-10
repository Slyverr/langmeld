use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, parse_language_definition_project};
use langmeld_project::InMemoryDefinitionResolver;

#[test]
fn resolves_nested_imports_and_composes_a_language_from_memory() {
    let mut resolver = InMemoryDefinitionResolver::new();
    let root_source = SourceFile::new(
        "language/main.langmeld",
        r#"language "Example" {
            import "constructs/statements.langmeld";
            root "program";
            construct "program" {
                syntax { literal "program"; }
                block "{" "}" { allow "draw"; }
            }
        }"#,
    );
    resolver
        .insert("language/main.langmeld", root_source)
        .unwrap();
    resolver
        .insert(
            "language/constructs/statements.langmeld",
            SourceFile::new(
                "language/constructs/statements.langmeld",
                r#"import "../constructs/shapes.langmeld";"#,
            ),
        )
        .unwrap();
    resolver
        .insert(
            "language/constructs/shapes.langmeld",
            SourceFile::new(
                "language/constructs/shapes.langmeld",
                r#"construct "draw" {
                    syntax {
                        literal "draw";
                        choice "shape" { value "circle"; value "square"; }
                        literal ";";
                    }
                }"#,
            ),
        )
        .unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(result.is_valid(), "{:?}", result.diagnostics);
    assert_eq!(resolver.sources().len(), 3);
}

#[test]
fn rejects_keys_and_imports_that_escape_the_virtual_project() {
    let mut resolver = InMemoryDefinitionResolver::new();
    assert!(
        resolver
            .insert("../outside.langmeld", SourceFile::new("outside", ""))
            .is_err()
    );
    resolver
        .insert(
            "language/main.langmeld",
            SourceFile::new("language/main.langmeld", ""),
        )
        .unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    assert!(
        resolver
            .resolve(&root.key, "../../outside.langmeld")
            .is_err()
    );
    assert!(resolver.resolve(&root.key, "/outside.langmeld").is_err());
    assert!(
        resolver
            .resolve(&root.key, "folder\\outside.langmeld")
            .is_err()
    );
    assert!(
        resolver
            .resolve(&root.key, "missing.langmeld")
            .unwrap_err()
            .contains("not in the source collection")
    );
}

#[test]
fn replaces_updated_snapshots_and_keeps_sources_sorted_by_key() {
    let mut resolver = InMemoryDefinitionResolver::new();
    resolver
        .insert("z.langmeld", SourceFile::new("z.langmeld", "old"))
        .unwrap();
    resolver
        .insert("a.langmeld", SourceFile::new("a.langmeld", "a"))
        .unwrap();
    let replaced = resolver
        .insert("z.langmeld", SourceFile::new("z.langmeld", "new"))
        .unwrap();

    assert_eq!(replaced.unwrap().text, "old");
    assert_eq!(
        resolver
            .sources()
            .iter()
            .map(|source| source.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "new"]
    );
}

#[test]
fn imported_definition_diagnostics_keep_the_embedded_source_name() {
    let mut resolver = InMemoryDefinitionResolver::new();
    resolver
        .insert(
            "language/main.langmeld",
            SourceFile::new(
                "language/main.langmeld",
                r#"language "Example" { import "constructs/broken.langmeld"; }"#,
            ),
        )
        .unwrap();
    resolver
        .insert(
            "language/constructs/broken.langmeld",
            SourceFile::new(
                "bundled/geometry/broken.langmeld",
                r#"construct "broken" {}"#,
            ),
        )
        .unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(!result.is_valid());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.source_name == "bundled/geometry/broken.langmeld")
    }));
}
