use langmeld_core::{
    Diagnostic as LangmeldDiagnostic, PositionEncoding, Severity, SourceFile, SourceLocation, Span,
};
use langmeld_language::{CompletionInsertTextFormat, CompletionKind};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Diagnostic, DiagnosticSeverity,
    InsertTextFormat, NumberOrString, Position, Range, TextEdit, Uri,
};

use crate::backend::BackendDocument;

pub(crate) struct ConvertedDiagnostics {
    pub diagnostics: Vec<Diagnostic>,
    pub invalid_locations: bool,
}

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
) -> ConvertedDiagnostics {
    if !document.matches_source(source) {
        return ConvertedDiagnostics {
            diagnostics: Vec::new(),
            invalid_locations: false,
        };
    }

    let mut converted = ConvertedDiagnostics {
        diagnostics: Vec::new(),
        invalid_locations: false,
    };
    for finding in document.diagnostics() {
        if finding.location.is_none() {
            continue;
        }

        if let Some(diagnostic) = diagnostic_for(source, finding, encoding) {
            converted.diagnostics.push(diagnostic);
        } else {
            converted.invalid_locations = true;
        }
    }

    converted
}

pub(crate) fn diagnostic_for(
    source: &SourceFile,
    diagnostic: &LangmeldDiagnostic,
    encoding: PositionEncoding,
) -> Option<Diagnostic> {
    let location = diagnostic.location.as_ref()?;
    if location.source_name() != source.name() || location.source_identity() != source.identity() {
        return None;
    }
    let range = to_lsp_range(source, location.span(), encoding)?;
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
    supports_snippets: bool,
) -> Vec<CompletionItem> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            if candidate.insert_text_format == CompletionInsertTextFormat::Snippet
                && !supports_snippets
            {
                return None;
            }

            let range = to_lsp_range(source, candidate.replace, encoding)?;
            if range.start.line != range.end.line {
                return None;
            }
            let insert_text_format = match candidate.insert_text_format {
                CompletionInsertTextFormat::PlainText => InsertTextFormat::PLAIN_TEXT,
                CompletionInsertTextFormat::Snippet => InsertTextFormat::SNIPPET,
            };
            let label = candidate.label.unwrap_or_else(|| candidate.text.clone());
            let filter_text = candidate
                .filter_text
                .unwrap_or_else(|| candidate.text.clone());
            Some(CompletionItem {
                label,
                kind: candidate.kind.map(to_lsp_completion_kind),
                detail: candidate.detail,
                filter_text: Some(filter_text),
                insert_text_format: Some(insert_text_format),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: candidate.text,
                })),
                ..CompletionItem::default()
            })
        })
        .collect()
}

fn to_lsp_completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Text => CompletionItemKind::TEXT,
        CompletionKind::Method => CompletionItemKind::METHOD,
        CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Constructor => CompletionItemKind::CONSTRUCTOR,
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::Variable => CompletionItemKind::VARIABLE,
        CompletionKind::Class => CompletionItemKind::CLASS,
        CompletionKind::Interface => CompletionItemKind::INTERFACE,
        CompletionKind::Module => CompletionItemKind::MODULE,
        CompletionKind::Property => CompletionItemKind::PROPERTY,
        CompletionKind::Unit => CompletionItemKind::UNIT,
        CompletionKind::Value => CompletionItemKind::VALUE,
        CompletionKind::Enum => CompletionItemKind::ENUM,
        CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        CompletionKind::Snippet => CompletionItemKind::SNIPPET,
        CompletionKind::Color => CompletionItemKind::COLOR,
        CompletionKind::File => CompletionItemKind::FILE,
        CompletionKind::Reference => CompletionItemKind::REFERENCE,
        CompletionKind::Folder => CompletionItemKind::FOLDER,
        CompletionKind::EnumMember => CompletionItemKind::ENUM_MEMBER,
        CompletionKind::Constant => CompletionItemKind::CONSTANT,
        CompletionKind::Struct => CompletionItemKind::STRUCT,
        CompletionKind::Event => CompletionItemKind::EVENT,
        CompletionKind::Operator => CompletionItemKind::OPERATOR,
        CompletionKind::TypeParameter => CompletionItemKind::TYPE_PARAMETER,
    }
}

pub(crate) fn lsp_location(
    location: &SourceLocation,
    uri: Uri,
    source: &SourceFile,
    encoding: PositionEncoding,
) -> Option<lsp_types::Location> {
    if location.source_name() != source.name() || location.source_identity() != source.identity() {
        return None;
    }
    let range = to_lsp_range(source, location.span(), encoding)?;
    Some(lsp_types::Location { uri, range })
}
