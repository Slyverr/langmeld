//! JSON written in a `.langmeld` file, loaded at run time and compiled by the build
//! script, gives the same trees as JSON declared with the builder.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../crates/langmeld-syntax/tests/corpus/format.rs"]
mod format;

use std::path::Path;

use langmeld::{FileId, Language, Source, SourceFile, SyntaxNode, load_language, parse};

/// The whole tree, trivia included, and each diagnostic's code and place.
fn shape(language: &Language, text: &str) -> String {
    fn walk(language: &Language, node: &SyntaxNode, out: &mut String) {
        out.push('(');
        out.push_str(language.kind_name(node.kind()));
        for child in node.children_with_tokens() {
            if let Some(node) = child.as_node() {
                walk(language, node, out);
            } else if let Some(token) = child.as_token() {
                out.push_str(&format!(" {:?}", token.text()));
            }
        }
        out.push(')');
    }
    let file = SourceFile::new(FileId::new(0), "a.json", text).expect("small");
    let parsed = parse(language, &file);
    assert_eq!(parsed.syntax().text().to_string(), text);
    let mut out = String::new();
    walk(language, &parsed.syntax(), &mut out);
    for d in &parsed.diagnostics {
        out.push_str(&format!("\n{} {:?}", d.code, d.range));
    }
    out
}

fn from_file() -> Language {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("json.langmeld");
    load_language(Source::path(path))
        .unwrap_or_else(|error| panic!("{error}"))
        .language
}

fn compiled() -> Language {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/language.bin"));
    load_language(Source::embedded(bytes))
        .unwrap_or_else(|error| panic!("{error}"))
        .language
}

fn texts() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/values.txt");
    let text = std::fs::read_to_string(path).expect("the corpus is readable");
    let mut texts: Vec<String> = format::parse(&text)
        .expect("the corpus is well formed")
        .into_iter()
        .map(|case| case.source)
        .collect();
    // Every prefix of a document is a case of its own.
    let document = r#"{"a": [1, {"b": null}, -2.5e+3], "c": "d\n", "e": [true, false]}"#;
    texts.extend((0..=document.len()).map(|end| document[..end].to_owned()));
    texts
}

#[test]
fn the_file_gives_the_trees_of_the_builder_on_every_corpus_case() {
    let (builder, file) = (json::language(), from_file());
    let texts = texts();
    assert!(texts.len() > 60);
    for text in &texts {
        assert_eq!(shape(builder, text), shape(&file, text), "{text:?}");
    }
}

#[test]
fn the_compiled_language_gives_the_same_trees_as_the_loaded_file() {
    let (file, compiled) = (from_file(), compiled());
    for text in &texts() {
        assert_eq!(shape(&file, text), shape(&compiled, text), "{text:?}");
    }
}

#[test]
fn the_file_is_accepted_without_warnings() {
    let loaded = load_language(Source::path(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("json.langmeld"),
    ))
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(loaded.language.warnings().is_empty());
    assert_eq!(loaded.extensions, ["json"]);
}
