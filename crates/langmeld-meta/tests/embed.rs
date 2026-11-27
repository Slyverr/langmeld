//! A language compiled ahead of time parses exactly like the one loaded from its file.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_meta::build::{SYNTAX_NAME, compile, compile_with_types, write};
use langmeld_meta::{Source, load_language};
use langmeld_syntax::{FileId, Language, SourceFile, parse};

fn shown(language: &Language, text: &str) -> String {
    let file = SourceFile::new(FileId::new(0), "a", text).expect("small");
    let parsed = parse(language, &file);
    let mut out = format!("{:#?}\n", parsed.syntax());
    for d in &parsed.diagnostics {
        out.push_str(&format!("{} {:?} {}\n", d.code, d.range, d.message));
    }
    out
}

const TEXTS: [&str; 5] = [
    "let x = 1 + 2 * 3;\nfn f(a, b) { a(b) }",
    "let = ;",
    "fn (",
    "if true { 1 } else { 2 } // done",
    "",
];

fn grammar() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/grammars/calc-format.langmeld"
    ))
    .expect("the grammar is readable")
}

#[test]
fn the_embedded_language_parses_like_the_loaded_one() {
    let loaded = load_language(Source::text("calc.langmeld", grammar())).expect("valid");
    let bytes = write(&loaded).expect("written");
    let embedded = load_language(Source::embedded(&bytes)).expect("read back");
    assert_eq!(embedded.extensions, loaded.extensions);
    assert_eq!(embedded.bindings, loaded.bindings);
    assert_eq!(embedded.editor, loaded.editor);
    for text in TEXTS {
        assert_eq!(
            shown(&loaded.language, text),
            shown(&embedded.language, text),
            "{text:?}"
        );
    }
}

#[test]
fn a_project_compiled_by_a_build_script_is_read_back() {
    let folder = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("compiled-project");
    std::fs::create_dir_all(folder.join("language")).expect("a folder");
    std::fs::write(folder.join("language/main.langmeld"), grammar()).expect("written");
    let out = folder.join("language.bin");
    let compiled = compile(&folder, &out).expect("compiles");
    assert_eq!(compiled.read.len(), 1);
    let bytes = std::fs::read(&out).expect("written");
    let embedded = load_language(Source::embedded(&bytes)).expect("read back");
    assert_eq!(embedded.extensions, ["calc"]);
}

#[test]
fn a_project_compiled_with_types_has_the_types_beside_the_language() {
    let folder = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("typed-project");
    std::fs::create_dir_all(folder.join("language")).expect("a folder");
    std::fs::write(folder.join("language/main.langmeld"), grammar()).expect("written");
    let compiled = compile_with_types(&folder, &folder).expect("compiles");
    assert_eq!(compiled.read.len(), 1);
    let types = std::fs::read_to_string(folder.join(SYNTAX_NAME)).expect("written");
    assert!(types.contains("pub struct LetStmt("), "{types}");
    assert!(folder.join("language.bin").is_file());
}

#[test]
fn a_project_that_does_not_compile_says_what_is_wrong() {
    let folder = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("broken-project");
    std::fs::create_dir_all(folder.join("language")).expect("a folder");
    std::fs::write(
        folder.join("language/main.langmeld"),
        "language Calc { }\nrule File = \"a\"\n",
    )
    .expect("written");
    let error = compile(&folder, &folder.join("language.bin")).expect_err("no version line");
    assert!(
        error
            .to_string()
            .starts_with("the grammar cannot be compiled\n\nerror[LM-F001]")
    );
}

#[test]
fn damaged_bytes_are_one_diagnostic() {
    let loaded = load_language(Source::text("calc.langmeld", grammar())).expect("valid");
    let mut bytes = write(&loaded).expect("written");
    bytes.truncate(bytes.len() / 2);
    let error = load_language(Source::embedded(&bytes)).expect_err("damaged");
    insta::assert_snapshot!(error.rendered());
}
