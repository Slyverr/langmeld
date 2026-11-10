use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{
    CapturedValue, ConstructSpec, LanguageSpec, LexOutput, LexedToken, Lexer, PatternPart,
    TokenKind, parse_with_lexer,
};

const PROGRAM: &str = "say r#\"comments // inside the string stay text\"#; // this is a comment\n";

struct RawStringLexer;

impl Lexer for RawStringLexer {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        let mut tokens = Vec::new();
        let mut diagnostics = Vec::new();
        let bytes = source.text.as_bytes();
        let mut cursor = 0;

        while cursor < bytes.len() {
            if bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
                continue;
            }
            if bytes[cursor..].starts_with(b"//") {
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                continue;
            }

            let start = cursor;
            let kind = if bytes[cursor..].starts_with(b"r#\"") {
                cursor += 3;
                if let Some(relative_end) = bytes[cursor..]
                    .windows(2)
                    .position(|window| window == b"\"#")
                {
                    cursor += relative_end + 2;
                    TokenKind::String
                } else {
                    cursor = bytes.len();
                    diagnostics.push(Diagnostic::error(
                        "RAW-LEX-001",
                        "unterminated raw string literal",
                        source,
                        Span::new(start, cursor),
                    ));
                    TokenKind::Invalid
                }
            } else if bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_' {
                cursor += 1;
                while cursor < bytes.len()
                    && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
                {
                    cursor += 1;
                }
                TokenKind::Identifier
            } else {
                let character = source.text[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is before the end of the source");
                cursor += character.len_utf8();
                TokenKind::Punctuation
            };

            tokens.push(LexedToken {
                text: source.text[start..cursor].to_owned(),
                kind,
                span: Span::new(start, cursor),
            });
        }

        LexOutput {
            tokens,
            diagnostics,
        }
    }
}

fn main() {
    let language = LanguageSpec::new(
        "RawSay",
        [ConstructSpec::new(
            "say",
            [
                PatternPart::literal("say"),
                PatternPart::string("text"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .expect("the example grammar is valid");
    let source = SourceFile::new("main.rawsay", PROGRAM);
    let parsed = parse_with_lexer(&source, &language, &RawStringLexer);

    if !parsed.diagnostics.is_empty() {
        for diagnostic in parsed.diagnostics {
            eprintln!("{}: {}", diagnostic.code, diagnostic.message);
        }
        std::process::exit(1);
    }

    let Some(field) = parsed
        .constructs
        .first()
        .and_then(|construct| construct.fields.first())
    else {
        eprintln!("the parser did not produce the expected `say` construct");
        std::process::exit(1);
    };
    match &field.value {
        CapturedValue::String(value) => println!("Captured raw string: {value}"),
        _ => {
            eprintln!("the `text` field was not a string token");
            std::process::exit(1);
        }
    }
}
