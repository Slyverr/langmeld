use super::super::capabilities::hover_for;
use super::super::{LanguageBackend, ServerState};
use crate::{
    conversion::{byte_offset, completion_items, to_lsp_range},
    semantic::encode,
};
use lsp_types::{
    CompletionParams, DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse,
    HoverParams, SemanticTokensParams, TextEdit,
};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn completion(
        &self,
        params: CompletionParams,
    ) -> Option<Vec<lsp_types::CompletionItem>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if document.analysis.source_identity() != source.identity() {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        let candidates = self.backend.complete(source, &document.analysis, offset);
        Some(completion_items(source, candidates, self.encoding))
    }

    pub(super) fn semantic_tokens(
        &self,
        params: SemanticTokensParams,
    ) -> Option<lsp_types::SemanticTokens> {
        let document = self.documents.get(&params.text_document.uri)?;
        let source = self.sources.get(document.source_id)?;
        if document.analysis.source_identity() != source.identity() {
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
        if document.analysis.source_identity() != source.identity() {
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
        if document.analysis.source_identity() != source.identity() {
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
        if document.analysis.source_identity() != source.identity() {
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
