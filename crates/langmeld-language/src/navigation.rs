use langmeld_core::{SourceFile, SourceLocation};

use crate::{AnalysisDocument, LanguageNavigator, LanguageParser, ParsedDocument};

/// Finds the declaration associated with a source offset.
#[must_use]
pub fn definition_at<P, N>(
    source: &SourceFile,
    parsed: &ParsedDocument<P::Tree>,
    analysis: Option<&AnalysisDocument<N::Analysis>>,
    offset: usize,
    navigator: &N,
) -> Vec<SourceLocation>
where
    P: LanguageParser,
    N: LanguageNavigator<P> + ?Sized,
{
    query_locations(source, parsed, analysis, offset, |analysis| {
        navigator.definition(source, parsed, analysis, offset)
    })
}

/// Finds references to the symbol at a source offset.
#[must_use]
pub fn references_at<P, N>(
    source: &SourceFile,
    parsed: &ParsedDocument<P::Tree>,
    analysis: Option<&AnalysisDocument<N::Analysis>>,
    offset: usize,
    include_declaration: bool,
    navigator: &N,
) -> Vec<SourceLocation>
where
    P: LanguageParser,
    N: LanguageNavigator<P> + ?Sized,
{
    query_locations(source, parsed, analysis, offset, |analysis| {
        navigator.references(source, parsed, analysis, offset, include_declaration)
    })
}

fn query_locations<P, A>(
    source: &SourceFile,
    parsed: &ParsedDocument<P>,
    analysis: Option<&AnalysisDocument<A>>,
    offset: usize,
    query: impl FnOnce(Option<&AnalysisDocument<A>>) -> Vec<SourceLocation>,
) -> Vec<SourceLocation> {
    if !source.is_valid_offset(offset)
        || parsed.source_identity() != source.identity()
        || analysis.is_some_and(|analysis| analysis.source_identity() != source.identity())
    {
        return Vec::new();
    }

    query(analysis)
        .into_iter()
        .filter(|location| {
            let span = location.span();
            if span.start > span.end {
                return false;
            }
            if location.source_name() != source.name() {
                return true;
            }
            location.is_valid_for(source)
        })
        .collect()
}
