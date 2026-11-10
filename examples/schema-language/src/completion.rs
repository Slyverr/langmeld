use langmeld_core::SourceFile;
use langmeld_language::{AnalysisDocument, CompletionCandidate, LanguageCompleter, ParsedDocument};

use crate::{Analysis, Schema, SchemaParser};

/// Suggests declared record types while editing a field's type name.
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaCompleter;

impl LanguageCompleter<SchemaParser> for SchemaCompleter {
    type Analysis = Analysis;

    fn complete(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Schema>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        if parsed.source != source.identity() || analysis.source != source.identity() {
            return Vec::new();
        }

        let Some(reference) = analysis.result.references.iter().find(|reference| {
            is_current_source(&reference.location, source)
                && reference.span.start <= offset
                && offset <= reference.span.end
        }) else {
            return Vec::new();
        };
        let Some(prefix) = source.text.get(reference.span.start..offset) else {
            return Vec::new();
        };

        analysis
            .result
            .symbols
            .iter()
            .filter(|symbol| symbol.name.starts_with(prefix))
            .map(|symbol| {
                CompletionCandidate::new(symbol.name.clone(), reference.span)
                    .with_detail("Record type")
            })
            .collect()
    }
}

fn is_current_source(location: &langmeld_core::SourceLocation, source: &SourceFile) -> bool {
    location.identity == source.identity() && location.source_name == source.name
}
