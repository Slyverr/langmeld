use std::path::Path;

use langmeld_core::{Diagnostic, SourceFile, Span, format_diagnostics};
use langmeld_language::{LanguageSpec, LexedToken, LexerExtension, LexicalMatch, TokenKind};
use langmeld_project::load_definition_project;

/// Adds raw-string tokens to Langmeld's built-in lexer.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawStringExtension;

impl LexerExtension for RawStringExtension {
    fn lex_token(&self, source: &SourceFile, offset: usize) -> Option<LexicalMatch> {
        let text = source.text();
        if !text.get(offset..)?.starts_with("r#\"") {
            return None;
        }

        let content_start = offset + 3;
        let (end, kind) = match text[content_start..].find("\"#") {
            Some(relative_end) => (content_start + relative_end + 2, TokenKind::String),
            None => (text.len(), TokenKind::Invalid),
        };
        let span = Span::new(offset, end);
        let token = LexedToken {
            text: text[offset..end].to_owned(),
            kind,
            span,
        };
        let mut matched = LexicalMatch::new(token);

        if kind == TokenKind::Invalid {
            matched.diagnostics.push(Diagnostic::error(
                "RAW-LEX-001",
                "unterminated raw string literal",
                source,
                span,
            ));
        }

        Some(matched)
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
