use langmeld_core::{
    Diagnostic as LangmeldDiagnostic, PositionEncoding, Severity, SourceFile, SourceLocation, Span,
};
use lsp_types::{
    CompletionItem, CompletionTextEdit, Diagnostic, DiagnosticSeverity, NumberOrString, Position,
    Range, TextEdit, Uri,
};

use crate::backend::BackendDocument;

pub(crate) fn byte_offset(
    source: &SourceFile,
    position: Position,
    encoding: PositionEncoding,
) -> Option<usize> {
    source.line_index().offset(
        position.line as usize,
        position.character as usize,
        encoding,
    )
}

pub(crate) fn to_lsp_position(
    source: &SourceFile,
    offset: usize,
    encoding: PositionEncoding,
) -> Option<Position> {
    let position = source.line_index().position(offset, encoding)?;
    Some(Position::new(
        position.line.try_into().unwrap_or(u32::MAX),
        position.character.try_into().unwrap_or(u32::MAX),
    ))
}

pub(crate) fn to_lsp_range(
    source: &SourceFile,
    span: Span,
    encoding: PositionEncoding,
) -> Option<Range> {
    let start = to_lsp_position(source, span.start, encoding)?;
    let end = to_lsp_position(source, span.end, encoding)?;
    Some(Range::new(start, end))
}

pub(crate) fn diagnostics_for<D>(
    source: &SourceFile,
    document: &BackendDocument<D>,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    if document.source != source.identity() {
        return Vec::new();
    }
    document
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic_for(source, diagnostic, encoding))
        .collect()
}

fn diagnostic_for(
    source: &SourceFile,
    diagnostic: &LangmeldDiagnostic,
    encoding: PositionEncoding,
) -> Option<Diagnostic> {
    let location = diagnostic.location.as_ref()?;
    if location.source_name != source.name || location.identity != source.identity() {
        return None;
    }
    let range = to_lsp_range(source, location.span, encoding)?;
    Some(Diagnostic {
        range,
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        code: Some(NumberOrString::String(diagnostic.code.clone())),
        source: Some("langmeld".into()),
        message: diagnostic.message.clone(),
        ..Diagnostic::default()
    })
}

pub(crate) fn completion_items(
    source: &SourceFile,
    candidates: impl IntoIterator<Item = langmeld_language::CompletionCandidate>,
    encoding: PositionEncoding,
) -> Vec<CompletionItem> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let range = to_lsp_range(source, candidate.replace, encoding)?;
            if range.start.line != range.end.line {
                return None;
            }
            let label = candidate.label.unwrap_or_else(|| candidate.text.clone());
            let filter_text = candidate
                .filter_text
                .unwrap_or_else(|| candidate.text.clone());
            Some(CompletionItem {
                label,
                detail: candidate.detail,
                filter_text: Some(filter_text),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: candidate.text,
                })),
                ..CompletionItem::default()
            })
        })
        .collect()
}

pub(crate) fn lsp_location(
    location: &SourceLocation,
    uri: Uri,
    source: &SourceFile,
    encoding: PositionEncoding,
) -> Option<lsp_types::Location> {
    if location.source_name != source.name || location.identity != source.identity() {
        return None;
    }
    let range = to_lsp_range(source, location.span, encoding)?;
    Some(lsp_types::Location { uri, range })
}
