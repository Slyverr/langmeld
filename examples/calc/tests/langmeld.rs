//! Calc written in a `.langmeld` file, loaded at run time and compiled by the build
//! script, gives the same trees as Calc declared with the builder.

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
    let file = SourceFile::new(FileId::new(0), "a.calc", text).expect("small");
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("calc.langmeld");
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

/// Every source text of the corpus files that Calc is held to, and some more.
fn texts() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let folders = [
        ("handwritten-calc/corpus", None),
        (
            "../tests/real-code",
            Some(["errors.txt", "expressions.txt", "literals.txt", "names.txt"]),
        ),
    ];
    let mut texts = Vec::new();
    for (folder, only) in folders {
        let mut paths: Vec<_> = std::fs::read_dir(root.join(folder))
            .expect("the corpus folder exists")
            .map(|entry| entry.expect("readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
            .filter(|path| {
                only.is_none_or(|names| {
                    names
                        .iter()
                        .any(|name| path.file_name().is_some_and(|file| file == *name))
                })
            })
            .collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path).expect("corpus files are readable");
            let cases =
                format::parse(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            texts.extend(cases.into_iter().map(|case| case.source));
        }
    }
    let program =
        "let x = 1 + 2 * -3;\nfn f(a, b) { if a < b { a } else { f(b, a) } }\n/* c */ f(x, \"s\")";
    texts.extend((0..=program.len()).filter_map(|end| program.get(..end).map(str::to_owned)));
    texts
}

#[test]
fn the_file_gives_the_trees_of_the_builder_on_every_corpus_case() {
    let (builder, file) = (calc::language(), from_file());
    let texts = texts();
    assert!(texts.len() > 80, "{} cases", texts.len());
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
fn the_file_is_the_one_the_format_describes_and_is_accepted() {
    let loaded = load_language(Source::path(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("calc.langmeld"),
    ))
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(loaded.language.warnings().is_empty());
    assert_eq!(loaded.extensions, ["calc"]);
    assert_eq!(loaded.language.max_lookahead(), 1);
}
