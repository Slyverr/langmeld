use std::collections::HashMap;

use lsp_server::{ErrorCode, Request, Response};
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, HoverParams, Location,
    ReferenceParams, RenameParams, SemanticTokensParams, SymbolInformation, TextEdit,
    WorkspaceEdit, WorkspaceSymbolParams,
};
use serde_json::{Value, json};

use crate::{
    conversion::{byte_offset, completion_items, lsp_location, to_lsp_range},
    semantic::encode,
};

use super::capabilities::hover_for;
use super::{LanguageBackend, ResolvedLocationSource, ServerState};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn request(&self, request: Request) -> Response {
        let id = request.id.clone();
        let result = match request.method.as_str() {
            "textDocument/completion" if self.capabilities.completion => {
                serde_json::from_value::<CompletionParams>(request.params).map(|params| {
                    self.completion(params)
                        .map(|items| json!(CompletionResponse::Array(items)))
                })
            }
            "textDocument/definition" if self.capabilities.definition => {
                serde_json::from_value::<GotoDefinitionParams>(request.params).map(|params| {
                    self.definition(params)
                        .map(|locations| json!(GotoDefinitionResponse::Array(locations)))
                })
            }
            "textDocument/references" if self.capabilities.references => {
                serde_json::from_value::<ReferenceParams>(request.params)
                    .map(|params| self.references(params).map(|locations| json!(locations)))
            }
            "textDocument/semanticTokens/full" if self.capabilities.semantic_tokens => {
                serde_json::from_value::<SemanticTokensParams>(request.params)
                    .map(|params| self.semantic_tokens(params).map(|tokens| json!(tokens)))
            }
            "textDocument/formatting" if self.capabilities.formatting => {
                serde_json::from_value::<DocumentFormattingParams>(request.params)
                    .map(|params| self.format_document(params).map(|edits| json!(edits)))
            }
            "textDocument/hover" if self.capabilities.hover => {
                serde_json::from_value::<HoverParams>(request.params)
                    .map(|params| self.hover(params).map(|hover| json!(hover)))
            }
            "textDocument/rename" if self.capabilities.rename => {
                serde_json::from_value::<RenameParams>(request.params)
                    .map(|params| self.rename(params).map(|edit| json!(edit)))
            }
            "textDocument/documentSymbol" if self.capabilities.document_symbols => {
                serde_json::from_value::<DocumentSymbolParams>(request.params)
                    .map(|params| self.document_symbols(params).map(|symbols| json!(symbols)))
            }
            "workspace/symbol" if self.capabilities.workspace_symbols => {
                serde_json::from_value::<WorkspaceSymbolParams>(request.params)
                    .map(|params| Some(json!(self.workspace_symbols(&params.query))))
            }
            _ => {
                return Response::new_err(
                    id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unsupported method `{}`", request.method),
                );
            }
        };
        match result {
            Ok(Some(value)) => Response::new_ok(id, value),
            Ok(None) => Response::new_ok(id, Value::Null),
            Err(error) => Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string()),
        }
    }

    fn completion(&self, params: CompletionParams) -> Option<Vec<lsp_types::CompletionItem>> {
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

    fn definition(&self, params: GotoDefinitionParams) -> Option<Vec<Location>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if document.analysis.source_identity() != source.identity() {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        Some(
            self.backend
                .definition(source, &document.analysis, offset)
                .iter()
                .filter_map(|location| self.lsp_location(location))
                .collect(),
        )
    }

    fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if document.analysis.source_identity() != source.identity() {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        Some(
            self.backend
                .references(
                    source,
                    &document.analysis,
                    offset,
                    params.context.include_declaration,
                )
                .iter()
                .filter_map(|location| self.lsp_location(location))
                .collect(),
        )
    }

    fn semantic_tokens(&self, params: SemanticTokensParams) -> Option<lsp_types::SemanticTokens> {
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

    fn format_document(&self, params: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
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

    fn hover(&self, params: HoverParams) -> Option<lsp_types::Hover> {
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

    fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if document.analysis.source_identity() != source.identity() {
            return None;
        }
        let offset = byte_offset(source, position, self.encoding)?;
        let locations =
            self.backend
                .rename(source, &document.analysis, offset, &params.new_name)?;
        if locations.is_empty() {
            return None;
        }
        let mut edits_by_uri: HashMap<String, Vec<(langmeld_core::Span, TextEdit)>> =
            HashMap::new();
        for location in locations {
            let target = self.resolve_location_source(&location)?;
            let target_uri = target.uri();
            let target_source = target.source();
            let span = location.span;
            if span.is_empty()
                || span.end > target_source.text().len()
                || !target_source.text().is_char_boundary(span.start)
                || !target_source.text().is_char_boundary(span.end)
            {
                return None;
            }
            let range = to_lsp_range(target_source, span, self.encoding)?;
            edits_by_uri
                .entry(target_uri.to_string())
                .or_default()
                .push((
                    span,
                    TextEdit {
                        range,
                        new_text: params.new_name.clone(),
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
        Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        })
    }

    fn document_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
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

    fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        self.backend
            .workspace_symbols(query)
            .into_iter()
            .filter_map(|symbol| {
                if symbol.name.is_empty() {
                    return None;
                }
                let location = self.lsp_location(&symbol.location)?;
                #[expect(
                    deprecated,
                    reason = "lsp-types 0.97 requires the deprecated field during construction"
                )]
                Some(SymbolInformation {
                    name: symbol.name,
                    kind: symbol.kind.into(),
                    tags: None,
                    deprecated: None,
                    location,
                    container_name: symbol.container_name,
                })
            })
            .collect()
    }

    fn lsp_location(&self, location: &langmeld_core::SourceLocation) -> Option<Location> {
        let target = self.resolve_location_source(location)?;
        lsp_location(
            location,
            target.uri().clone(),
            target.source(),
            self.encoding,
        )
    }

    fn resolve_location_source<'a>(
        &'a self,
        location: &langmeld_core::SourceLocation,
    ) -> Option<ResolvedLocationSource<'a>> {
        if let Some((uri, source)) = self.documents.iter().find_map(|(uri, document)| {
            let source = self.sources.get(document.source_id)?;
            (source.name() == location.source_name && source.identity() == location.identity)
                .then_some((uri, source))
        }) {
            return Some(ResolvedLocationSource::Open { uri, source });
        }

        let target = self.backend.source_for_location(location)?;
        (target.source.name() == location.source_name
            && target.source.identity() == location.identity)
            .then_some(ResolvedLocationSource::External(target))
    }
}
