//! The parser primitives, through a hand-written parser: any input gives a complete
//! tree, without panics or stalls, and the tree stays within its depth limit.

#![no_main]

use langmeld_syntax::{FileId, ParseLimits, SourceFile};
use libfuzzer_sys::fuzz_target;

/// Pieces of Calc, so that random bytes reach deep into the parser.
const PIECES: [&str; 28] = [
    "let ", "fn ", "if ", "else ", "true", "x", "1", "2.5", "\"s\"", "(", ")", "{", "}", ",", ";",
    "=", "==", "<", "+", "-", "*", "/", " ", "\n", "//", "/*", "*/", "@",
];

fn check(text: &str) {
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz", text) else {
        return;
    };
    let parse = handwritten_calc::parse(&file);
    let root = parse.syntax();
    assert_eq!(root.text().to_string(), text, "the tree is lossless");
    let limit = ParseLimits::default().max_tree_depth as usize;
    for node in root.descendants() {
        assert!(node.ancestors().count() <= limit, "the tree is too deep");
    }
    for diagnostic in &parse.diagnostics {
        assert!(usize::from(diagnostic.range.end()) <= text.len());
        assert_ne!(diagnostic.code, "LM-P004", "the parser stalled");
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
