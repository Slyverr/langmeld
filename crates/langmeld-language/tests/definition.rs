use langmeld_core::SourceFile;
use langmeld_language::{
    CapturedValue, DefinitionResolver, DefinitionSource, parse, parse_language_definition,
    parse_language_definition_project,
};
use std::collections::HashMap;

#[derive(Default)]
struct Resolver(HashMap<String, DefinitionSource>);

impl DefinitionResolver for Resolver {
    fn resolve(&self, _from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        self.0
            .get(import)
            .cloned()
            .ok_or_else(|| format!("unknown test import `{import}`"))
    }
}

const DEFINITION: &str = r#"
language "Toy" {
    construct "program" {
        syntax {
            literal "program";
        }
        block "{" "}" {
            allow "let";
            allow "print";
        }
    }
    construct "let" {
        syntax {
            literal "let";
            identifier "name";
            literal "=";
            integer "value";
            literal ";";
        }
    }
    construct "print" {
        syntax {
            literal "print";
            identifier "name";
            literal ";";
        }
    }
}
"#;

#[test]
fn parses_definition_with_the_generic_parser_and_builds_a_usable_language() {
    let definition = parse_language_definition(&SourceFile::new("language.lm", DEFINITION));

    assert!(
        definition.diagnostics.is_empty(),
        "{:?}",
        definition.diagnostics
    );
    let language = definition.language.unwrap();
    assert_eq!(language.name(), "Toy");

    let source = SourceFile::new("main.toy", "program { let answer = 42; print answer; }");
    let parsed = parse(&source, &language);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].children.len(), 2);
    assert_eq!(
        parsed.constructs[0].children[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
}

#[test]
fn definition_errors_have_source_locations() {
    let source = SourceFile::new(
        "language.lm",
        r#"language "Toy" { construct "program" { syntax { literal "program"; } block "{" "}" { allow "missing"; } } }"#,
    );

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    let diagnostic = result.diagnostics.last().unwrap();
    assert_eq!(diagnostic.code, "LM-DEF-004");
    let span = diagnostic.location.as_ref().unwrap().span;
    assert!(span.end > span.start);
}

#[test]
fn reports_invalid_definition_string_escapes() {
    let source = SourceFile::new(
        "language.lm",
        r#"language "Toy\q" { construct "item" { syntax { literal "item"; } } }"#,
    );

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-003")
    );
}

#[test]
fn requires_at_least_one_syntax_block_per_construct() {
    let source = SourceFile::new("language.lm", r#"language "Toy" { construct "item" { } }"#);

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-005")
    );
}

#[test]
fn definition_frontend_lowers_multiple_syntax_variants() {
    let source = SourceFile::new(
        "functions.lm",
        r#"language "Functions" {
            construct "function" {
                syntax {
                    literal "fn";
                    identifier "name";
                }
                syntax {
                    literal "function";
                    identifier "name";
                    literal "(";
                    literal ")";
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let parsed = parse(
        &SourceFile::new("functions", "function render()"),
        definition.language.as_ref().unwrap(),
    );

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].variant, 1);
}

#[test]
fn composes_nested_imports_and_keeps_construct_source_locations() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new(
            "language/main.lm",
            r#"language "Widget" {
                import "statements.lm";
                import "statements.lm";
                construct "program" {
                    syntax { literal "program"; }
                    block "{" "}" { allow "print"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "statements.lm".into(),
        DefinitionSource::new(
            "statements",
            SourceFile::new(
                "language/statements.lm",
                r#"import "values.lm";
                construct "print" {
                    syntax { literal "print"; identifier "name"; literal ";"; }
                }"#,
            ),
        ),
    );
    resolver.0.insert(
        "values.lm".into(),
        DefinitionSource::new(
            "values",
            SourceFile::new(
                "language/values.lm",
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
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].children[0].name, "print");
}

#[test]
fn reports_import_cycles_and_missing_imports_at_the_import_site() {
    let root = DefinitionSource::new(
        "root",
        SourceFile::new("main.lm", r#"language "Cycle" { import "module.lm"; }"#),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "module.lm".into(),
        DefinitionSource::new(
            "module",
            SourceFile::new("module.lm", r#"import "root.lm";"#),
        ),
    );
    resolver.0.insert(
        "root.lm".into(),
        DefinitionSource::new(
            "root",
            SourceFile::new("main.lm", r#"language "Cycle" { import "module.lm"; }"#),
        ),
    );

    let cycle = parse_language_definition_project(root, &resolver);

    assert!(cycle.language.is_none());
    assert!(cycle.diagnostics.iter().any(|d| d.code == "LM-DEF-020"));
    assert!(cycle.diagnostics.iter().any(|d| {
        d.code == "LM-DEF-020"
            && d.location.as_ref().is_some_and(|location| {
                location.span.end > location.span.start && location.source_name == "module.lm"
            })
    }));

    let missing = parse_language_definition_project(
        DefinitionSource::new(
            "missing-root",
            SourceFile::new("main.lm", r#"language "Missing" { import "none.lm"; }"#),
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
            "language/main.lm",
            r#"language "Duplicate" {
                import "constructs.lm";
                construct "print" {
                    syntax { literal "print"; identifier "name"; }
                }
            }"#,
        ),
    );
    let mut resolver = Resolver::default();
    resolver.0.insert(
        "constructs.lm".into(),
        DefinitionSource::new(
            "constructs",
            SourceFile::new(
                "language/constructs.lm",
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
            .map(|location| location.source_name.as_str()),
        Some("language/constructs.lm")
    );
}

#[test]
fn single_file_definition_api_reports_when_imports_need_a_resolver() {
    let source = SourceFile::new(
        "main.lm",
        r#"language "Imported" { import "constructs.lm"; }"#,
    );

    let result = parse_language_definition(&source);

    assert!(result.language.is_none());
    assert!(result.diagnostics.iter().any(|d| d.code == "LM-DEF-010"));
}

#[test]
fn definition_frontend_lowers_optional_and_repeated_syntax_groups() {
    let source = SourceFile::new(
        "functions.lm",
        r#"language "Functions" {
            construct "function" {
                syntax {
                    literal "fn";
                    identifier "name";
                    literal "(";
                    optional {
                        identifier "first_parameter";
                        literal ":";
                        identifier "first_type";
                        zero_or_more {
                            literal ",";
                            identifier "parameter";
                            literal ":";
                            identifier "parameter_type";
                        }
                    }
                    literal ")";
                    optional {
                        literal "->";
                        identifier "return_type";
                    }
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let parsed = parse(
        &SourceFile::new("functions", "fn join(left: Text, right: Text) -> Text"),
        definition.language.as_ref().unwrap(),
    );

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].fields.len(), 6);
}
