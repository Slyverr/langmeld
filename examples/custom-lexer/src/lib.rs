use std::path::Path;

use langmeld_core::{Diagnostic, SourceFile, Span, format_diagnostics};
use langmeld_language::{LanguageSpec, LexOutput, LexedToken, Lexer, TokenKind};
use langmeld_project::load_definition_project;

/// Lexer for the example's raw string syntax and line comments.
pub struct RawStringLexer;

impl Lexer for RawStringLexer {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        let mut tokens = Vec::new();
        let mut diagnostics = Vec::new();
        let bytes = source.text().as_bytes();
        let mut cursor = 0;

        while cursor < bytes.len() {
            if bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
                continue;
            }
            if bytes[cursor..].starts_with(b"//") {
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
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
                let character = source.text()[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is before the end of the source");
                cursor += character.len_utf8();
                TokenKind::Punctuation
            };

            tokens.push(LexedToken {
                text: source.text()[start..cursor].to_owned(),
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

/// Loads and validates the language declared in this example project.
///
/// # Errors
///
/// Returns an error when the project cannot be loaded, its definition is
/// invalid, or loading does not produce a language specification.
pub fn load_language(project_root: impl AsRef<Path>) -> Result<LanguageSpec, String> {
    let project = load_definition_project(project_root).map_err(|error| error.to_string())?;
    let langmeld_project::LoadedDefinitionProjectParts {
        definition,
        sources,
        ..
    } = project.into_parts();

    if !definition.is_valid() {
        return Err(format_diagnostics(&sources, &definition.diagnostics));
    }

    definition
        .language
        .ok_or_else(|| "validated definition did not produce a language".to_owned())
}
