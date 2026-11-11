use langmeld_core::PositionEncoding;
use lsp_types::{
    CompletionOptions, InitializeParams, MarkupContent, MarkupKind, PositionEncodingKind,
    SemanticTokensOptions, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};

use crate::{
    backend::BackendCapabilities,
    conversion::to_lsp_range,
    hover::{HoverContent, HoverInfo},
    semantic::SemanticTokenLegend,
};

pub(super) fn negotiate_encoding(params: &InitializeParams) -> PositionEncoding {
    params
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref())
        .and_then(|encodings| {
            encodings
                .iter()
                .find_map(|encoding| match encoding.as_str() {
                    "utf-8" => Some(PositionEncoding::Utf8),
                    "utf-16" => Some(PositionEncoding::Utf16),
                    "utf-32" => Some(PositionEncoding::Utf32),
                    _ => None,
                })
        })
        .unwrap_or(PositionEncoding::Utf16)
}

pub(super) fn server_capabilities(
    backend: BackendCapabilities,
    workspace_folders: bool,
    encoding: PositionEncoding,
    semantic_legend: &SemanticTokenLegend,
    client_supports_workspace_folders: bool,
) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(match encoding {
            PositionEncoding::Utf8 => PositionEncodingKind::UTF8,
            PositionEncoding::Utf16 => PositionEncodingKind::UTF16,
            PositionEncoding::Utf32 => PositionEncodingKind::UTF32,
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        completion_provider: backend.completion.then(CompletionOptions::default),
        definition_provider: backend.definition.then_some(lsp_types::OneOf::Left(true)),
        references_provider: backend.references.then_some(lsp_types::OneOf::Left(true)),
        semantic_tokens_provider: backend.semantic_tokens.then(|| {
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                work_done_progress_options: Default::default(),
                legend: lsp_types::SemanticTokensLegend {
                    token_types: semantic_legend
                        .token_types
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect(),
                    token_modifiers: semantic_legend
                        .token_modifiers
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect(),
                },
                range: Some(false),
                full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
            })
        }),
        document_formatting_provider: backend.formatting.then_some(lsp_types::OneOf::Left(true)),
        hover_provider: backend
            .hover
            .then_some(lsp_types::HoverProviderCapability::Simple(true)),
        rename_provider: backend.rename.then_some(lsp_types::OneOf::Left(true)),
        document_symbol_provider: backend
            .document_symbols
            .then_some(lsp_types::OneOf::Left(true)),
        workspace_symbol_provider: backend
            .workspace_symbols
            .then_some(lsp_types::OneOf::Left(true)),
        workspace: workspace_folders.then(|| WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: client_supports_workspace_folders
                    .then_some(lsp_types::OneOf::Left(true)),
            }),
            ..WorkspaceServerCapabilities::default()
        }),
        ..ServerCapabilities::default()
    }
}

pub(super) fn hover_for(
    source: &langmeld_core::SourceFile,
    info: HoverInfo,
    encoding: PositionEncoding,
    supports_markdown: bool,
) -> Option<lsp_types::Hover> {
    let (kind, value) = match info.content {
        HoverContent::PlainText(value) => (MarkupKind::PlainText, value),
        HoverContent::Markdown(value) if supports_markdown => (MarkupKind::Markdown, value),
        HoverContent::Markdown(_) => return None,
    };
    let range = info.range.and_then(|span| {
        (span.start <= span.end
            && span.end <= source.text.len()
            && source.text.is_char_boundary(span.start)
            && source.text.is_char_boundary(span.end))
        .then(|| to_lsp_range(source, span, encoding))
        .flatten()
    });
    Some(lsp_types::Hover {
        contents: lsp_types::HoverContents::Markup(MarkupContent { kind, value }),
        range,
    })
}
