//! A small C-like language used by the lexer and parser tests.

// Each test target uses its own part of this module.
#![allow(dead_code)]

use langmeld_syntax::{
    FileId, FloatPreset, IdentStyle, IntPreset, Interpolation, Lexed, Lexer, Preset, SourceFile,
    StringParts, StringPreset, SyntaxKind, Terminators, TokenSpec,
};

macro_rules! kinds {
    ($($name:ident)*) => {
        kinds!(@ 1, $($name)*);
        /// The name of a kind, for readable test output.
        pub fn kind_name(kind: SyntaxKind) -> &'static str {
            match kind {
                $($name => stringify!($name),)*
                SyntaxKind::ERROR => "ERROR",
                SyntaxKind::BOM => "BOM",
                _ => "?",
            }
        }
    };
    (@ $n:expr, $name:ident $($rest:ident)*) => {
        pub const $name: SyntaxKind = SyntaxKind($n);
        kinds!(@ $n + 1, $($rest)*);
    };
    (@ $n:expr,) => {};
}

kinds! {
    WS LINE_COMMENT BLOCK_COMMENT DOC_COMMENT
    IDENT BARE INT FLOAT DURATION STRING CHAR
    LET_KW FN_KW FROM_KW
    EQ EQ_EQ NOT_EQ NOT MINUS PLUS STAR SLASH LT GT
    DOT DOT_DOT DOT_DOT_EQ COMMA SEMI COLON ARROW
    L_PAREN R_PAREN L_BRACK R_BRACK L_BRACE R_BRACE
    NEWLINE LINE_BREAK
    STR_START STR_TEXT STR_END HOLE_OPEN HOLE_CLOSE
    RAW_STRING
    FILE LET_STMT NAME NAME_REF LITERAL BINARY PAREN CALL BLOCK TYPE
}

pub fn puncts() -> Vec<TokenSpec> {
    [
        (EQ, "="),
        (EQ_EQ, "=="),
        (NOT_EQ, "!="),
        (NOT, "!"),
        (MINUS, "-"),
        (PLUS, "+"),
        (STAR, "*"),
        (SLASH, "/"),
        (LT, "<"),
        (GT, ">"),
        (DOT, "."),
        (DOT_DOT, ".."),
        (DOT_DOT_EQ, "..="),
        (COMMA, ","),
        (SEMI, ";"),
        (COLON, ":"),
        (ARROW, "->"),
        (L_PAREN, "("),
        (R_PAREN, ")"),
        (L_BRACK, "["),
        (R_BRACK, "]"),
        (L_BRACE, "{"),
        (R_BRACE, "}"),
    ]
    .into_iter()
    .map(|(kind, text)| TokenSpec::punct(kind, text))
    .collect()
}

pub fn int() -> IntPreset {
    IntPreset::default()
        .hex(true)
        .binary(true)
        .octal(true)
        .underscores(true)
}

pub fn float() -> FloatPreset {
    FloatPreset::default().exponent(true).underscores(true)
}

pub fn double_quoted() -> StringPreset {
    StringPreset::default().double(true)
}

/// Everything but terminators and interpolation.
pub fn c_like() -> Lexer {
    let mut specs = vec![
        TokenSpec::regex(WS, "whitespace", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(LINE_COMMENT, "//"),
        TokenSpec::line_comment(DOC_COMMENT, "///").doc(),
        TokenSpec::block_comment(BLOCK_COMMENT, "/*", "*/").nested(),
        TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::regex(BARE, "bare word", r"[a-z]+(-[a-z]+)+"),
        TokenSpec::preset(INT, "integer", Preset::Int(int())),
        TokenSpec::preset(FLOAT, "float", Preset::Float(float())),
        TokenSpec::regex(DURATION, "duration", r"[0-9]+(ms|s)"),
        TokenSpec::preset(STRING, "string", Preset::String(double_quoted())),
        TokenSpec::preset(CHAR, "character", Preset::Char),
        TokenSpec::keyword(LET_KW, "let"),
        TokenSpec::keyword(FN_KW, "fn"),
        TokenSpec::contextual(FROM_KW, "from"),
    ];
    specs.extend(puncts());
    Lexer::builder()
        .tokens(specs)
        .build()
        .expect("the test language is valid")
}

/// Newlines end statements.
pub fn terminated() -> Lexer {
    let mut specs = puncts();
    specs.extend([
        TokenSpec::regex(WS, "whitespace", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(LINE_COMMENT, "//"),
        TokenSpec::block_comment(BLOCK_COMMENT, "/*", "*/"),
        TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Ascii)),
    ]);
    let terminators = Terminators::new(NEWLINE, LINE_BREAK)
        .brackets([(L_PAREN, R_PAREN), (L_BRACK, R_BRACK)])
        .blocks([(L_BRACE, R_BRACE)])
        .continue_after([PLUS, COMMA]);
    Lexer::builder()
        .tokens(specs)
        .terminators(terminators)
        .build()
        .expect("valid")
}

/// Strings with holes between `open` and `}`.
pub fn interpolating(open: &str) -> Lexer {
    let parts = StringParts {
        start: STR_START,
        text: STR_TEXT,
        end: STR_END,
        hole_open: HOLE_OPEN,
        hole_close: HOLE_CLOSE,
    };
    let preset = double_quoted().interpolation(Interpolation::new(open, "}", parts));
    let mut specs = puncts();
    specs.extend([
        TokenSpec::regex(WS, "whitespace", r"[ \t\r\n]+").skip(),
        TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Ascii)),
        TokenSpec::preset(STRING, "string", Preset::String(preset)),
    ]);
    Lexer::builder().tokens(specs).build().expect("valid")
}

pub fn source(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test", text).expect("test text is small")
}

/// Lexes and checks what must hold for any input: the tokens cover the text.
pub fn lex(lexer: &Lexer, text: &str) -> Lexed {
    let lexed = lexer.lex(&source(text));
    let mut end = 0;
    for token in &lexed.tokens {
        assert_eq!(
            u32::from(token.range.start()),
            end,
            "gap or overlap in {text:?}"
        );
        assert!(!token.range.is_empty(), "empty token in {text:?}");
        end = u32::from(token.range.end());
        assert!(
            text.is_char_boundary(end as usize),
            "split character in {text:?}"
        );
    }
    assert_eq!(end as usize, text.len(), "text not covered: {text:?}");
    lexed
}

/// The tokens that are not trivia, as `KIND text`.
pub fn significant(lexer: &Lexer, text: &str) -> Vec<String> {
    lex(lexer, text)
        .tokens
        .iter()
        .filter(|token| !token.trivia)
        .map(|token| format!("{} {}", kind_name(token.kind), &text[token.range]))
        .collect()
}

/// Diagnostics as `CODE start..end message`.
pub fn errors(diagnostics: &[langmeld_syntax::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| {
            let (start, end) = (u32::from(d.range.start()), u32::from(d.range.end()));
            format!("{} {start}..{end} {}", d.code, d.message)
        })
        .collect()
}

pub fn lex_errors(lexer: &Lexer, text: &str) -> Vec<String> {
    errors(&lex(lexer, text).diagnostics)
}
