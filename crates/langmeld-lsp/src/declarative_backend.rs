use langmeld_core::SourceFile;
use std::collections::BTreeSet;

use langmeld_language::{
    CompletionCandidate, LanguageSpec, ParseResult, PatternPart, complete_at, parse,
};

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
        let completion_trigger_characters = infer_completion_trigger_characters(&language);

        Self {
            language,
            server_info: LanguageServerInfo::default(),
            completion_trigger_characters,
        }
    }

    /// Sets the name and version shown by the editor for this server.
    #[must_use]
    pub fn with_server_info(mut self, server_info: LanguageServerInfo) -> Self {
        self.server_info = server_info;
        self
    }

    /// Overrides characters inferred from the grammar that trigger completion.
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

fn infer_completion_trigger_characters(language: &LanguageSpec) -> Vec<String> {
    let mut characters = BTreeSet::new();

    for construct in language.constructs() {
        for pattern in construct.variants() {
            for part in pattern {
                collect_trigger_characters(part, &mut characters);
            }
        }

        if let Some(block) = construct.block_spec() {
            collect_trigger_characters_from_text(block.open(), &mut characters);
            collect_trigger_characters_from_text(block.close(), &mut characters);
        }
    }

    characters
        .into_iter()
        .map(|character| character.to_string())
        .collect()
}

fn collect_trigger_characters(part: &PatternPart, characters: &mut BTreeSet<char>) {
    match part {
        PatternPart::Literal(value) => collect_trigger_characters_from_text(value, characters),
        PatternPart::Choice { values, .. } => {
            for value in values {
                collect_trigger_characters_from_text(value, characters);
            }
        }
        PatternPart::Expression { terminator, .. } => {
            collect_trigger_characters_from_text(terminator, characters);
        }
        PatternPart::Optional { parts, .. } | PatternPart::Repeat { parts, .. } => {
            for part in parts {
                collect_trigger_characters(part, characters);
            }
        }
        PatternPart::Field { .. } => {}
    }
}

fn collect_trigger_characters_from_text(value: &str, characters: &mut BTreeSet<char>) {
    characters.extend(value.chars().filter(|character| {
        !character.is_alphanumeric() && !character.is_whitespace() && *character != '_'
    }));
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
