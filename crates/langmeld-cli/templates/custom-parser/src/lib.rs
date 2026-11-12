use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub statements: Vec<SayStatement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SayStatement {
    pub message: String,
    pub span: Span,
}

pub struct SayParser;

impl LanguageParser for SayParser {
    type Tree = Program;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let mut program = Program::default();
        let mut diagnostics = Vec::new();
        let mut line_start = 0;

        for line_with_ending in source.text().split_inclusive('\n') {
            let line = line_with_ending
                .strip_suffix('\n')
                .unwrap_or(line_with_ending);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let leading = line.len() - line.trim_start().len();
            let content = line.trim();
            let content_start = line_start + leading;
            let content_span = Span::new(content_start, content_start + content.len());
            if !content.is_empty() {
                if let Some(message) = content.strip_prefix("say ") {
                    if message.trim().is_empty() {
                        diagnostics.push(Diagnostic::error(
                            "{{DIAGNOSTIC_PREFIX}}-001",
                            "expected text after `say`",
                            source,
                            content_span,
                        ));
                    } else {
                        program.statements.push(SayStatement {
                            message: message.trim().to_owned(),
                            span: content_span,
                        });
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        "{{DIAGNOSTIC_PREFIX}}-001",
                        "expected `say <text>`",
                        source,
                        content_span,
                    ));
                }
            }
            line_start += line_with_ending.len();
        }

        ParsedDocument::new(source, program, diagnostics)
    }
}
