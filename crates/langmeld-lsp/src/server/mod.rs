use std::{collections::HashMap, error::Error};

use langmeld_core::{PositionEncoding, SourceDatabase, SourceFile, SourceId};
use lsp_server::{Connection, Message, RequestId};
use lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, MarkupKind, ServerInfo, Uri,
};

use crate::{
    backend::{
        BackendCapabilities, BackendDocument, BackendSource, BackendWorkspace,
        BackendWorkspaceCapabilities, LanguageBackend,
    },
    semantic::SemanticTokenLegend,
};

struct OpenDocument<D> {
    source_id: SourceId,
    version: i32,
    analysis: BackendDocument<D>,
}

enum ResolvedLocationSource<'a> {
    Open {
        uri: &'a Uri,
        source: &'a SourceFile,
    },
    External(BackendSource),
}

impl ResolvedLocationSource<'_> {
    fn uri(&self) -> &Uri {
        match self {
            Self::Open { uri, .. } => uri,
            Self::External(source) => source.uri(),
        }
    }

    fn source(&self) -> &SourceFile {
        match self {
            Self::Open { source, .. } => source,
            Self::External(source) => source.source(),
        }
    }
}

#[derive(Clone, Copy)]
enum WorkspaceRefreshTrigger {
    DocumentChanged,
    DocumentClosed,
    WorkspaceChanged,
}

const WORKSPACE_REGISTRATION_REQUEST_ID: &str = "langmeld-workspace-registration";

#[derive(Clone, Copy)]
struct PendingWorkspaceRegistration {
    configuration_changes: bool,
    watched_files: bool,
}

/// Runs the server over standard input and output.
///
/// # Errors
///
/// Returns an I/O or protocol error when the connection cannot be initialized
/// or served, or when its I/O threads cannot be joined.
pub fn run_stdio<B: LanguageBackend>(backend: B) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    run_connection(connection, backend)?;
    io_threads.join()?;
    Ok(())
}

/// Runs the server over an LSP connection. The connection can be stdio, TCP,
/// or in-memory for protocol tests.
///
/// # Errors
///
/// Returns a transport, JSON-RPC, or protocol error when initialization or
/// message handling fails.
pub fn run_connection<B: LanguageBackend>(
    connection: Connection,
    backend: B,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;
    let encoding = negotiate_encoding(&initialize_params);
    let client_supports_snippets = supports_completion_snippets(&initialize_params);
    let client_supports_workspace_folders = initialize_params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.workspace_folders)
        .unwrap_or(false);
    let client_workspace = initialize_params.capabilities.workspace.as_ref();
    let client_supports_configuration_registration = client_workspace
        .and_then(|workspace| workspace.did_change_configuration.as_ref())
        .and_then(|capabilities| capabilities.dynamic_registration)
        .unwrap_or(false);
    let client_supports_file_watcher_registration = client_workspace
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|capabilities| capabilities.dynamic_registration)
        .unwrap_or(false);
    let client_supports_relative_file_watcher_patterns = client_workspace
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|capabilities| capabilities.relative_pattern_support)
        .unwrap_or(false);
    #[expect(
        deprecated,
        reason = "rootUri remains part of the LSP initialize compatibility contract"
    )]
    let workspace = BackendWorkspace {
        root_uri: initialize_params.root_uri.clone(),
        workspace_folders: initialize_params.workspace_folders.clone(),
        supports_workspace_folders: client_supports_workspace_folders,
        supports_relative_file_watcher_patterns: client_supports_relative_file_watcher_patterns,
        initialization_options: initialize_params.initialization_options.clone(),
    };
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
    backend.initialize_workspace(&workspace);
    let workspace_capabilities = backend.workspace_capabilities();
    let result = InitializeResult {
        capabilities: server_capabilities(
            capabilities,
            workspace_capabilities.workspace_folders,
            encoding,
            &semantic_legend,
            client_supports_workspace_folders,
        ),
        server_info: Some(ServerInfo {
            name: "langmeld".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let mut server = ServerState {
        backend,
        capabilities,
        workspace_capabilities,
        client_supports_workspace_folders,
        client_supports_configuration_registration,
        client_supports_file_watcher_registration,
        client_supports_relative_file_watcher_patterns,
        pending_workspace_registration: None,
        configuration_registered: false,
        watched_files_registered: false,
        semantic_legend,
        supports_markdown_hover,
        supports_hierarchical_document_symbols,
        client_supports_snippets,
        encoding,
        sources: SourceDatabase::default(),
        documents: HashMap::new(),
    };
    // initialize_finish consumes the client's `initialized` notification.
    server.register_workspace_capabilities(&connection)?;
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
                } else if server.workspace_capabilities.workspace_folders
                    && server.client_supports_workspace_folders
                    && let Some(params) = parse_notification::<DidChangeWorkspaceFoldersParams>(
                        &notification,
                        "workspace/didChangeWorkspaceFolders",
                    )
                {
                    server.backend.did_change_workspace_folders(&params.event);
                    server.refresh_workspace(
                        None,
                        WorkspaceRefreshTrigger::WorkspaceChanged,
                        &connection,
                    )?;
                } else if server.configuration_registered
                    && let Some(params) = parse_notification::<DidChangeConfigurationParams>(
                        &notification,
                        "workspace/didChangeConfiguration",
                    )
                {
                    server.backend.did_change_configuration(&params.settings);
                    server.refresh_workspace(
                        None,
                        WorkspaceRefreshTrigger::WorkspaceChanged,
                        &connection,
                    )?;
                } else if server.watched_files_registered
                    && let Some(params) = parse_notification::<DidChangeWatchedFilesParams>(
                        &notification,
                        "workspace/didChangeWatchedFiles",
                    )
                {
                    server.backend.did_change_watched_files(&params.changes);
                    server.refresh_workspace(
                        None,
                        WorkspaceRefreshTrigger::WorkspaceChanged,
                        &connection,
                    )?;
                }
            }
            Message::Response(response) => {
                let Some(pending) = server.pending_workspace_registration else {
                    continue;
                };
                if response.id == RequestId::from(WORKSPACE_REGISTRATION_REQUEST_ID.to_owned()) {
                    server.pending_workspace_registration = None;
                    if response.response_result.is_ok() {
                        server.configuration_registered = pending.configuration_changes;
                        server.watched_files_registered = pending.watched_files;
                    }
                }
            }
        }
    }
    Ok(())
}

struct ServerState<B: LanguageBackend> {
    backend: B,
    capabilities: BackendCapabilities,
    workspace_capabilities: BackendWorkspaceCapabilities,
    client_supports_workspace_folders: bool,
    client_supports_configuration_registration: bool,
    client_supports_file_watcher_registration: bool,
    client_supports_relative_file_watcher_patterns: bool,
    pending_workspace_registration: Option<PendingWorkspaceRegistration>,
    configuration_registered: bool,
    watched_files_registered: bool,
    semantic_legend: SemanticTokenLegend,
    supports_markdown_hover: bool,
    supports_hierarchical_document_symbols: bool,
    client_supports_snippets: bool,
    encoding: PositionEncoding,
    sources: SourceDatabase,
    documents: HashMap<Uri, OpenDocument<B::Document>>,
}

mod capabilities;
mod documents;
mod requests;
mod workspace;

use capabilities::{negotiate_encoding, server_capabilities, supports_completion_snippets};
use documents::parse_notification;
