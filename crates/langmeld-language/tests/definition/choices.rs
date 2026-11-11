use super::*;

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
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::Token {
            text: "right".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );

    let rejected = parse(&SourceFile::new("main", "move forward;"), &language);
    assert!(
        rejected
            .diagnostics()
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
                .is_some_and(|location| location.source_name() == "language.langmeld")
    }));
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
