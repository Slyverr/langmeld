//! Strings with holes: random braces, quotes and holes never put the lexer out of
//! step. The tokens cover the text, every string that starts also ends, and a
//! string that never ends is reported once.

#![no_main]

use std::sync::OnceLock;

use langmeld_syntax::{
    FileId, IdentStyle, Interpolation, Lexer, Preset, SourceFile, StringParts, StringPreset,
    SyntaxKind, TokenSpec,
};
use libfuzzer_sys::fuzz_target;

const START: SyntaxKind = SyntaxKind(20);
const TEXT: SyntaxKind = SyntaxKind(21);
const END: SyntaxKind = SyntaxKind(22);
const HOLE_OPEN: SyntaxKind = SyntaxKind(23);
const HOLE_CLOSE: SyntaxKind = SyntaxKind(24);
const UNTERMINATED: &str = "LM-L002";

/// Pieces of text with strings, so that random bytes make deep and broken nesting.
const PIECES: [&str; 20] = [
    "\"", "\"", "'", "{", "}", "${", "{{", "}}", "\\\"", "\\{", "\\", "x", "1", "(", ")", " ",
    "\n", "//", "+", "é",
];

fn language(open: &str) -> Lexer {
    let parts = StringParts {
        start: START,
        text: TEXT,
        end: END,
        hole_open: HOLE_OPEN,
        hole_close: HOLE_CLOSE,
    };
    let strings = StringPreset::default()
        .double(true)
        .single(true)
        .interpolation(Interpolation::new(open, "}", parts));
    let mut specs = vec![
        TokenSpec::regex(SyntaxKind(1), "space", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(SyntaxKind(2), "//"),
        TokenSpec::preset(SyntaxKind(3), "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::regex(SyntaxKind(4), "number", "[0-9]+"),
        TokenSpec::preset(SyntaxKind(5), "string", Preset::String(strings)),
    ];
    for (n, text) in ["{", "}", "(", ")", "+"].into_iter().enumerate() {
        specs.push(TokenSpec::punct(SyntaxKind(10 + n as u16), text));
    }
    Lexer::builder()
        .tokens(specs)
        .build()
        .expect("the declarations are valid")
}

fn languages() -> &'static [Lexer] {
    static LANGUAGES: OnceLock<Vec<Lexer>> = OnceLock::new();
    LANGUAGES.get_or_init(|| vec![language("{"), language("${")])
}

fn check(choice: u8, text: &str) {
    let lexer = &languages()[usize::from(choice) % languages().len()];
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz", text) else {
        return;
    };
    let lexed = lexer.lex(&file);

    // The tokens cover the text, in order.
    let mut end = 0;
    for token in &lexed.tokens {
        assert_eq!(usize::from(token.range.start()), end, "gap or overlap");
        assert!(!token.range.is_empty());
        end = usize::from(token.range.end());
        assert!(text.is_char_boundary(end));
    }
    assert_eq!(end, text.len(), "text not covered");

    // Each string needs exactly one report when it never ends: starts are
    // matched with ends, and a report closes the string it names.
    let mut reports: Vec<_> = lexed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == UNTERMINATED)
        .map(|diagnostic| diagnostic.range)
        .collect();
    reports.sort_by_key(|range| range.start());
    for pair in reports.windows(2) {
        assert_ne!(pair[0].start(), pair[1].start(), "string reported twice");
    }

    let mut open: Vec<usize> = Vec::new();
    let mut holes = 0usize;
    let mut used = 0;
    for token in &lexed.tokens {
        let start = usize::from(token.range.start());
        // A report ends the open strings at its end.
        if let Some(&outer) = open.first()
            && let Some(report) = reports.iter().find(|r| usize::from(r.start()) == outer)
            && start >= usize::from(report.end())
        {
            open.clear();
            holes = 0;
            used += 1;
        }
        match token.kind {
            START => open.push(start),
            END => {
                assert!(open.pop().is_some(), "a string ended without starting");
            }
            HOLE_OPEN => {
                assert!(!open.is_empty(), "a hole opened outside a string");
                holes += 1;
            }
            HOLE_CLOSE => {
                assert!(holes > 0, "a hole closed without opening");
                holes -= 1;
            }
            TEXT => assert!(!open.is_empty(), "string text outside a string"),
            _ => {}
        }
    }
    if let Some(&outer) = open.first() {
        assert!(
            reports.iter().any(|r| usize::from(r.start()) == outer),
            "a string never ended and was not reported"
        );
        used += 1;
    }
    assert_eq!(used, reports.len(), "a report names no open string");
}

fuzz_target!(|data: (u8, &[u8])| {
    let (choice, bytes) = data;
    if let Ok(text) = std::str::from_utf8(bytes) {
        check(choice, text);
    }
    let built: String = bytes
        .iter()
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    check(choice, &built);
});
