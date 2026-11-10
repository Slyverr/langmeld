use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use langmeld_core::{Diagnostic, SourceFile, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, ConstructSpec, LanguageAnalyzer, LanguageCompleter,
    LanguageNavigator, LanguageParser, LanguageSpec, LanguageWorkspaceAnalyzer, ParsedDocument,
    PatternPart,
};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, BackendWorkspace, BackendWorkspaceCapabilities,
    DeclarativeBackend, DocumentSymbolInfo, DocumentSymbolKind, HostLanguageBackend, HoverInfo,
    LanguageBackend, SemanticToken, SemanticTokenLegend, run_connection,
};
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

#[derive(Default)]
struct Backend(Arc<Mutex<HashMap<String, SourceFile>>>);

impl LanguageBackend for Backend {
    type Document = String;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        self.0
            .lock()
            .unwrap()
            .insert(source.name.clone(), source.clone());
        let diagnostics = if source.text.contains("bad") {
            vec![Diagnostic::error(
                "TEST-001",
                "replace this token",
                source,
                Span::new(0, "😀".len()),
            )]
        } else {
            Vec::new()
        };
        BackendDocument::new(source, source.text.clone(), diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            definition: true,
            references: true,
            semantic_tokens: true,
            formatting: true,
            hover: true,
            rename: true,
            document_symbols: true,
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<String>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        assert_eq!(document.state, source.text);
        assert_eq!(offset, "😀".len());
        vec![CompletionCandidate {
            text: "good".into(),
            replace: Span::new("😀".len(), source.text.len()),
        }]
    }

    fn definition(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        vec![SourceLocation {
            source_name: source.name.clone(),
            identity: source.identity(),
            span: Span::new(0, "😀".len()),
        }]
    }

    fn references(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        _offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        include_declaration
            .then(|| SourceLocation {
                source_name: source.name.clone(),
                identity: source.identity(),
                span: Span::new(0, "😀".len()),
            })
            .into_iter()
            .collect()
    }

    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        SemanticTokenLegend::new(["variable"], ["declaration"])
    }

    fn semantic_tokens(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
    ) -> Vec<SemanticToken> {
        if source.text.contains('\n') {
            return vec![SemanticToken::new(
                Span::new(0, source.text.len()),
                "variable",
            )];
        }
        vec![
            SemanticToken::new(Span::new("😀".len(), "😀bad".len()), "variable"),
            SemanticToken::new(Span::new(0, "😀".len()), "variable").with_modifier("declaration"),
            SemanticToken::new(Span::new(2, 5), "variable"),
            SemanticToken::new(Span::new(4, 7), "unknown"),
        ]
    }

    fn format_document(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        options: &lsp_types::FormattingOptions,
    ) -> Option<String> {
        let indentation = if options.insert_spaces {
            " ".repeat(options.tab_size as usize)
        } else {
            "\t".to_owned()
        };
        Some(format!("{indentation}{}", source.text))
    }

    fn hover(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<String>,
        offset: usize,
    ) -> Option<HoverInfo> {
        assert_eq!(offset, "😀".len());
        Some(HoverInfo::markdown("**bad**").with_range(Span::new(4, 7)))
    }

    fn rename(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        offset: usize,
        new_name: &str,
    ) -> Option<Vec<SourceLocation>> {
        assert_eq!(offset, "😀".len());
        if new_name == "empty" {
            return Some(Vec::new());
        }
        if new_name == "cross" {
            let other = self.0.lock().unwrap().get("file:///other.test").cloned()?;
            return Some(
                [(source, Span::new(4, 7)), (&other, Span::new(0, 5))]
                    .into_iter()
                    .map(|(source, span)| SourceLocation {
                        source_name: source.name.clone(),
                        identity: source.identity(),
                        span,
                    })
                    .collect(),
            );
        }
        let spans = if new_name == "overlap" {
            [Span::new(0, 7), Span::new(4, 7)]
        } else {
            [Span::new(4, 7), Span::new(0, 4)]
        };
        let mut locations = spans
            .into_iter()
            .map(|span| SourceLocation {
                source_name: source.name.clone(),
                identity: source.identity(),
                span,
            })
            .collect::<Vec<_>>();
        if new_name == "missing" {
            locations.push(SourceLocation {
                source_name: "file:///closed.test".into(),
                identity: source.identity(),
                span: Span::new(0, 4),
            });
        }
        Some(locations)
    }

    fn document_symbols(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
    ) -> Vec<DocumentSymbolInfo> {
        if source.name == "file:///invalid-symbol.test" {
            return vec![
                DocumentSymbolInfo::new(
                    "invalid",
                    DocumentSymbolKind::Variable,
                    Span::new(0, 4),
                    Span::new(0, 1),
                ),
                DocumentSymbolInfo::new(
                    "valid",
                    DocumentSymbolKind::Variable,
                    Span::new(4, 7),
                    Span::new(4, 7),
                ),
                DocumentSymbolInfo::new(
                    "root",
                    DocumentSymbolKind::Module,
                    Span::new(0, 7),
                    Span::new(4, 7),
                )
                .with_children([
                    DocumentSymbolInfo::new(
                        "invalid-child",
                        DocumentSymbolKind::Variable,
                        Span::new(4, 8),
                        Span::new(4, 7),
                    ),
                    DocumentSymbolInfo::new(
                        "valid-child",
                        DocumentSymbolKind::Variable,
                        Span::new(4, 7),
                        Span::new(4, 7),
                    ),
                ]),
            ];
        }
        vec![
            DocumentSymbolInfo::new(
                "root",
                DocumentSymbolKind::Module,
                Span::new(0, 7),
                Span::new(0, 4),
            )
            .with_children([DocumentSymbolInfo::new(
                "bad",
                DocumentSymbolKind::Variable,
                Span::new(4, 7),
                Span::new(4, 7),
            )]),
        ]
    }
}

fn send_request(connection: &Connection, id: i32, method: &str, params: Value) {
    connection
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(id),
            method: method.into(),
            params,
        }))
        .unwrap();
}

fn send_notification(connection: &Connection, method: &str, params: Value) {
    connection
        .sender
        .send(Message::Notification(Notification {
            method: method.into(),
            params,
        }))
        .unwrap();
}

fn receive(connection: &Connection) -> Message {
    connection
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server should respond")
}

fn receive_response(connection: &Connection) -> Value {
    match receive(connection) {
        Message::Response(response) => response.response_result.expect("successful LSP response"),
        message => panic!("expected response, got {message:?}"),
    }
}

fn receive_diagnostics(connection: &Connection) -> Value {
    match receive(connection) {
        Message::Notification(notification) => {
            assert_eq!(notification.method, "textDocument/publishDiagnostics");
            notification.params
        }
        message => panic!("expected diagnostics notification, got {message:?}"),
    }
}

#[derive(Clone, Default)]
struct WorkspaceBackend {
    initialized: Arc<Mutex<Option<BackendWorkspace>>>,
    changes: Arc<Mutex<Vec<lsp_types::WorkspaceFoldersChangeEvent>>>,
}

impl LanguageBackend for WorkspaceBackend {
    type Document = ();

    fn initialize_workspace(&self, workspace: &BackendWorkspace) {
        *self.initialized.lock().unwrap() = Some(workspace.clone());
    }

    fn did_change_workspace_folders(&self, event: &lsp_types::WorkspaceFoldersChangeEvent) {
        self.changes.lock().unwrap().push(event.clone());
    }

    fn workspace_capabilities(&self) -> BackendWorkspaceCapabilities {
        BackendWorkspaceCapabilities {
            workspace_folders: true,
            ..BackendWorkspaceCapabilities::default()
        }
    }

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        BackendDocument::new(source, (), Vec::new())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}

#[test]
fn forwards_initial_workspace_context_and_folder_changes_to_opted_in_backends() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = WorkspaceBackend::default();
    let observed = backend.clone();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "rootUri": "file:///workspace",
            "workspaceFolders": [
                { "uri": "file:///workspace", "name": "workspace" }
            ],
            "initializationOptions": { "target": "web" },
            "capabilities": {
                "workspace": { "workspaceFolders": true }
            }
        }),
    );

    let initialized = receive_response(&client);
    assert_eq!(
        initialized["capabilities"]["workspace"]["workspaceFolders"]["supported"],
        true
    );
    assert_eq!(
        initialized["capabilities"]["workspace"]["workspaceFolders"]["changeNotifications"],
        true
    );
    let context = observed.initialized.lock().unwrap().clone().unwrap();
    assert_eq!(context.root_uri.unwrap().as_str(), "file:///workspace");
    assert_eq!(context.workspace_folders.as_ref().unwrap().len(), 1);
    assert_eq!(
        context.workspace_folders.as_ref().unwrap()[0].name,
        "workspace"
    );
    assert_eq!(
        context.initialization_options,
        Some(json!({ "target": "web" }))
    );

    send_notification(&client, "initialized", json!({}));
    send_notification(
        &client,
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [
                    { "uri": "file:///workspace/packages/new", "name": "new" }
                ],
                "removed": [
                    { "uri": "file:///workspace/packages/old", "name": "old" }
                ]
            }
        }),
    );
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");

    let changes = observed.changes.lock().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].added[0].name, "new");
    assert_eq!(changes[0].removed[0].name, "old");
}

#[derive(Clone, Default)]
struct WorkspaceLifecycleBackend {
    configuration: Arc<Mutex<Vec<Value>>>,
    file_changes: Arc<Mutex<Vec<Vec<lsp_types::FileEvent>>>>,
    relative_watchers: bool,
    workspace_revision: Arc<AtomicUsize>,
}

impl LanguageBackend for WorkspaceLifecycleBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let revision = self.workspace_revision.load(Ordering::Relaxed);
        BackendDocument::new(
            source,
            (),
            vec![Diagnostic::error(
                format!("REV-{revision}"),
                "workspace revision",
                source,
                Span::new(0, 0),
            )],
        )
    }

    fn did_change_workspace_folders(&self, _event: &lsp_types::WorkspaceFoldersChangeEvent) {
        self.workspace_revision.fetch_add(1, Ordering::Relaxed);
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn workspace_capabilities(&self) -> BackendWorkspaceCapabilities {
        let glob_pattern = if self.relative_watchers {
            lsp_types::GlobPattern::Relative(lsp_types::RelativePattern {
                base_uri: lsp_types::OneOf::Left(lsp_types::WorkspaceFolder {
                    uri: "file:///workspace".parse().unwrap(),
                    name: "workspace".into(),
                }),
                pattern: "**/*.langmeld".into(),
            })
        } else {
            lsp_types::GlobPattern::String("**/*.langmeld".into())
        };
        BackendWorkspaceCapabilities {
            workspace_folders: true,
            configuration_changes: true,
            watched_files: vec![lsp_types::FileSystemWatcher {
                glob_pattern,
                kind: Some(
                    lsp_types::WatchKind::Create
                        | lsp_types::WatchKind::Change
                        | lsp_types::WatchKind::Delete,
                ),
            }],
        }
    }

    fn did_change_configuration(&self, settings: &Value) {
        self.configuration.lock().unwrap().push(settings.clone());
        self.workspace_revision.fetch_add(1, Ordering::Relaxed);
    }

    fn did_change_watched_files(&self, changes: &[lsp_types::FileEvent]) {
        self.file_changes.lock().unwrap().push(changes.to_vec());
        self.workspace_revision.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn registers_and_forwards_supported_workspace_lifecycle_events() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = WorkspaceLifecycleBackend::default();
    let observed = backend.clone();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "capabilities": {
                "workspace": {
                    "didChangeConfiguration": { "dynamicRegistration": true },
                    "didChangeWatchedFiles": { "dynamicRegistration": true }
                }
            }
        }),
    );
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));

    let registration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected capability registration request, got {message:?}"),
    };
    assert_eq!(registration.method, "client/registerCapability");
    assert_eq!(
        registration.params["registrations"][0]["method"],
        "workspace/didChangeConfiguration"
    );
    assert_eq!(
        registration.params["registrations"][1]["method"],
        "workspace/didChangeWatchedFiles"
    );
    assert_eq!(
        registration.params["registrations"][1]["registerOptions"]["watchers"][0]["globPattern"],
        "**/*.langmeld"
    );
    send_notification(
        &client,
        "workspace/didChangeConfiguration",
        json!({ "settings": { "beforeRegistration": true } }),
    );
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            registration.id,
            Value::Null,
        )))
        .unwrap();

    let settings = json!({ "langmeld": { "strict": true } });
    send_notification(
        &client,
        "workspace/didChangeConfiguration",
        json!({ "settings": settings }),
    );
    send_notification(
        &client,
        "workspace/didChangeWatchedFiles",
        json!({
            "changes": [{ "uri": "file:///workspace/defs/main.langmeld", "type": 2 }]
        }),
    );
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");

    assert_eq!(*observed.configuration.lock().unwrap(), [settings]);
    let file_changes = observed.file_changes.lock().unwrap();
    assert_eq!(file_changes.len(), 1);
    assert_eq!(file_changes[0].len(), 1);
    assert_eq!(
        file_changes[0][0].uri.as_str(),
        "file:///workspace/defs/main.langmeld"
    );
    assert_eq!(file_changes[0][0].typ, lsp_types::FileChangeType::CHANGED);
}

#[test]
fn ignores_workspace_events_when_client_does_not_support_dynamic_registration() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = WorkspaceLifecycleBackend::default();
    let observed = backend.clone();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    send_notification(
        &client,
        "workspace/didChangeConfiguration",
        json!({ "settings": { "strict": true } }),
    );
    send_notification(
        &client,
        "workspace/didChangeWatchedFiles",
        json!({
            "changes": [{ "uri": "file:///workspace/defs/main.langmeld", "type": 2 }]
        }),
    );
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");

    assert!(observed.configuration.lock().unwrap().is_empty());
    assert!(observed.file_changes.lock().unwrap().is_empty());
}

#[test]
fn skips_relative_watcher_registration_when_client_does_not_support_it() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = WorkspaceLifecycleBackend {
        relative_watchers: true,
        ..WorkspaceLifecycleBackend::default()
    };
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "capabilities": {
                "workspace": {
                    "didChangeConfiguration": { "dynamicRegistration": true },
                    "didChangeWatchedFiles": {
                        "dynamicRegistration": true,
                        "relativePatternSupport": false
                    }
                }
            }
        }),
    );
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));

    let registration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected capability registration request, got {message:?}"),
    };
    assert_eq!(
        registration.params["registrations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        registration.params["registrations"][0]["method"],
        "workspace/didChangeConfiguration"
    );
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            registration.id,
            Value::Null,
        )))
        .unwrap();
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn refreshes_open_document_diagnostics_after_workspace_events() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = WorkspaceLifecycleBackend::default();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "capabilities": {
                "workspace": {
                    "workspaceFolders": true,
                    "didChangeConfiguration": { "dynamicRegistration": true },
                    "didChangeWatchedFiles": { "dynamicRegistration": true }
                }
            }
        }),
    );
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    let registration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected capability registration request, got {message:?}"),
    };
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            registration.id,
            Value::Null,
        )))
        .unwrap();

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///workspace/main.langmeld",
                "languageId": "test",
                "version": 1,
                "text": "program"
            }
        }),
    );
    assert_eq!(
        receive_diagnostics(&client)["diagnostics"][0]["code"],
        "REV-0"
    );

    send_notification(
        &client,
        "workspace/didChangeConfiguration",
        json!({ "settings": { "strict": true } }),
    );
    assert_eq!(
        receive_diagnostics(&client)["diagnostics"][0]["code"],
        "REV-1"
    );

    send_notification(
        &client,
        "workspace/didChangeWatchedFiles",
        json!({
            "changes": [{ "uri": "file:///workspace/defs/shared.langmeld", "type": 2 }]
        }),
    );
    assert_eq!(
        receive_diagnostics(&client)["diagnostics"][0]["code"],
        "REV-2"
    );

    send_notification(
        &client,
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [],
                "removed": [{ "uri": "file:///workspace", "name": "workspace" }]
            }
        }),
    );
    assert_eq!(
        receive_diagnostics(&client)["diagnostics"][0]["code"],
        "REV-3"
    );

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn serves_diagnostics_completion_navigation_and_full_document_changes() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, Backend::default());
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "capabilities": {
                "general": { "positionEncodings": ["utf-16", "utf-8"] },
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                }
            }
        }),
    );
    let initialized = receive_response(&client);
    assert_eq!(initialized["capabilities"]["positionEncoding"], "utf-16");
    assert!(initialized["capabilities"].get("workspace").is_none());
    assert!(initialized["capabilities"]["completionProvider"].is_object());
    assert_eq!(initialized["capabilities"]["definitionProvider"], true);
    assert_eq!(initialized["capabilities"]["referencesProvider"], true);
    assert_eq!(
        initialized["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"],
        json!(["variable"])
    );
    assert_eq!(
        initialized["capabilities"]["semanticTokensProvider"]["full"],
        true
    );
    assert_eq!(
        initialized["capabilities"]["documentFormattingProvider"],
        true
    );
    assert_eq!(initialized["capabilities"]["hoverProvider"], true);
    assert_eq!(initialized["capabilities"]["renameProvider"], true);
    assert_eq!(initialized["capabilities"]["documentSymbolProvider"], true);
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///main.test";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    let published = receive_diagnostics(&client);
    assert_eq!(published["diagnostics"][0]["code"], "TEST-001");
    assert_eq!(
        published["diagnostics"][0]["range"]["start"]["character"],
        0
    );
    assert_eq!(published["diagnostics"][0]["range"]["end"]["character"], 2);

    send_request(
        &client,
        2,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = receive_response(&client);
    assert_eq!(symbols[0]["name"], "root");
    assert_eq!(symbols[0]["range"]["end"]["character"], 5);
    assert_eq!(symbols[0]["selectionRange"]["end"]["character"], 2);
    assert_eq!(symbols[0]["children"][0]["name"], "bad");
    assert_eq!(symbols[0]["children"][0]["range"]["start"]["character"], 2);

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///invalid-symbol.test",
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        30,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///invalid-symbol.test" } }),
    );
    let symbols = receive_response(&client);
    assert_eq!(symbols.as_array().unwrap().len(), 2);
    assert_eq!(symbols[0]["name"], "valid");
    assert_eq!(symbols[1]["children"].as_array().unwrap().len(), 1);
    assert_eq!(symbols[1]["children"][0]["name"], "valid-child");

    send_request(
        &client,
        3,
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri } }),
    );
    let tokens = receive_response(&client);
    assert_eq!(tokens["data"], json!([0, 0, 2, 0, 1, 0, 2, 3, 0, 0]));

    send_request(
        &client,
        4,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );
    let edits = receive_response(&client);
    assert_eq!(
        edits[0]["range"]["start"],
        json!({ "line": 0, "character": 0 })
    );
    assert_eq!(
        edits[0]["range"]["end"],
        json!({ "line": 0, "character": 5 })
    );
    assert_eq!(edits[0]["newText"], "  😀bad");

    send_request(
        &client,
        5,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let hover = receive_response(&client);
    assert_eq!(hover["contents"]["kind"], "markdown");
    assert_eq!(hover["contents"]["value"], "**bad**");
    assert_eq!(hover["range"]["start"]["character"], 2);
    assert_eq!(hover["range"]["end"]["character"], 5);

    send_request(
        &client,
        6,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "renamed"
        }),
    );
    let rename = receive_response(&client);
    assert_eq!(rename["changes"][uri].as_array().unwrap().len(), 2);
    assert_eq!(rename["changes"][uri][0]["range"]["start"]["character"], 0);
    assert_eq!(rename["changes"][uri][0]["newText"], "renamed");
    assert_eq!(rename["changes"][uri][1]["range"]["start"]["character"], 2);

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///other.test",
                "languageId": "test",
                "version": 1,
                "text": "other"
            }
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        7,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "cross"
        }),
    );
    let cross_file_rename = receive_response(&client);
    assert_eq!(
        cross_file_rename["changes"][uri].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        cross_file_rename["changes"]["file:///other.test"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(cross_file_rename["changes"][uri][0]["newText"], "cross");
    assert_eq!(
        cross_file_rename["changes"]["file:///other.test"][0]["newText"],
        "cross"
    );

    send_request(
        &client,
        8,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "overlap"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        9,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "empty"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        10,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "missing"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        11,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let completion = receive_response(&client);
    assert_eq!(completion[0]["label"], "good");
    assert_eq!(completion[0]["textEdit"]["range"]["start"]["character"], 2);
    assert_eq!(completion[0]["textEdit"]["range"]["end"]["character"], 5);

    send_request(
        &client,
        12,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let definition = receive_response(&client);
    assert_eq!(definition[0]["uri"], uri);
    assert_eq!(definition[0]["range"]["end"]["character"], 2);

    send_request(
        &client,
        13,
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 1);

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "f😀\r\nx" }]
        }),
    );
    let changed = receive_diagnostics(&client);
    assert!(changed["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(changed["version"], 2);

    send_request(
        &client,
        14,
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri } }),
    );
    let tokens = receive_response(&client);
    assert_eq!(tokens["data"], json!([0, 0, 3, 0, 0, 1, 0, 1, 0, 0]));

    // Older document versions must not replace the current snapshot or publish
    // stale diagnostics.
    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 1 },
            "contentChanges": [{ "text": "bad" }]
        }),
    );

    send_notification(
        &client,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    let closed = receive_diagnostics(&client);
    assert!(closed["diagnostics"].as_array().unwrap().is_empty());

    send_request(&client, 15, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn declarative_backend_provides_scoped_completion_over_lsp() {
    let language = LanguageSpec::new(
        "Demo",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("value"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .expect("valid test grammar");
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, DeclarativeBackend::new(language));
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    let initialized = receive_response(&client);
    assert!(initialized["capabilities"]["completionProvider"].is_object());
    assert!(initialized["capabilities"]["definitionProvider"].is_null());
    assert!(initialized["capabilities"]["semanticTokensProvider"].is_null());
    assert!(initialized["capabilities"]["documentFormattingProvider"].is_null());
    assert!(initialized["capabilities"]["hoverProvider"].is_null());
    assert!(initialized["capabilities"]["renameProvider"].is_null());
    assert!(initialized["capabilities"]["documentSymbolProvider"].is_null());
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///declarative.langmeld";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "demo",
                "version": 1,
                "text": "program { pri"
            }
        }),
    );
    let diagnostics = receive_diagnostics(&client);
    assert_eq!(diagnostics["uri"], uri);

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 13 }
        }),
    );
    let completions = receive_response(&client);
    let labels = completions
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["print"]);

    send_request(&client, 3, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

struct DiagnosticParser;

impl LanguageParser for DiagnosticParser {
    type Tree = bool;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let diagnostics = source
            .text
            .contains("syntax")
            .then(|| Diagnostic::error("HOST-PARSE", "parser diagnostic", source, Span::new(0, 6)))
            .into_iter()
            .collect();
        ParsedDocument::new(source, true, diagnostics)
    }
}

struct DiagnosticAnalyzer;

impl LanguageAnalyzer<DiagnosticParser> for DiagnosticAnalyzer {
    type Analysis = usize;

    fn analyze(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<bool>,
    ) -> AnalysisDocument<Self::Analysis> {
        let diagnostics = source
            .text
            .contains("semantic")
            .then(|| {
                Diagnostic::error(
                    "HOST-SEMANTIC",
                    "semantic diagnostic",
                    source,
                    Span::new(7, source.text.len()),
                )
            })
            .into_iter()
            .collect();
        AnalysisDocument::new(source, source.text.len(), diagnostics)
    }
}

struct DiagnosticCompleter;

impl LanguageCompleter<DiagnosticParser> for DiagnosticCompleter {
    type Analysis = usize;

    fn complete(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<bool>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
    ) -> Vec<CompletionCandidate> {
        assert!(parsed.tree);
        assert_eq!(analysis.unwrap().result, source.text.len());
        vec![CompletionCandidate {
            text: "suggestion".into(),
            replace: Span::new(7, source.text.len()),
        }]
    }
}

struct DiagnosticNavigator;

impl LanguageNavigator<DiagnosticParser> for DiagnosticNavigator {
    type Analysis = usize;

    fn definition(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<bool>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        assert!(parsed.tree);
        assert_eq!(analysis.unwrap().result, source.text.len());
        vec![SourceLocation {
            source_name: source.name.clone(),
            identity: source.identity(),
            span: Span::new(7, source.text.len()),
        }]
    }

    fn references(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<bool>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        assert!(parsed.tree);
        assert_eq!(analysis.unwrap().result, source.text.len());
        include_declaration
            .then(|| SourceLocation {
                source_name: source.name.clone(),
                identity: source.identity(),
                span: Span::new(7, source.text.len()),
            })
            .into_iter()
            .collect()
    }
}

#[test]
fn host_language_backend_connects_parser_analysis_and_editor_queries() {
    let parser_only = HostLanguageBackend::new(DiagnosticParser);
    let parsed = parser_only.analyze(&SourceFile::new("file:///parser.test", "syntax"));
    assert_eq!(parsed.diagnostics[0].code, "HOST-PARSE");
    assert_eq!(parser_only.capabilities(), BackendCapabilities::default());

    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = HostLanguageBackend::new(DiagnosticParser)
        .with_analyzer(DiagnosticAnalyzer)
        .with_completer(DiagnosticCompleter)
        .with_navigator(DiagnosticNavigator);
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":"file:///host.test","languageId":"test","version":1,"text":"syntax semantic"}}),
    );
    let diagnostics = receive_diagnostics(&client);
    let codes = diagnostics["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["HOST-PARSE", "HOST-SEMANTIC"]);

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({"textDocument":{"uri":"file:///host.test"},"position":{"line":0,"character":10}}),
    );
    let completion = receive_response(&client);
    assert_eq!(completion[0]["label"], "suggestion");

    send_request(
        &client,
        3,
        "textDocument/definition",
        json!({"textDocument":{"uri":"file:///host.test"},"position":{"line":0,"character":10}}),
    );
    let definition = receive_response(&client);
    assert_eq!(definition[0]["uri"], "file:///host.test");

    send_request(
        &client,
        4,
        "textDocument/references",
        json!({"textDocument":{"uri":"file:///host.test"},"position":{"line":0,"character":10},"context":{"includeDeclaration":true}}),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 1);

    send_request(&client, 5, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

struct WorkspaceDiagnosticAnalyzer;

impl LanguageAnalyzer<DiagnosticParser> for WorkspaceDiagnosticAnalyzer {
    type Analysis = usize;

    fn analyze(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<bool>,
    ) -> AnalysisDocument<Self::Analysis> {
        AnalysisDocument::new(source, 1, Vec::new())
    }
}

impl LanguageWorkspaceAnalyzer<DiagnosticParser, WorkspaceDiagnosticAnalyzer>
    for WorkspaceDiagnosticAnalyzer
{
    fn analyze_workspace(
        &self,
        analyzer: &WorkspaceDiagnosticAnalyzer,
        documents: &[(&SourceFile, &ParsedDocument<bool>)],
    ) -> Vec<AnalysisDocument<usize>> {
        let count = documents.len();
        documents
            .iter()
            .map(|(source, parsed)| {
                let mut diagnostics = analyzer.analyze(source, parsed).diagnostics;
                diagnostics.push(Diagnostic::error(
                    format!("WORKSPACE-{count}"),
                    "workspace document count",
                    source,
                    Span::new(0, 0),
                ));
                AnalysisDocument::new(source, count, diagnostics)
            })
            .collect()
    }
}

#[test]
fn host_language_backend_analyzes_open_sources_together_when_configured() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = HostLanguageBackend::new(DiagnosticParser)
        .with_analyzer(WorkspaceDiagnosticAnalyzer)
        .with_workspace_analyzer(WorkspaceDiagnosticAnalyzer);
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///first.host",
                "languageId": "test",
                "version": 1,
                "text": "first"
            }
        }),
    );
    let first = receive_diagnostics(&client);
    assert_eq!(first["diagnostics"][0]["code"], "WORKSPACE-1");

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///second.host",
                "languageId": "test",
                "version": 1,
                "text": "second"
            }
        }),
    );
    let refreshed = [receive_diagnostics(&client), receive_diagnostics(&client)];
    assert!(
        refreshed
            .iter()
            .all(|diagnostics| { diagnostics["diagnostics"][0]["code"] == "WORKSPACE-2" })
    );

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[derive(Clone, Default)]
struct InvalidWorkspaceBackend(Arc<AtomicUsize>);

impl LanguageBackend for InvalidWorkspaceBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let count = self.0.load(Ordering::Relaxed);
        BackendDocument::new(
            source,
            (),
            vec![Diagnostic::error(
                format!("OPEN-{count}"),
                "workspace fallback analysis",
                source,
                Span::new(0, 0),
            )],
        )
    }

    fn analyze_workspace(
        &self,
        sources: &[SourceFile],
    ) -> Option<Vec<BackendDocument<Self::Document>>> {
        self.0.store(sources.len(), Ordering::Relaxed);
        Some(
            sources
                .iter()
                .map(|_| BackendDocument {
                    source: None,
                    state: (),
                    diagnostics: Vec::new(),
                })
                .collect(),
        )
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}

#[test]
fn invalid_workspace_results_reanalyze_every_open_document() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, InvalidWorkspaceBackend::default());
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":"file:///first.test","languageId":"test","version":1,"text":"first"}}),
    );
    let first = receive_diagnostics(&client);
    assert_eq!(first["uri"], "file:///first.test");
    assert_eq!(first["diagnostics"][0]["code"], "OPEN-1");

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":"file:///second.test","languageId":"test","version":1,"text":"second"}}),
    );
    let refreshed = [receive_diagnostics(&client), receive_diagnostics(&client)];
    let first_after_refresh = refreshed
        .iter()
        .find(|diagnostics| diagnostics["uri"] == "file:///first.test")
        .expect("fallback refreshes the previously open source");
    let second_after_refresh = refreshed
        .iter()
        .find(|diagnostics| diagnostics["uri"] == "file:///second.test")
        .expect("fallback analyzes the newly opened source");
    assert_eq!(first_after_refresh["diagnostics"][0]["code"], "OPEN-2");
    assert_eq!(second_after_refresh["diagnostics"][0]["code"], "OPEN-2");

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[derive(Clone)]
struct CloseBackend(Arc<Mutex<Option<(String, String)>>>);

impl LanguageBackend for CloseBackend {
    type Document = String;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        BackendDocument::new(source, source.text.clone(), Vec::new())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn did_close(&self, source: &SourceFile, document: &BackendDocument<Self::Document>) {
        *self.0.lock().unwrap() = Some((source.text.clone(), document.state.clone()));
    }
}

#[test]
fn backend_receives_the_last_snapshot_when_a_document_closes() {
    let closed = Arc::new(Mutex::new(None));
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = CloseBackend(Arc::clone(&closed));
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    let uri = "file:///closed.test";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "last open snapshot"
            }
        }),
    );
    receive_diagnostics(&client);
    send_notification(
        &client,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    let cleared = receive_diagnostics(&client);
    assert!(cleared["diagnostics"].as_array().unwrap().is_empty());

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
    assert_eq!(
        *closed.lock().unwrap(),
        Some(("last open snapshot".into(), "last open snapshot".into()))
    );
}

struct InvalidLegendBackend;

impl LanguageBackend for InvalidLegendBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        BackendDocument::new(source, (), Vec::new())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            semantic_tokens: true,
            ..BackendCapabilities::default()
        }
    }

    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        SemanticTokenLegend::new(["variable", "variable"], [] as [&str; 0])
    }
}

#[test]
fn rejects_invalid_semantic_token_legends_during_initialization() {
    let (server_connection, client) = Connection::memory();
    let server_thread =
        thread::spawn(move || run_connection(server_connection, InvalidLegendBackend));
    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    let error = server_thread
        .join()
        .expect("server thread should finish")
        .expect_err("duplicate legend entries should prevent initialization");
    assert!(error.to_string().contains("declared more than once"));
}

#[test]
fn does_not_return_markdown_hover_when_client_did_not_advertise_it() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, Backend::default());
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));
    let uri = "file:///plaintext-only.test";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        2,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    assert!(receive_response(&client).is_null());
    send_request(&client, 3, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
