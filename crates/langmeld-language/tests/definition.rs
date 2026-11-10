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
    let definition = parse_language_definition(&SourceFile::new("language.langmeld", DEFINITION));

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
        "language.langmeld",
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
        "language.langmeld",
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
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Toy" { construct "item" { } }"#,
    );

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
        "functions.langmeld",
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
fn definition_frontend_lowers_generic_token_captures() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "TaggedTokens" {
            construct "emit" {
                syntax {
                    literal "emit";
                    token "value";
                    literal ";";
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let parsed = parse(
        &SourceFile::new("main.tag", "emit @;"),
        definition.language.as_ref().unwrap(),
    );

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::Token {
            text: "@".into(),
            kind: langmeld_language::TokenKind::Punctuation,
        }
    );
}

#[test]
fn definition_frontend_lowers_expression_captures_with_consumed_terminators() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Expressions" {
            construct "print" {
                syntax {
                    literal "print";
                    expression "value" until ";";
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let program = SourceFile::new("main.expr", "print call(a + nested(b)) + 3;");
    let parsed = parse(&program, definition.language.as_ref().unwrap());

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let field = &parsed.constructs[0].fields[0];
    assert_eq!(field.value, CapturedValue::Expression);
    assert_eq!(
        &program.text[field.span.start..field.span.end],
        "call(a + nested(b)) + 3"
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
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].children[0].name, "print");
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
                location.span.end > location.span.start && location.source_name == "module.langmeld"
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
            .map(|location| location.source_name.as_str()),
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
fn definition_frontend_lowers_optional_and_repeated_syntax_groups() {
    let source = SourceFile::new(
        "functions.langmeld",
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
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].children.len(), 2);
}

#[test]
fn unknown_block_category_has_a_definition_diagnostic() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "program" {
                syntax { literal "program"; }
                block "{" "}" { allow_category "statement"; }
            }
            construct "let" {
                syntax { literal "let"; }
                implements "declaration";
            }
        }"#,
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
