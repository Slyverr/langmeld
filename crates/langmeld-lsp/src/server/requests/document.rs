use super::super::capabilities::hover_for;
use super::super::{LanguageBackend, ServerState};
use crate::{
    conversion::{byte_offset, completion_items, to_lsp_range},
    semantic::encode,
};
use lsp_types::{
    CodeAction, CodeActionOrCommand, CodeActionParams, CompletionParams, DocumentFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, HoverParams, PrepareRenameResponse,
    SemanticTokensParams, TextDocumentPositionParams, TextEdit, WorkspaceEdit,
};
use std::collections::HashMap;

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Option<PrepareRenameResponse> {
        let document = self.documents.get(&params.text_document.uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return None;
        }

        let offset = byte_offset(source, params.position, self.encoding)?;
        let target = self
            .backend
            .prepare_rename(source, &document.analysis, offset)?;
        let range = to_lsp_range(source, target.range, self.encoding)?;

        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range,
            placeholder: target.placeholder,
        })
    }

    pub(super) fn code_actions(
        &self,
        params: CodeActionParams,
    ) -> Option<Vec<CodeActionOrCommand>> {
        let document = self.documents.get(&params.text_document.uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(Vec::new());
        }

        let start = byte_offset(source, params.range.start, self.encoding)?;
        let end = byte_offset(source, params.range.end, self.encoding)?;
        if start > end {
            return Some(Vec::new());
        }

        let actions = self.backend.code_actions(
            source,
            &document.analysis,
            &params,
            langmeld_core::Span::new(start, end),
        );

        Some(
            actions
                .into_iter()
                .filter_map(|action| {
                    let mut edits_by_uri: HashMap<String, Vec<(langmeld_core::Span, TextEdit)>> =
                        HashMap::new();

                    for edit in &action.edits {
                        let target = self.resolve_location_source(&edit.location)?;
                        if !target.source().is_valid_span(edit.location.span()) {
                            return None;
                        }
                        let range =
                            to_lsp_range(target.source(), edit.location.span(), self.encoding)?;
                        edits_by_uri
                            .entry(target.uri().to_string())
                            .or_default()
                            .push((
                                edit.location.span(),
                                TextEdit {
                                    range,
                                    new_text: edit.new_text.clone(),
                                },
                            ));
                    }

                    #[expect(
                        clippy::mutable_key_type,
                        reason = "WorkspaceEdit requires URI keys, whose equality remains stable here"
                    )]
                    let mut changes = HashMap::with_capacity(edits_by_uri.len());
                    for (uri, mut edits) in edits_by_uri {
                        edits.sort_by_key(|(span, _)| (span.start, span.end));
                        if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
                            return None;
                        }
                        changes.insert(
                            uri.parse().ok()?,
                            edits.into_iter().map(|(_, edit)| edit).collect(),
                        );
                    }

                    if changes.is_empty() {
                        return action.command.map(CodeActionOrCommand::Command);
                    }

                    Some(CodeActionOrCommand::CodeAction(CodeAction {
                        title: action.title,
                        kind: action.kind,
                        diagnostics: (!action.diagnostics.is_empty())
                            .then_some(action.diagnostics),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..WorkspaceEdit::default()
                        }),
                        command: action.command,
                        is_preferred: action.is_preferred.then_some(true),
                        disabled: None,
                        data: None,
                    }))
                })
                .collect(),
        )
    }

    pub(super) fn completion(
        &self,
        params: CompletionParams,
    ) -> Option<Vec<lsp_types::CompletionItem>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        let candidates = self.backend.complete(source, &document.analysis, offset);
        Some(completion_items(
            source,
            candidates,
            self.encoding,
            self.client_supports_snippets,
        ))
    }

    pub(super) fn semantic_tokens(
        &self,
        params: SemanticTokensParams,
    ) -> Option<lsp_types::SemanticTokens> {
        let document = self.documents.get(&params.text_document.uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(lsp_types::SemanticTokens {
                result_id: None,
                data: Vec::new(),
            });
        }
        Some(encode(
            source,
            &self.semantic_legend,
            self.backend.semantic_tokens(source, &document.analysis),
            self.encoding,
        ))
    }

    pub(super) fn format_document(
        &self,
        params: DocumentFormattingParams,
    ) -> Option<Vec<TextEdit>> {
        let document = self.documents.get(&params.text_document.uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(Vec::new());
        }
        let Some(text) = self
            .backend
            .format_document(source, &document.analysis, &params.options)
        else {
            return Some(Vec::new());
        };
        if text == source.text() {
            return Some(Vec::new());
        }
        let range = to_lsp_range(
            source,
            langmeld_core::Span::new(0, source.text().len()),
            self.encoding,
        )?;
        Some(vec![TextEdit {
            range,
            new_text: text,
        }])
    }

    pub(super) fn hover(&self, params: HoverParams) -> Option<lsp_types::Hover> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return None;
        }
        let offset = byte_offset(source, position, self.encoding)?;
        let info = self.backend.hover(source, &document.analysis, offset)?;
        hover_for(source, info, self.encoding, self.supports_markdown_hover)
    }

    pub(super) fn document_symbols(
        &self,
        params: DocumentSymbolParams,
    ) -> Option<DocumentSymbolResponse> {
        let uri = params.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return None;
        }
        crate::document_symbol::convert(
            source,
            &uri,
            self.backend.document_symbols(source, &document.analysis),
            self.encoding,
            self.supports_hierarchical_document_symbols,
        )
    }
}
