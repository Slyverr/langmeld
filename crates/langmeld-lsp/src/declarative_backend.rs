use langmeld_core::SourceFile;
use langmeld_language::{CompletionCandidate, LanguageSpec, ParseResult, complete_at, parse};

use crate::backend::{BackendCapabilities, BackendDocument, LanguageBackend, LanguageServerInfo};

/// Backend for a language described by a validated declarative grammar.
#[derive(Clone, Debug)]
pub struct DeclarativeBackend {
    language: LanguageSpec,
    server_info: LanguageServerInfo,
}

impl DeclarativeBackend {
    /// Creates a backend for `language`.
    #[must_use]
    pub fn new(language: LanguageSpec) -> Self {
        Self {
            language,
            server_info: LanguageServerInfo::default(),
        }
    }

    /// Sets the name and version shown by the editor for this server.
    #[must_use]
    pub fn with_server_info(mut self, server_info: LanguageServerInfo) -> Self {
        self.server_info = server_info;
        self
    }
}

impl LanguageBackend for DeclarativeBackend {
    type Document = ParseResult;

    fn server_info(&self) -> LanguageServerInfo {
        self.server_info.clone()
    }

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
