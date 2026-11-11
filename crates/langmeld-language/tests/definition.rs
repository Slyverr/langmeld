use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{
    CapturedValue, CompletionKind, DefinitionResolver, DefinitionResult, DefinitionSource,
    complete_at, decode_definition_string, parse, parse_language_definition,
    parse_language_definition_project, quote_definition_string,
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
fn definition_string_helpers_round_trip_supported_escapes() {
    let value = "quotes: \" slash: \\ newline:\nreturn:\rtab:\t";
    let encoded = quote_definition_string(value);

    assert_eq!(
        encoded,
        "\"quotes: \\\" slash: \\\\ newline:\\nreturn:\\rtab:\\t\""
    );
    assert_eq!(decode_definition_string(&encoded).as_deref(), Some(value));
}

#[test]
fn decode_definition_string_rejects_invalid_literals_and_escapes() {
    let invalid_values = [
        "",
        "\"unterminated",
        "\"unsupported\\q\"",
        "\"unescaped\"quote\"",
        "\"trailing\\",
    ];

    for value in invalid_values {
        assert_eq!(decode_definition_string(value), None, "{value:?}");
    }
}

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

    let top_level_statement = parse(&SourceFile::new("main.toy", "let answer = 42;"), &language);
    assert!(top_level_statement.diagnostics.is_empty());
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
fn parses_finite_choice_fields_and_uses_them_for_validation_and_completion() {
    let definition = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"
language "Movement" {
    root "move";
    construct "move" {
        syntax {
            literal "move";
            choice "direction" {
                value "left";
                value "right";
            }
            literal ";";
        }
    }
}
"#,
    ));
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();

    let parsed = parse(&SourceFile::new("main", "move right;"), &language);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::Token {
            text: "right".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );

    let rejected = parse(&SourceFile::new("main", "move forward;"), &language);
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-PARSE-002")
    );

    let source = SourceFile::new("main", "move r");
    let candidates = complete_at(&source, &language, source.text().len());
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["right"]
    );
}

#[test]
fn empty_choice_declarations_have_source_located_diagnostics() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Movement" {
            construct "move" {
                syntax {
                    choice "direction" { }
                }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);

    assert!(definition.language.is_none());
    assert!(definition.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "LM-DEF-028"
            && diagnostic
                .location
                .as_ref()
                .is_some_and(|location| location.source_name == "language.langmeld")
    }));
}

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
    assert!(!invalid_root.diagnostics.is_empty());
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
    assert_eq!(location.span.start, root_start);
    assert!(definition.language.is_none());
}

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
    assert_eq!(location.span.start, duplicate_start);
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
fn construct_completion_kind_is_declared_and_used_by_grammar_completion() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "print" {
                completion keyword;
                syntax { literal "print"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    assert_eq!(
        language.constructs()[0].completion_kind(),
        Some(CompletionKind::Keyword)
    );

    let suggestions = complete_at(&SourceFile::new("main", ""), &language, 0);
    assert_eq!(suggestions[0].text, "print");
    assert_eq!(suggestions[0].kind, Some(CompletionKind::Keyword));
}

#[test]
fn choice_completion_kind_can_be_overridden_in_langmeld() {
    let definition = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"
language "Movement" {
    construct "move" {
        syntax {
            literal "move";
            choice "direction" {
                completion keyword;
                value "left";
                value "right";
            }
        }
    }
}
"#,
    ));
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    let source = SourceFile::new("main", "move r");

    let candidates = complete_at(&source, &language, source.text().len());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "right");
    assert_eq!(candidates[0].kind, Some(CompletionKind::Keyword));
}

#[test]
fn definition_frontend_accepts_each_completion_kind_name() {
    let kinds = [
        ("text", CompletionKind::Text),
        ("method", CompletionKind::Method),
        ("function", CompletionKind::Function),
        ("constructor", CompletionKind::Constructor),
        ("field", CompletionKind::Field),
        ("variable", CompletionKind::Variable),
        ("class", CompletionKind::Class),
        ("interface", CompletionKind::Interface),
        ("module", CompletionKind::Module),
        ("property", CompletionKind::Property),
        ("unit", CompletionKind::Unit),
        ("value", CompletionKind::Value),
        ("enum", CompletionKind::Enum),
        ("keyword", CompletionKind::Keyword),
        ("snippet", CompletionKind::Snippet),
        ("color", CompletionKind::Color),
        ("file", CompletionKind::File),
        ("reference", CompletionKind::Reference),
        ("folder", CompletionKind::Folder),
        ("enum_member", CompletionKind::EnumMember),
        ("constant", CompletionKind::Constant),
        ("struct", CompletionKind::Struct),
        ("event", CompletionKind::Event),
        ("operator", CompletionKind::Operator),
        ("type_parameter", CompletionKind::TypeParameter),
    ];

    for (name, expected) in kinds {
        let source = SourceFile::new(
            "language.langmeld",
            format!(
                "language \"Toy\" {{ construct \"item\" {{ completion \"{name}\"; syntax {{ literal \"item\"; }} }} }}"
            ),
        );
        let definition = parse_language_definition(&source);

        assert!(
            definition.is_valid(),
            "{name}: {:?}",
            definition.diagnostics
        );
        assert_eq!(
            definition.language.unwrap().constructs()[0].completion_kind(),
            Some(expected),
            "completion kind `{name}`"
        );

        let bare_source = SourceFile::new(
            "language.langmeld",
            format!(
                "language \"Toy\" {{ construct \"item\" {{ completion {name}; syntax {{ literal \"item\"; }} }} }}"
            ),
        );
        let bare_definition = parse_language_definition(&bare_source);
        assert!(
            bare_definition.is_valid(),
            "unquoted {name}: {:?}",
            bare_definition.diagnostics
        );
        assert_eq!(
            bare_definition.language.unwrap().constructs()[0].completion_kind(),
            Some(expected),
            "unquoted completion kind `{name}`"
        );
    }
}

#[test]
fn invalid_and_duplicate_construct_completion_kinds_are_diagnosed() {
    let invalid = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "print" {
                completion "keywordish";
                syntax { literal "print"; }
            }
        }"#,
    ));
    assert!(invalid.language.is_none());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-031")
    );

    let duplicate = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "print" {
                completion "keyword";
                completion "function";
                syntax { literal "print"; }
            }
        }"#,
    ));
    assert!(duplicate.language.is_none());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-029")
    );
}

#[test]
fn invalid_and_duplicate_choice_completion_kinds_are_diagnosed() {
    let invalid = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "move" {
                syntax {
                    choice "direction" {
                        completion "keywordish";
                        value "left";
                    }
                }
            }
        }"#,
    ));
    assert!(invalid.language.is_none());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-033")
    );

    let duplicate = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r#"language "Toy" {
            construct "move" {
                syntax {
                    choice "direction" {
                        completion "keyword";
                        completion "enum_member";
                        value "left";
                    }
                }
            }
        }"#,
    ));
    assert!(duplicate.language.is_none());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-DEF-032")
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
        &program.text()[field.span.start..field.span.end],
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

#[test]
fn definition_lexical_rules_configure_comments_for_parsing_and_allow_definition_comments() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"// Comments in definition files are recognized by the built-in meta-grammar.
        language "Toy" {
            lexical {
                line_comment "//";
                block_comment "/*" "*/";
            }
            construct "print" {
                syntax { literal "print"; string "value"; literal ";"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    assert_eq!(language.lexical_rules().line_comments(), ["//"]);
    assert_eq!(
        language.lexical_rules().block_comments(),
        [("/*".to_owned(), "*/".to_owned())]
    );

    let parsed = parse(
        &SourceFile::new("main.toy", "// before\nprint \"ok\"; /* after */"),
        &language,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs.len(), 1);
}

#[test]
fn invalid_definition_comment_rules_report_at_the_lexical_block() {
    let source = SourceFile::new(
        "language.langmeld",
        r##"language "Toy" {
            lexical {
                line_comment "#";
                block_comment "#" "*/";
            }
            construct "item" { syntax { literal "item"; } }
        }"##,
    );

    let definition = parse_language_definition(&source);

    assert!(definition.language.is_none());
    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-004")
        .unwrap();
    let span = diagnostic.location.as_ref().unwrap().span;
    let text = &source.text()[span.start..span.end];
    assert!(text.contains("lexical"));
    assert!(text.contains("block_comment"));
}
