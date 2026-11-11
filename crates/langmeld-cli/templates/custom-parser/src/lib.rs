use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

pub struct SayParser;

impl LanguageParser for SayParser {
    type Tree = Vec<String>;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let mut statements = Vec::new();
        let mut diagnostics = Vec::new();
        let mut line_start = 0;

        for line_with_ending in source.text.split_inclusive('\n') {
            let line = line_with_ending
                .strip_suffix('\n')
                .unwrap_or(line_with_ending);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let leading = line.len() - line.trim_start().len();
            let content = line.trim();
            if !content.is_empty() {
                if let Some(message) = content.strip_prefix("say ") {
                    if message.trim().is_empty() {
                        diagnostics.push(Diagnostic::error(
                            "EXAMPLE-001",
                            "expected text after `say`",
                            source,
                            Span::new(line_start + leading, line_start + line.len()),
                        ));
                    } else {
                        statements.push(message.trim().to_owned());
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        "EXAMPLE-001",
                        "expected `say <text>`",
                        source,
                        Span::new(line_start + leading, line_start + line.len()),
                    ));
                }
            }
            line_start += line_with_ending.len();
        }

        ParsedDocument::new(source, statements, diagnostics)
    }
}
