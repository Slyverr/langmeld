//! JSON declared as a grammar: any input gives a complete tree, without panics or stalls.

#![no_main]

use langmeld_syntax::{FileId, Language, SourceFile};
use libfuzzer_sys::fuzz_target;

/// Pieces of JSON, so that random bytes reach deep into the grammar.
const PIECES: [&str; 20] = [
    "{", "}", "[", "]", ",", ":", "\"k\"", "\"s\\n\"", "1", "-2.5e+3", "true", "false", "null",
    " ", "\n", "\"", "\\", "e", "-", "@",
];

fn check(language: &Language, text: &str) {
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz", text) else {
        return;
    };
    let parsed = langmeld_syntax::parse(language, &file);
    let root = parsed.syntax();
    assert_eq!(root.text().to_string(), text, "the tree is lossless");
    assert!(root.descendants_with_tokens().count() > 0);
    for diagnostic in &parsed.diagnostics {
        assert!(usize::from(diagnostic.range.end()) <= text.len());
        assert_ne!(diagnostic.code, "LM-P004", "the parser stalled");
    }
    if parsed.diagnostics.is_empty() {
        let broken = language
            .schema()
            .check(&root, |kind| language.kind_name(kind).to_owned());
        assert!(
            broken.is_empty(),
            "a sound tree breaks the schema: {broken:?}"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let language = json::language();
    if let Ok(text) = std::str::from_utf8(data) {
        check(language, text);
    }
    let built: String = data
        .iter()
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    check(language, &built);
});
