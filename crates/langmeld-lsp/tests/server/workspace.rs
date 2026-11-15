use super::*;
use serde_json::json;

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
            "rootPath": "/workspace",
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
    assert_eq!(context.root_path, Some("/workspace".into()));
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
    folders: Arc<Mutex<Vec<lsp_types::WorkspaceFolder>>>,
    relative_watchers: bool,
    workspace_revision: Arc<AtomicUsize>,
}

impl LanguageBackend for WorkspaceLifecycleBackend {
    type Document = ();

    fn initialize_workspace(&self, workspace: &BackendWorkspace) {
        *self.folders.lock().unwrap() = workspace.workspace_folders.clone().unwrap_or_default();
    }

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

    fn did_change_workspace_folders(&self, event: &lsp_types::WorkspaceFoldersChangeEvent) {
        let mut folders = self.folders.lock().unwrap();
        folders.retain(|folder| {
            event
                .removed
                .iter()
                .all(|removed| removed.uri != folder.uri)
        });

        for folder in &event.added {
            if !folders.iter().any(|current| current.uri == folder.uri) {
                folders.push(folder.clone());
            }
        }

        self.workspace_revision.fetch_add(1, Ordering::Relaxed);
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn workspace_capabilities(&self) -> BackendWorkspaceCapabilities {
        let watched_files = if self.relative_watchers {
            let mut folders = self.folders.lock().unwrap().clone();
            if folders.is_empty() {
                folders.push(lsp_types::WorkspaceFolder {
                    uri: "file:///workspace".parse().unwrap(),
                    name: "workspace".into(),
                });
            }

            folders
                .into_iter()
                .map(|folder| lsp_types::FileSystemWatcher {
                    glob_pattern: lsp_types::GlobPattern::Relative(lsp_types::RelativePattern {
                        base_uri: lsp_types::OneOf::Left(folder),
                        pattern: "**/*.langmeld".into(),
                    }),
                    kind: Some(
                        lsp_types::WatchKind::Create
                            | lsp_types::WatchKind::Change
                            | lsp_types::WatchKind::Delete,
                    ),
                })
                .collect()
        } else {
            vec![lsp_types::FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::String("**/*.langmeld".into()),
                kind: Some(
                    lsp_types::WatchKind::Create
                        | lsp_types::WatchKind::Change
                        | lsp_types::WatchKind::Delete,
                ),
            }]
        };

        BackendWorkspaceCapabilities {
            workspace_folders: true,
            configuration_changes: true,
            watched_files,
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
fn updates_relative_file_watcher_registration_after_workspace_folder_changes() {
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
            "workspaceFolders": [
                { "uri": "file:///workspace", "name": "workspace" }
            ],
            "capabilities": {
                "workspace": {
                    "workspaceFolders": true,
                    "didChangeWatchedFiles": {
                        "dynamicRegistration": true,
                        "relativePatternSupport": true
                    }
                }
            }
        }),
    );
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));

    let initial_registration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected watcher registration request, got {message:?}"),
    };
    assert_eq!(initial_registration.method, "client/registerCapability");
    let initial_watcher_registration_id = initial_registration.params["registrations"][0]["id"]
        .as_str()
        .expect("watcher registration should have an id")
        .to_owned();
    assert_eq!(
        initial_registration.params["registrations"][0]["registerOptions"]["watchers"][0]["globPattern"]
            ["baseUri"]["uri"],
        "file:///workspace"
    );
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            initial_registration.id,
            Value::Null,
        )))
        .unwrap();

    send_notification(
        &client,
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [
                    { "uri": "file:///replacement", "name": "replacement" }
                ],
                "removed": [
                    { "uri": "file:///workspace", "name": "workspace" }
                ]
            }
        }),
    );

    let unregistration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected watcher unregistration request, got {message:?}"),
    };
    assert_eq!(unregistration.method, "client/unregisterCapability");
    assert_eq!(
        unregistration.params["unregisterations"][0]["id"],
        initial_watcher_registration_id
    );

    send_notification(
        &client,
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [
                    { "uri": "file:///latest", "name": "latest" }
                ],
                "removed": [
                    { "uri": "file:///replacement", "name": "replacement" }
                ]
            }
        }),
    );
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            unregistration.id,
            Value::Null,
        )))
        .unwrap();

    let replacement_registration = match receive(&client) {
        Message::Request(request) => request,
        message => panic!("expected replacement watcher registration, got {message:?}"),
    };
    assert_eq!(replacement_registration.method, "client/registerCapability");
    assert_eq!(
        replacement_registration.params["registrations"][0]["registerOptions"]["watchers"][0]["globPattern"]
            ["baseUri"]["uri"],
        "file:///latest"
    );
    client
        .sender
        .send(Message::Response(lsp_server::Response::new_ok(
            replacement_registration.id,
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
