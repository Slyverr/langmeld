use langmeld_core::{SourceFile, SourceLocation, Span};
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, LanguageNavigator, ParsedDocument};

use crate::{Analysis, AsterParser, Program, parse_program};

/// Provides navigation through Aster's lexically resolved symbols.
#[derive(Clone, Copy, Debug, Default)]
pub struct AsterNavigator;

impl LanguageNavigator<AsterParser> for AsterNavigator {
    type Analysis = Analysis;

    fn definition(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Program>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        let Some(symbol) = selected_symbol(analysis.result(), offset) else {
            return Vec::new();
        };
        vec![location(
            source,
            analysis.result().symbols[symbol].declaration,
        )]
    }

    fn references(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Program>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        let Some(symbol) = selected_symbol(analysis.result(), offset) else {
            return Vec::new();
        };
        let mut locations = analysis
            .result()
            .references
            .iter()
            .filter(|reference| reference.target == Some(symbol))
            .map(|reference| location(source, reference.span))
            .collect::<Vec<_>>();
        if include_declaration {
            locations.push(location(
                source,
                analysis.result().symbols[symbol].declaration,
            ));
        }
        locations
    }
}

impl AsterNavigator {
    /// Returns a safe rename for an Aster symbol, including its declaration
    /// and every resolved use in this source file.
    pub fn rename(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        analysis: Option<&AnalysisDocument<Analysis>>,
        offset: usize,
        new_name: &str,
    ) -> Vec<SourceLocation> {
        let Some(analysis) = analysis else {
            return Vec::new();
        };
        if !is_identifier(new_name) {
            return Vec::new();
        }
        if parsed.source_identity() != source.identity()
            || analysis.source_identity() != source.identity()
        {
            return Vec::new();
        }
        let result = analysis.result();
        let Some(symbol_id) = selected_symbol(result, offset) else {
            return Vec::new();
        };
        let symbol = &result.symbols[symbol_id];
        if symbol.name == new_name
            || result.symbols.iter().enumerate().any(|(other_id, other)| {
                other_id != symbol_id && other.scope == symbol.scope && other.name == new_name
            })
        {
            return Vec::new();
        }

        let mut spans = result
            .references
            .iter()
            .filter(|reference| reference.target == Some(symbol_id))
            .map(|reference| reference.span)
            .chain(std::iter::once(symbol.declaration))
            .collect::<Vec<_>>();
        spans.sort_by_key(|span| (span.start, span.end));
        if spans.windows(2).any(|pair| pair[0].end > pair[1].start) {
            return Vec::new();
        }
        if !rename_preserves_resolution(source, result, symbol_id, &spans, new_name) {
            return Vec::new();
        }

        spans
            .into_iter()
            .map(|span| location(source, span))
            .collect()
    }
}

fn rename_preserves_resolution(
    source: &SourceFile,
    original: &Analysis,
    symbol_id: usize,
    spans: &[Span],
    new_name: &str,
) -> bool {
    let mut renamed_text = source.text().to_owned();
    for span in spans.iter().rev() {
        if span.end > renamed_text.len()
            || !renamed_text.is_char_boundary(span.start)
            || !renamed_text.is_char_boundary(span.end)
        {
            return false;
        }
        renamed_text.replace_range(span.start..span.end, new_name);
    }

    let renamed_source = SourceFile::new(source.name().to_owned(), renamed_text);
    let renamed_parse = parse_program(&renamed_source);
    let renamed_analysis = crate::AsterAnalyzer.analyze(&renamed_source, &renamed_parse);

    let Some(new_declaration_start) = mapped_start(
        original.symbols[symbol_id].declaration.start,
        spans,
        new_name,
    ) else {
        return false;
    };
    let Some(renamed_symbol_id) = renamed_analysis.result().symbols.iter().position(|symbol| {
        symbol.declaration.start == new_declaration_start
            && symbol.name == new_name
            && symbol.kind == original.symbols[symbol_id].kind
    }) else {
        return false;
    };
    let renamed_symbol = &renamed_analysis.result().symbols[renamed_symbol_id];
    if renamed_analysis
        .result()
        .symbols
        .iter()
        .enumerate()
        .any(|(other_id, other)| {
            other_id != renamed_symbol_id
                && other.scope == renamed_symbol.scope
                && other.name == new_name
        })
    {
        return false;
    }
    original
        .references
        .iter()
        .filter(|reference| reference.target == Some(symbol_id))
        .all(|reference| {
            let Some(new_start) = mapped_start(reference.span.start, spans, new_name) else {
                return false;
            };
            renamed_analysis
                .result()
                .references
                .iter()
                .any(|renamed_reference| {
                    renamed_reference.span.start == new_start
                        && renamed_reference.name == new_name
                        && renamed_reference.target == Some(renamed_symbol_id)
                })
        })
}

fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
        && !matches!(
            name,
            "fn" | "let" | "return" | "i32" | "bool" | "string" | "true" | "false"
        )
}

fn mapped_start(offset: usize, spans: &[Span], new_name: &str) -> Option<usize> {
    let mut mapped = offset;
    for span in spans.iter().take_while(|span| span.start < offset) {
        mapped = mapped
            .checked_sub(span.len())?
            .checked_add(new_name.len())?;
    }
    Some(mapped)
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
    SourceLocation::new(source, span)
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
