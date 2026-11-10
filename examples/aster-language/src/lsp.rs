use langmeld_core::{Diagnostic, SourceFile, SourceLocation};
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, LanguageNavigator, ParsedDocument};
use langmeld_lsp::{BackendCapabilities, BackendDocument, LanguageBackend};

use crate::{Analysis, AsterAnalyzer, AsterNavigator, Program, parse_program};

/// LSP backend that exposes Aster's own parser, semantic diagnostics, and
/// resolved-symbol navigation.
#[derive(Clone, Copy, Debug, Default)]
pub struct AsterBackend;

/// Retained Aster parse and analysis results for one editor document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsterDocument {
    /// Host-owned syntax tree and parser diagnostics.
    pub parsed: ParsedDocument<Program>,
    /// Host-owned symbols, resolved references, and semantic diagnostics.
    pub analysis: AnalysisDocument<Analysis>,
}

impl LanguageBackend for AsterBackend {
    type Document = AsterDocument;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = parse_program(source);
        let analysis = AsterAnalyzer.analyze(source, &parsed);
        let diagnostics = parsed
            .diagnostics
            .iter()
            .chain(&analysis.diagnostics)
            .cloned()
            .collect::<Vec<Diagnostic>>();
        BackendDocument::new(source, AsterDocument { parsed, analysis }, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            definition: true,
            references: true,
            ..BackendCapabilities::default()
        }
    }

    fn definition(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        AsterNavigator.definition(
            source,
            &document.state.parsed,
            Some(&document.state.analysis),
            offset,
        )
    }

    fn references(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        AsterNavigator.references(
            source,
            &document.state.parsed,
            Some(&document.state.analysis),
            offset,
            include_declaration,
        )
    }
}
