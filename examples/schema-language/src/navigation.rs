use langmeld_core::{SourceFile, SourceLocation, Span};
use langmeld_language::{AnalysisDocument, LanguageNavigator, ParsedDocument};

use crate::{Analysis, Schema, SchemaParser};

/// Provides navigation through the schema's global record type namespace.
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaNavigator;

impl LanguageNavigator<SchemaParser> for SchemaNavigator {
    type Analysis = Analysis;

    fn definition(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Schema>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        let Some(symbol) = selected_symbol(&analysis.result, offset) else {
            return Vec::new();
        };
        vec![location(
            source,
            analysis.result.symbols[symbol].declaration,
        )]
    }

    fn references(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Schema>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        let Some(symbol) = selected_symbol(&analysis.result, offset) else {
            return Vec::new();
        };
        let mut locations = analysis
            .result
            .references
            .iter()
            .filter(|reference| reference.target == Some(symbol))
            .map(|reference| location(source, reference.span))
            .collect::<Vec<_>>();
        if include_declaration {
            locations.push(location(
                source,
                analysis.result.symbols[symbol].declaration,
            ));
        }
        locations
    }
}

fn selected_symbol(analysis: &Analysis, offset: usize) -> Option<usize> {
    analysis
        .references
        .iter()
        .find(|reference| contains(reference.span, offset))
        .and_then(|reference| reference.target)
        .or_else(|| {
            analysis
                .symbols
                .iter()
                .position(|symbol| contains(symbol.declaration, offset))
        })
}

fn location(source: &SourceFile, span: Span) -> SourceLocation {
    SourceLocation {
        source_name: source.name.clone(),
        identity: source.identity(),
        span,
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
