use langmeld_core::SourceFile;
use langmeld_language::{CompletionCandidate, LanguageSpec, ParseResult, complete_at, parse};

use crate::backend::{BackendCapabilities, BackendDocument, LanguageBackend};

/// Backend for a language described by a validated declarative grammar.
#[derive(Clone, Debug)]
pub struct DeclarativeBackend {
    language: LanguageSpec,
}

impl DeclarativeBackend {
    /// Creates a backend for `language`.
    #[must_use]
    pub fn new(language: LanguageSpec) -> Self {
        Self { language }
    }
}

impl LanguageBackend for DeclarativeBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = parse(source, &self.language);
        let diagnostics = parsed.diagnostics().to_vec();

        BackendDocument::new(source, parsed, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        complete_at(source, &self.language, offset)
    }
}
