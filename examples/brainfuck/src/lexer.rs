use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LexOutput, LexedToken, Lexer, TokenKind};

const COMMANDS: &[u8] = b"><+-.,[]";

pub struct BrainfuckLexer;

impl Lexer for BrainfuckLexer {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        self.lex_with_token_limit(source, usize::MAX)
    }

    fn lex_with_token_limit(&self, source: &SourceFile, max_tokens: usize) -> LexOutput {
        let mut tokens = Vec::with_capacity(source.text().len().min(max_tokens).min(4096));
        let mut diagnostics = Vec::new();

        for (offset, byte) in source.text().as_bytes().iter().enumerate() {
            if !COMMANDS.contains(byte) {
                continue;
            }
            if tokens.len() == max_tokens {
                diagnostics.push(Diagnostic::error(
                    "LM-PARSE-008",
                    format!("token limit of {max_tokens} exceeded"),
                    source,
                    Span::new(offset, offset + 1),
                ));
                break;
            }

            tokens.push(LexedToken {
                text: char::from(*byte).to_string(),
                kind: TokenKind::Punctuation,
                span: Span::new(offset, offset + 1),
            });
        }

        LexOutput {
            tokens,
            diagnostics,
        }
    }
}
