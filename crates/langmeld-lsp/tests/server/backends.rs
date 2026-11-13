use super::*;

struct DiagnosticParser;

impl LanguageParser for DiagnosticParser {
    type Tree = bool;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let diagnostics = source
            .text()
            .contains("syntax")
            .then(|| Diagnostic::error("HOST-PARSE", "parser diagnostic", source, Span::new(0, 6)))
            .into_iter()
            .collect();
        ParsedDocument::new(source, true, diagnostics)
    }
}

#[path = "backends/declarative.rs"]
mod declarative;
#[path = "backends/diagnostics.rs"]
mod diagnostics;
#[path = "backends/host.rs"]
mod host;
#[path = "backends/source_identity.rs"]
mod source_identity;
#[path = "backends/workspace.rs"]
mod workspace;
