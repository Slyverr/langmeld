use super::*;

#[test]
fn root_declarations_can_name_imported_constructs() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "language/main.langmeld",
            r#"language "Rooted" {
                import "constructs/items.langmeld";
                root "program";
                root "item";
                construct "program" {
                    syntax { literal "program"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "constructs/items.langmeld".into(),
        DefinitionSource::new(
            "items",
            SourceFile::new(
                "language/constructs/items.langmeld",
                r#"construct "item" { syntax { literal "item"; } }"#,
            ),
        ),
    );

    let definition = parse_language_definition_project(root, &resolver);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    assert_eq!(
        definition
            .language
            .unwrap()
            .root_constructs()
            .map(|construct| construct.name())
            .collect::<Vec<_>>(),
        ["program", "item"]
    );
}

#[test]
fn composes_nested_imports_and_keeps_construct_source_locations() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "language/main.langmeld",
            r#"language "Widget" {
                import "statements.langmeld";
                import "statements.langmeld";
                construct "program" {
                    syntax { literal "program"; }
                    block "{" "}" { allow "print"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "statements.langmeld".into(),
        DefinitionSource::new(
            "statements",
            SourceFile::new(
                "language/statements.langmeld",
                r#"import "values.langmeld";
                construct "print" {
                    syntax { literal "print"; identifier "name"; literal ";"; }
                }"#,
            ),
        ),
    );
    resolver.0.insert(
        "values.langmeld".into(),
        DefinitionSource::new(
            "values",
            SourceFile::new(
                "language/values.langmeld",
                r#"construct "let" {
                    syntax { literal "let"; identifier "name"; literal "="; integer "value"; literal ";"; }
                }"#,
            ),
        ),
    );

    let definition = parse_language_definition_project(root, &resolver);

    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    let source = SourceFile::new("main.widget", "program { print item; }");
    let parsed = parse(&source, &language);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].children[0].name, "print");
}

#[test]
fn reports_import_cycles_and_missing_imports_at_the_import_site() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "main.langmeld",
            r#"language "Cycle" { import "module.langmeld"; }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "module.langmeld".into(),
        DefinitionSource::new(
            "module",
            SourceFile::new("module.langmeld", r#"import "root.langmeld";"#),
        ),
    );
    resolver.0.insert(
        "root.langmeld".into(),
        DefinitionSource::new(
            "root",
            SourceFile::new(
                "main.langmeld",
                r#"language "Cycle" { import "module.langmeld"; }"#,
            ),
        ),
    );

    let cycle = parse_language_definition_project(root, &resolver);

    assert!(cycle.language.is_none());
    assert!(cycle.diagnostics.iter().any(|d| d.code == "LM-DEF-020"));
    assert!(cycle.diagnostics.iter().any(|d| {
        d.code == "LM-DEF-020"
            && d.location.as_ref().is_some_and(|location| {
                location.span().end > location.span().start
                    && location.source_name() == "module.langmeld"
            })
    }));

    let missing = parse_language_definition_project(
        DefinitionSource::new(
            "missing-root",
            SourceFile::new(
                "main.langmeld",
                r#"language "Missing" { import "none.langmeld"; }"#,
            ),
        ),
        &Resolver::default(),
    );
    assert!(missing.language.is_none());
    assert!(missing.diagnostics.iter().any(|d| d.code == "LM-DEF-018"));
}

#[test]
fn duplicate_imported_construct_diagnostic_points_to_later_declaration() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "language/main.langmeld",
            r#"language "Duplicate" {
                import "constructs.langmeld";
                construct "print" {
                    syntax { literal "print"; identifier "name"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "constructs.langmeld".into(),
        DefinitionSource::new(
            "constructs",
            SourceFile::new(
                "language/constructs.langmeld",
                r#"construct "print" {
                    syntax { literal "print"; identifier "value"; }
                }"#,
            ),
        ),
    );

    let definition = parse_language_definition_project(root, &resolver);

    assert!(definition.language.is_none());
    let duplicate = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-004")
        .expect("duplicate construct diagnostic");
    assert_eq!(
        duplicate
            .location
            .as_ref()
            .map(|location| location.source_name()),
        Some("language/constructs.langmeld")
    );
}

#[test]
fn single_file_definition_api_reports_when_imports_need_a_resolver() {
    let source = SourceFile::new(
        "main.langmeld",
        r#"language "Imported" { import "constructs.langmeld"; }"#,
    );

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    assert!(result.diagnostics.iter().any(|d| d.code == "LM-DEF-010"));
}

#[test]
fn imported_construct_categories_can_populate_root_block_children() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "language/main.langmeld",
            r#"language "Toy" {
                import "constructs.langmeld";
                construct "program" {
                    syntax { literal "program"; }
                    block "{" "}" { allow_category "statement"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "constructs.langmeld".into(),
        DefinitionSource::new(
            "constructs",
            SourceFile::new(
                "language/constructs.langmeld",
                r#"construct "let" {
                    syntax { literal "let"; identifier "name"; }
                    implements "statement";
                }
                construct "print" {
                    syntax { literal "print"; identifier "name"; }
                    implements "statement";
                }"#,
            ),
        ),
    );

    let definition = parse_language_definition_project(root, &resolver);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    let block = language
        .constructs()
        .iter()
        .find(|construct| construct.name() == "program")
        .unwrap()
        .block_spec()
        .unwrap();
    assert_eq!(block.constructs(), ["let", "print"]);

    let parsed = parse(
        &SourceFile::new("main.toy", "program { let answer print answer }"),
        &language,
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].children.len(), 2);
}
