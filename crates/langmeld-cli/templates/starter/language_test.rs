use langmeld::core::SourceFile;
use @CRATE@::{load_language, parse_source};

#[test]
fn starter_definition_and_example_parse() {
    let language = load_language().expect("modular definition validates");
    assert_eq!(language.spec().name, "@NAME@");

    let source = SourceFile::new(
        "hello.@EXT@",
        include_str!("../examples/hello.@EXT@"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].construct, "program");
    assert_eq!(parsed.program.nodes[0].children[0].construct, "statement");
}
