//! Any text lexes into tokens that cover it, whatever the language declares.

#![no_main]

use std::sync::OnceLock;

use langmeld_syntax::{
    FileId, FloatPreset, IdentStyle, IntPreset, Interpolation, LexCursor, LexLimits, Lexer, Preset,
    SourceFile, StringParts, StringPreset, SyntaxKind, Terminators, TokenSpec,
};
use libfuzzer_sys::fuzz_target;

const PUNCT: [&str; 12] = [
    "(", ")", "[", "]", "{", "}", ",", "+", "-", "..=", ".", ">>",
];

fn kind(n: u16) -> SyntaxKind {
    SyntaxKind(n)
}

/// `r#"..."#`, which needs counting.
fn raw_string(cursor: &mut LexCursor<'_>) -> Option<usize> {
    let after_r = cursor.rest().strip_prefix('r')?;
    let hashes = after_r.len() - after_r.trim_start_matches('#').len();
    let body = after_r.get(hashes..)?.strip_prefix('"')?;
    let closer = format!("\"{}", "#".repeat(hashes));
    let end = body.find(&closer)? + closer.len();
    Some(cursor.rest().len() - body.len() + end)
}

/// A language using every kind of declaration.
fn language(terminators: bool, hole: Option<&str>, limits: bool) -> Lexer {
    let int = IntPreset::default()
        .hex(true)
        .binary(true)
        .octal(true)
        .underscores(true);
    let float = FloatPreset::default().exponent(true);
    let mut string = StringPreset::default().double(true).raw(true);
    if let Some(open) = hole {
        let parts = StringParts {
            start: kind(20),
            text: kind(21),
            end: kind(22),
            hole_open: kind(23),
            hole_close: kind(24),
        };
        string = string.interpolation(Interpolation::new(open, "}", parts));
    }

    let mut specs = vec![
        TokenSpec::regex(kind(1), "space", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(kind(2), "//"),
        TokenSpec::block_comment(kind(3), "/*", "*/").nested(),
        TokenSpec::preset(kind(4), "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::keyword(kind(5), "let"),
        TokenSpec::contextual(kind(6), "from"),
        TokenSpec::preset(kind(7), "integer", Preset::Int(int)),
        TokenSpec::preset(kind(8), "float", Preset::Float(float)),
        TokenSpec::preset(kind(9), "string", Preset::String(string)),
        TokenSpec::preset(kind(10), "character", Preset::Char),
        TokenSpec::external(kind(11), "raw string", raw_string),
        TokenSpec::regex(kind(12), "bare word", r"[a-z]+(-[a-z]+)+"),
    ];
    specs.extend(
        PUNCT
            .iter()
            .zip(100..)
            .map(|(&text, n)| TokenSpec::punct(kind(n), text)),
    );
    let mut builder = Lexer::builder().tokens(specs);
    if terminators {
        let newlines = Terminators::new(kind(30), kind(31))
            .brackets([(kind(100), kind(101)), (kind(102), kind(103))])
            .blocks([(kind(104), kind(105))])
            .continue_after([kind(106), kind(107)]);
        builder = builder.terminators(newlines);
    }
    if limits {
        builder = builder.limits(LexLimits::default().max_len(200).max_tokens(40));
    }
    builder.build().expect("the declarations are valid")
}

fn languages() -> &'static [Lexer] {
    static LANGUAGES: OnceLock<Vec<Lexer>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        vec![
            language(false, None, false),
            language(true, None, false),
            language(false, Some("{"), false),
            language(true, Some("${"), false),
            language(true, Some("{"), true),
        ]
    })
}

fuzz_target!(|input: (u8, &[u8])| {
    let (choice, bytes) = input;
    // Valid text as it is, and anything else with its bad bytes replaced.
    let text = String::from_utf8_lossy(bytes);
    let lexer = &languages()[usize::from(choice) % languages().len()];
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz", text.as_ref()) else {
        return;
    };
    let lexed = lexer.lex(&file);

    let mut end = 0;
    for (index, token) in lexed.tokens.iter().enumerate() {
        assert_eq!(usize::from(token.range.start()), end, "gap or overlap");
        assert!(!token.range.is_empty());
        end = usize::from(token.range.end());
        assert!(text.is_char_boundary(end));
        let next_is_code = lexed.tokens.get(index + 1).is_some_and(|next| !next.trivia);
        assert_eq!(token.joint, !token.trivia && next_is_code);
    }
    assert_eq!(end, text.len(), "text not covered");
    for diagnostic in &lexed.diagnostics {
        assert!(usize::from(diagnostic.range.end()) <= text.len());
    }
});
