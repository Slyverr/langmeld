use langmeld_core::{Diagnostic, SourceFile, SourceLocation};
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, LanguageNavigator, ParsedDocument};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, DocumentSymbolInfo, DocumentSymbolKind, LanguageBackend,
    SemanticToken, SemanticTokenLegend,
};

use crate::{Analysis, AsterAnalyzer, AsterNavigator, Program, SymbolKind, parse_program};

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
            rename: true,
            document_symbols: true,
            semantic_tokens: true,
            ..BackendCapabilities::default()
        }
    }

    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        SemanticTokenLegend::new(["function", "parameter", "variable"], ["declaration"])
    }

    fn semantic_tokens(
        &self,
        _source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<SemanticToken> {
        let analysis = &document.state.analysis.result;
        let mut tokens = analysis
            .symbols
            .iter()
            .map(|symbol| {
                SemanticToken::new(symbol.declaration, symbol_kind_name(symbol.kind))
                    .with_modifier("declaration")
            })
            .collect::<Vec<_>>();
        tokens.extend(analysis.references.iter().filter_map(|reference| {
            let target = reference.target?;
            let symbol = analysis.symbols.get(target)?;
            Some(SemanticToken::new(
                reference.span,
                symbol_kind_name(symbol.kind),
            ))
        }));
        tokens
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

    fn rename(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        new_name: &str,
    ) -> Option<Vec<SourceLocation>> {
        let locations = AsterNavigator.rename(
            source,
            &document.state.parsed,
            Some(&document.state.analysis),
            offset,
            new_name,
        );
        (!locations.is_empty()).then_some(locations)
    }

    fn document_symbols(
        &self,
        _source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<DocumentSymbolInfo> {
        let analysis = &document.state.analysis.result;
        document
            .state
            .parsed
            .tree
            .functions
            .iter()
            .map(|function| {
                let children = analysis
                    .symbols
                    .iter()
                    .filter(|symbol| {
                        symbol.kind != SymbolKind::Function
                            && function.span.start <= symbol.declaration.start
                            && symbol.declaration.end <= function.span.end
                    })
                    .map(|symbol| {
                        DocumentSymbolInfo::new(
                            symbol.name.clone(),
                            DocumentSymbolKind::Variable,
                            symbol.declaration,
                            symbol.declaration,
                        )
                    })
                    .collect::<Vec<_>>();
                DocumentSymbolInfo::new(
                    function.name.clone(),
                    DocumentSymbolKind::Function,
                    function.span,
                    function.name_span,
                )
                .with_children(children)
            })
            .collect()
    }
}

fn symbol_kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Parameter => "parameter",
        SymbolKind::Variable => "variable",
    }
}
