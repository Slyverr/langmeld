//! `.langmeld` text of any kind: loading never panics, and every refusal is a
//! diagnostic with a valid place.

#![no_main]

use langmeld_meta::{LoadError, Source, load_language, render};
use langmeld_syntax::{FileId, SourceFile};
use libfuzzer_sys::fuzz_target;

/// Pieces of the format, so that random bytes reach deep into the loader.
const PIECES: &[&str] = &[
    "langmeld 1\n",
    "language L { extensions \"l\" comments { line \"#\" block \"/*\" \"*/\" nested } }\n",
    "tokens {\n",
    "skip ws = /[ \\t\\n]+/\n",
    "id = preset ident(xid)\n",
    "n = preset int(hex, underscores)\n",
    "s = preset string(double, interpolate \"{\" \"}\" with E)\n",
    "keywords { let }\n",
    "contextual { from }\n",
    "}\n",
    "rule File = ",
    "rule E = ",
    "Item* ",
    "items:Item+ ",
    "sep(E, \",\") ",
    "terminated(E) ",
    "try (E) ",
    "&\"}\" ",
    "!id ",
    "\"let\" ",
    "id ",
    "( ",
    ") ",
    "| ",
    "? ",
    "recover \"}\" ",
    "expr X { atom A = id\n infix M \"+\" prec 5 left\n prefix N \"-\" prec 9\n postfix C \"(\" \")\" prec 10 }\n",
    "extern rule R -> E first \"(\"\n",
    "extern token T\n",
    "bind { scope File\n File.items defines value after\n E refers value }\n",
    "editor { symbol File kind module name items\n fold File\n highlight E by_binding }\n",
    "import \"other.langmeld\"\n",
    "import \"missing.langmeld\"\n",
    "/regex/ ",
    "\"unterminated",
    "\\",
    "{ ",
    "} ",
    "//",
    "\n",
    "@",
];

/// The other file the text may import: a rule that imports back, to try cycles.
const OTHER: &str = "langmeld 1\nimport \"fuzz.langmeld\"\nrule Other = \"o\"\n";

fn check_spans(error: &LoadError) {
    assert!(!error.diagnostics.is_empty(), "a refusal says why");
    for diagnostic in &error.diagnostics {
        assert!(diagnostic.range.start() <= diagnostic.range.end());
        if let Some(file) = error.files.iter().find(|file| file.id() == diagnostic.file) {
            let end = usize::from(diagnostic.range.end());
            assert!(end <= file.text().len(), "{diagnostic:?}");
        } else {
            assert!(error.files.is_empty() || diagnostic.range.is_empty());
        }
        assert!(
            diagnostic
                .notes
                .iter()
                .any(|note| note.starts_with("To fix this")
                    || note.starts_with("In technical terms")),
            "no way to fix it is given: {diagnostic:?}"
        );
    }
    assert!(!error.rendered().is_empty());
}

fn check(text: &str) {
    if text.len() > 4096 {
        return;
    }
    let source = Source::text("fuzz.langmeld", text)
        .file("other.langmeld", OTHER)
        .generic();
    match load_language(source) {
        Ok(loaded) => {
            // A language that loads parses its own source safely and completely.
            let Ok(file) = SourceFile::new(FileId::new(0), "again", text) else {
                return;
            };
            let parsed = langmeld_syntax::parse(&loaded.language, &file);
            assert_eq!(parsed.syntax().text().to_string(), text);
            let _ = render(&loaded.files, &parsed.diagnostics);
        }
        Err(error) => check_spans(&error),
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        check(text);
    }
    let built: String = data
        .iter()
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    check(&built);
});
