use langmeld_core::SourceFile;
use langmeld_language::{CompletionCandidate, LanguageSpec, ParseResult, complete_at, parse};

use crate::backend::{BackendCapabilities, BackendDocument, LanguageBackend, LanguageServerInfo};

/// Backend for a language described by a validated declarative grammar.
#[derive(Clone, Debug)]
pub struct DeclarativeBackend {
    language: LanguageSpec,
    server_info: LanguageServerInfo,
    completion_trigger_characters: Vec<String>,
}

impl DeclarativeBackend {
    /// Creates a backend for `language`.
    #[must_use]
    pub fn new(language: LanguageSpec) -> Self {
        Self {
            language,
            server_info: LanguageServerInfo::default(),
            completion_trigger_characters: Vec::new(),
        }
    }

    /// Sets the name and version shown by the editor for this server.
    #[must_use]
    pub fn with_server_info(mut self, server_info: LanguageServerInfo) -> Self {
        self.server_info = server_info;
        self
    }

    /// Configures characters that should trigger automatic completion.
    ///
    /// The server advertises these strings through LSP
    /// `completionProvider.triggerCharacters`.
    #[must_use]
    pub fn with_completion_trigger_characters<I, S>(mut self, characters: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.completion_trigger_characters = characters.into_iter().map(Into::into).collect();
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

    fn completion_trigger_characters(&self) -> Vec<String> {
        self.completion_trigger_characters.clone()
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
