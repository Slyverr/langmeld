//! Editing a grammar file changes how files parse at the next load, with no rebuild.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_meta::{Source, load_language};
use langmeld_syntax::{FileId, SourceFile, parse};

const BEFORE: &str = "\
langmeld 1
language Notes { extensions \"notes\" }
tokens {
    skip space = /[ \\t\\n]+/
    word = preset ident(ascii)
}
rule File = Note*
rule Note = \"note\" text:word
";

/// The same language with a second kind of note.
const AFTER: &str = "\
langmeld 1
language Notes { extensions \"notes\" }
tokens {
    skip space = /[ \\t\\n]+/
    word = preset ident(ascii)
}
rule File = Note*
rule Note = \"note\" text:word | \"todo\" text:word
";

fn parsed(path: &std::path::Path, text: &str) -> (usize, usize) {
    let loaded = load_language(Source::path(path)).unwrap_or_else(|error| panic!("{error}"));
    let file = SourceFile::new(FileId::new(0), "a.notes", text).expect("small");
    let parsed = parse(&loaded.language, &file);
    (parsed.syntax().children().count(), parsed.diagnostics.len())
}

#[test]
fn an_edited_grammar_reparses_an_example_with_no_rebuild() {
    let folder = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hot-reload");
    std::fs::create_dir_all(&folder).expect("a folder");
    let grammar = folder.join("notes.langmeld");
    let example = "note milk\ntodo bread\nnote eggs";

    std::fs::write(&grammar, BEFORE).expect("written");
    // `todo` is unknown, so it is one mistake.
    assert_eq!(parsed(&grammar, example), (3, 1));

    std::fs::write(&grammar, AFTER).expect("written");
    assert_eq!(parsed(&grammar, example), (3, 0));

    // A mistake in the edit is reported at its place, and the old language is not kept.
    std::fs::write(&grammar, AFTER.replace("Note*", "Nope*")).expect("written");
    let error = load_language(Source::path(&grammar)).expect_err("`Nope` is not declared");
    assert_eq!(error.diagnostics[0].code, "LM-G001");
}
