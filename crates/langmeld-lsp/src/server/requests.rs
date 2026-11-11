use lsp_server::{ErrorCode, Request, Response};
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentFormattingParams, DocumentSymbolParams,
    GotoDefinitionParams, GotoDefinitionResponse, HoverParams, ReferenceParams, RenameParams,
    SemanticTokensParams, WorkspaceSymbolParams,
};
use serde_json::{Value, json};

use super::{LanguageBackend, ServerState};

mod document;
mod navigation;

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
}
