use langmeld_language::CapturedValue;

#[test]
fn combines_declarative_extension_sources_with_a_rust_authored_construct() {
    let parsed = langmeld_embedded_definitions_example::parse_example().unwrap();

    assert_eq!(parsed.constructs().len(), 1);
    assert_eq!(parsed.constructs()[0].name, "program");
    assert_eq!(parsed.constructs()[0].children.len(), 2);
    assert_eq!(parsed.constructs()[0].children[0].name, "draw");
    assert_eq!(
        parsed.constructs()[0].children[0].fields[0].value,
        CapturedValue::Token {
            text: "circle".into(),
            kind: langmeld_language::TokenKind::Identifier,
        }
    );
    assert_eq!(parsed.constructs()[0].children[1].name, "host_note");
    assert_eq!(
        parsed.constructs()[0].children[1].fields[0].value,
        CapturedValue::String("\"added by Rust\"".into())
    );
}
