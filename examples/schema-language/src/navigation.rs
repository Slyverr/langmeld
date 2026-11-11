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
        let Some(symbol) = selected_symbol(analysis.result(), source, offset) else {
            return Vec::new();
        };
        vec![analysis.result().symbols[symbol].location.clone()]
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
        let Some(symbol) = selected_symbol(analysis.result(), source, offset) else {
            return Vec::new();
        };
        let mut locations = analysis
            .result()
            .references
            .iter()
            .filter(|reference| reference.target == Some(symbol))
            .map(|reference| reference.location.clone())
            .collect::<Vec<_>>();
        if include_declaration {
            locations.push(analysis.result().symbols[symbol].location.clone());
        }
        locations
    }
}

fn selected_symbol(analysis: &Analysis, source: &SourceFile, offset: usize) -> Option<usize> {
    analysis
        .references
        .iter()
        .find(|reference| {
            is_current_source(&reference.location, source) && contains(reference.span, offset)
        })
        .and_then(|reference| reference.target)
        .or_else(|| {
            analysis.symbols.iter().position(|symbol| {
                is_current_source(&symbol.location, source) && contains(symbol.declaration, offset)
            })
        })
}

fn is_current_source(location: &SourceLocation, source: &SourceFile) -> bool {
    location.source_identity() == source.identity() && location.source_name() == source.name()
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
