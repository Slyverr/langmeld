//! JSON documents, good and bad.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../crates/langmeld-syntax/tests/corpus/format.rs"]
mod format;
#[path = "../../../crates/langmeld-syntax/tests/support/render.rs"]
mod render;

use std::path::Path;

use langmeld_syntax::{FileId, Parse, SourceFile, SyntaxKind, SyntaxNode};

fn parsed(text: &str) -> Parse {
    let file = SourceFile::new(FileId::new(0), "test.json", text).expect("small");
    let parse = langmeld_syntax::parse(json::language(), &file);
    assert_eq!(parse.syntax().text().to_string(), text);
    parse
}

fn rendered(text: &str) -> String {
    let language = json::language();
    render::render(
        &parsed(text),
        &|kind| language.kind_name(kind).to_owned(),
        &|kind| language.is_trivia(kind),
    )
}

#[test]
fn corpus() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/values.txt");
    let text = std::fs::read_to_string(path).expect("the corpus is readable");
    let cases = format::parse(&text).expect("the corpus is well formed");
    assert!(cases.len() >= 5);
    for case in cases {
        assert_eq!(
            rendered(&case.source),
            case.expected,
            "case `{}`",
            case.name
        );
    }
}

#[test]
fn a_document_with_everything_has_no_problems() {
    let text = r#"
        {"name": "Langmeld é \" 😀", "tags": ["a", "b"], "n": [-0, 1.5e+3, 10E-2],
         "flags": {"on": true, "off": false, "none": null}, "empty": [{}, []]}
    "#;
    let parse = parsed(text);
    assert!(parse.diagnostics.is_empty(), "{:?}", parse.diagnostics);
    let language = json::language();
    let schema = language
        .schema()
        .check(&parse.syntax(), |kind| language.kind_name(kind).to_owned());
    assert!(schema.is_empty(), "{schema:?}");
}

#[test]
fn every_prefix_of_a_document_parses() {
    let text = r#"{"a": [1, {"b": null}], "c": "d"}"#;
    for end in 0..=text.len() {
        let parse = parsed(&text[..end]);
        assert_eq!(
            parse.diagnostics.is_empty(),
            end == text.len(),
            "{:?}",
            &text[..end]
        );
    }
}

#[test]
fn the_grammar_can_be_grown_for_measuring() {
    let large = json::builder(295).build().expect("the grammar is valid");
    assert!(large.warnings().is_empty());
    assert_eq!(large.schema().nodes().count(), 300);
    let file = SourceFile::new(FileId::new(0), "a.json", r#"[extraaab 1, {}]"#).expect("small");
    assert!(langmeld_syntax::parse(&large, &file).diagnostics.is_empty());
}
