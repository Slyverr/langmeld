use super::*;

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
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].children.len(), 2);
    assert_eq!(
        parsed.constructs()[0].children[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );

    let top_level_statement = parse(&SourceFile::new("main.toy", "let answer = 42;"), &language);
    assert!(top_level_statement.diagnostics().is_empty());
}

#[test]
fn definition_validity_ignores_warnings_but_rejects_errors() {
    let source = SourceFile::new("language.langmeld", DEFINITION);
    let language = parse_language_definition(&source).language.unwrap();
    let warning = Diagnostic::warning(
        "LM-DEF-W001",
        "style recommendation",
        &source,
        Span::new(0, 1),
    );
    let info = Diagnostic::info(
        "LM-DEF-I001",
        "additional context",
        &source,
        Span::new(0, 1),
    );

    let result = DefinitionResult {
        language: Some(language.clone()),
        diagnostics: vec![warning, info],
    };

    assert!(result.is_valid());

    let result = DefinitionResult {
        language: Some(language),
        diagnostics: vec![Diagnostic::error(
            "LM-DEF-E001",
            "invalid language declaration",
            &source,
            Span::new(0, 1),
        )],
    };

    assert!(!result.is_valid());
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
    let span = diagnostic.location.as_ref().unwrap().span();
    assert!(span.end > span.start);
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
                syntax "short" {
                    literal "fn";
                    identifier "name";
                }
                syntax "parenthesized" {
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

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].variant, 1);
    let function = definition
        .language
        .as_ref()
        .unwrap()
        .constructs()
        .iter()
        .find(|construct| construct.name() == "function")
        .unwrap();
    assert_eq!(function.variant_name(0), Some("short"));
    assert_eq!(function.variant_name(1), Some("parenthesized"));
}

#[test]
fn definition_frontend_reports_duplicate_syntax_names_at_the_later_variant() {
    let source = SourceFile::new(
        "functions.langmeld",
        r#"language "Functions" {
            construct "function" {
                syntax "body" { literal "fn"; }
                syntax "body" { literal "function"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.language.is_none());

    let duplicate = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-035")
        .expect("the later syntax label should be diagnosed");
    let span = duplicate.location.as_ref().unwrap().span();
    assert_eq!(
        span.start,
        source.text().rfind("syntax \"body\" {").unwrap()
    );
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

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
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

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let field = &parsed.constructs()[0].fields[0];
    assert_eq!(field.value, CapturedValue::Expression);
    assert_eq!(
        &program.text()[field.span.start..field.span.end],
        "call(a + nested(b)) + 3"
    );
}

#[test]
fn definition_frontend_reports_empty_group_names_at_the_group() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Names" {
            construct "item" {
                syntax {
                    literal "item";
                    optional "  " { literal "value"; }
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.language.is_none());

    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-036")
        .expect("the empty group name should be diagnosed");
    let span = diagnostic.location.as_ref().unwrap().span();
    assert_eq!(span.start, source.text().find("optional").unwrap());
}

#[test]
fn definition_frontend_reports_duplicate_group_names_at_the_later_group() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Names" {
            construct "item" {
                syntax {
                    literal "item";
                    optional "body" { literal "first"; }
                    optional "body" { literal "second"; }
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.language.is_none());

    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-037")
        .expect("the duplicate group name should be diagnosed");
    let span = diagnostic.location.as_ref().unwrap().span();
    assert_eq!(span.start, source.text().rfind("optional").unwrap());
}

#[test]
fn definition_frontend_reports_a_field_that_reuses_a_group_name() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Names" {
            construct "item" {
                syntax {
                    literal "item";
                    optional "value" { literal "optional"; }
                    identifier "value";
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.language.is_none());

    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-037")
        .expect("the field/group name collision should be diagnosed");
    let span = diagnostic.location.as_ref().unwrap().span();
    assert_eq!(span.start, source.text().find("identifier").unwrap());
}

#[test]
fn definition_frontend_lowers_optional_and_repeated_syntax_groups() {
    let source = SourceFile::new(
        "functions.langmeld",
        r#"language "Functions" {
            construct "function" {
                syntax "signature" {
                    literal "fn";
                    identifier "name";
                    literal "(";
                    optional "parameters" {
                        identifier "first_parameter";
                        literal ":";
                        identifier "first_type";
                        zero_or_more "additional_parameters" {
                            literal ",";
                            identifier "parameter";
                            literal ":";
                            identifier "parameter_type";
                        }
                    }
                    literal ")";
                    optional "return_type_clause" {
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

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].fields.len(), 6);
    assert_eq!(
        parsed.constructs()[0].capture_groups[0].name.as_deref(),
        Some("parameters")
    );
    assert_eq!(
        parsed.constructs()[0].capture_groups[0].occurrences[0].groups[0]
            .name
            .as_deref(),
        Some("additional_parameters")
    );
    assert_eq!(
        parsed.constructs()[0].capture_groups[1].name.as_deref(),
        Some("return_type_clause")
    );
}
