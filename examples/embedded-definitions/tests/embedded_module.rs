use langmeld_language::CapturedValue;

#[test]
fn parses_a_program_using_a_rust_embedded_definition_module() {
    let parsed = langmeld_embedded_definitions_example::parse_example().unwrap();

    assert_eq!(parsed.constructs().len(), 1);
    assert_eq!(parsed.constructs()[0].name, "program");
    assert_eq!(parsed.constructs()[0].children.len(), 1);
    assert_eq!(parsed.constructs()[0].children[0].name, "draw");
    assert_eq!(
        parsed.constructs()[0].children[0].fields[0].value,
        CapturedValue::Token {
            text: "circle".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );
}
