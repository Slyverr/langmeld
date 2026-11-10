use langmeld_core::SourceFile;
use langmeld_language::{CapturedValue, parse, parse_language_definition};

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
