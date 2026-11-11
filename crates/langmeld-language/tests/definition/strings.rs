use super::*;

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
