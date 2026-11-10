use std::{collections::HashMap, error::Error};

use langmeld_core::{PositionEncoding, SourceDatabase, SourceId};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    HoverParams, InitializeParams, InitializeResult, Location, MarkupContent, MarkupKind,
    PositionEncodingKind, PublishDiagnosticsParams, ReferenceParams, RenameParams,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Uri, WorkspaceEdit,
};
use serde_json::{Value, json};

use crate::{
    backend::{BackendCapabilities, BackendDocument, LanguageBackend},
    conversion::{byte_offset, completion_items, diagnostics_for, lsp_location, to_lsp_range},
    hover::{HoverContent, HoverInfo},
    semantic::{SemanticTokenLegend, encode},
};

struct OpenDocument<D> {
    source_id: SourceId,
    version: i32,
    analysis: BackendDocument<D>,
}

/// Runs the server over standard input and output.
pub fn run_stdio<B: LanguageBackend>(backend: B) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    run_connection(connection, backend)?;
    io_threads.join()?;
    Ok(())
}

/// Runs the server over an LSP connection. The connection can be stdio, TCP,
/// or in-memory for protocol tests.
pub fn run_connection<B: LanguageBackend>(
    connection: Connection,
    backend: B,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;
    let encoding = negotiate_encoding(&initialize_params);
    let supports_markdown_hover = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|capabilities| capabilities.hover.as_ref())
        .and_then(|capabilities| capabilities.content_format.as_ref())
        .is_some_and(|formats| formats.contains(&MarkupKind::Markdown));
    let supports_hierarchical_document_symbols = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|capabilities| capabilities.document_symbol.as_ref())
        .and_then(|capabilities| capabilities.hierarchical_document_symbol_support)
        .unwrap_or(false);
    let capabilities = backend.capabilities();
    let semantic_legend = if capabilities.semantic_tokens {
        backend.semantic_token_legend()
    } else {
        SemanticTokenLegend::default()
    };
    if capabilities.semantic_tokens {
        semantic_legend
            .validate()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    }
    let result = InitializeResult {
        capabilities: server_capabilities(capabilities, encoding, &semantic_legend),
        server_info: Some(ServerInfo {
            name: "langmeld".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let mut server = ServerState {
        backend,
        capabilities,
        semantic_legend,
        supports_markdown_hover,
        supports_hierarchical_document_symbols,
        encoding,
        sources: SourceDatabase::default(),
        documents: HashMap::new(),
    };
    loop {
        let message = connection.receiver.recv()?;
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = server.request(request);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    break;
                }
                if let Some(params) = parse_notification::<DidOpenTextDocumentParams>(
                    &notification,
                    "textDocument/didOpen",
                ) {
                    server.open(params, &connection)?;
                } else if let Some(params) = parse_notification::<DidChangeTextDocumentParams>(
                    &notification,
                    "textDocument/didChange",
                ) {
                    server.change(params, &connection)?;
                } else if let Some(params) = parse_notification::<DidCloseTextDocumentParams>(
                    &notification,
                    "textDocument/didClose",
                ) {
                    server.close(params, &connection)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

struct ServerState<B: LanguageBackend> {
    backend: B,
    capabilities: BackendCapabilities,
    semantic_legend: SemanticTokenLegend,
    supports_markdown_hover: bool,
    supports_hierarchical_document_symbols: bool,
    encoding: PositionEncoding,
    sources: SourceDatabase,
    documents: HashMap<Uri, OpenDocument<B::Document>>,
}

impl<B: LanguageBackend> ServerState<B> {
    fn request(&self, request: Request) -> Response {
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
        if document.analysis.source != source.identity() {
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
        if document.analysis.source != source.identity() {
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
        if document.analysis.source != source.identity() {
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
        if document.analysis.source != source.identity() {
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
        if document.analysis.source != source.identity() {
            return Some(Vec::new());
        }
        let Some(text) = self
            .backend
            .format_document(source, &document.analysis, &params.options)
        else {
            return Some(Vec::new());
        };
        if text == source.text {
            return Some(Vec::new());
        }
        let range = to_lsp_range(
            source,
            langmeld_core::Span::new(0, source.text.len()),
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
        if document.analysis.source != source.identity() {
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
        if document.analysis.source != source.identity() {
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
            let mut targets = self.documents.iter().filter_map(|(uri, document)| {
                let target_source = self.sources.get(document.source_id)?;
                (location.source_name == target_source.name
                    && location.identity == target_source.identity())
                .then_some((uri, target_source))
            });
            let target = targets.next()?;
            if targets.next().is_some() {
                return None;
            }
            let (target_uri, target_source) = target;
            let span = location.span;
            if span.is_empty()
                || span.end > target_source.text.len()
                || !target_source.text.is_char_boundary(span.start)
                || !target_source.text.is_char_boundary(span.end)
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
        if document.analysis.source != source.identity() {
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

    fn lsp_location(&self, location: &langmeld_core::SourceLocation) -> Option<Location> {
        self.documents.iter().find_map(|(uri, document)| {
            let source = self.sources.get(document.source_id)?;
            lsp_location(location, uri.clone(), source, self.encoding)
        })
    }

    fn open(
        &mut self,
        params: DidOpenTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let item = params.text_document;
        let uri = item.uri;
        if self
            .documents
            .get(&uri)
            .is_some_and(|document| document.version > item.version)
        {
            return Ok(());
        }
        let source_id = self.sources.upsert(uri.as_str(), item.text)?;
        self.refresh(uri, source_id, item.version, connection)
    }

    fn change(
        &mut self,
        params: DidChangeTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let uri = params.text_document.uri;
        let Some(current) = self.documents.get(&uri) else {
            return Ok(());
        };
        if params.text_document.version <= current.version {
            return Ok(());
        }
        let Some(change) = params.content_changes.last() else {
            return Ok(());
        };
        let source_id = self.sources.upsert(uri.as_str(), change.text.clone())?;
        self.refresh(uri, source_id, params.text_document.version, connection)
    }

    fn close(
        &mut self,
        params: DidCloseTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let uri = params.text_document.uri;
        if let Some(document) = self.documents.remove(&uri) {
            if let Some(source) = self.sources.get(document.source_id) {
                self.backend.did_close(source, &document.analysis);
            }
            self.sources.remove(document.source_id);
            publish_diagnostics(connection, uri, None, Vec::new())?;
        }
        Ok(())
    }

    fn refresh(
        &mut self,
        uri: Uri,
        source_id: SourceId,
        version: i32,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let source = self
            .sources
            .get(source_id)
            .expect("upserted document source exists")
            .clone();
        let analysis = self.backend.analyze(&source);
        let diagnostics = diagnostics_for(&source, &analysis, self.encoding);
        self.documents.insert(
            uri.clone(),
            OpenDocument {
                source_id,
                version,
                analysis,
            },
        );
        publish_diagnostics(connection, uri, Some(version), diagnostics)
    }
}

fn publish_diagnostics(
    connection: &Connection,
    uri: Uri,
    version: Option<i32>,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version,
    };
    connection.sender.send(Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".into(),
        params: serde_json::to_value(params)?,
    }))?;
    Ok(())
}

fn parse_notification<T: serde::de::DeserializeOwned>(
    notification: &Notification,
    method: &str,
) -> Option<T> {
    (notification.method == method)
        .then(|| serde_json::from_value(notification.params.clone()).ok())
        .flatten()
}

fn negotiate_encoding(params: &InitializeParams) -> PositionEncoding {
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

fn server_capabilities(
    backend: BackendCapabilities,
    encoding: PositionEncoding,
    semantic_legend: &SemanticTokenLegend,
) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(match encoding {
            PositionEncoding::Utf8 => PositionEncodingKind::UTF8,
            PositionEncoding::Utf16 => PositionEncodingKind::UTF16,
            PositionEncoding::Utf32 => PositionEncodingKind::UTF32,
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
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
        ..ServerCapabilities::default()
    }
}

fn hover_for(
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
