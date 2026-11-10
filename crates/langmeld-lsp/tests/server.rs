use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use langmeld_core::{Diagnostic, SourceFile, SourceLocation, Span};
use langmeld_language::{CompletionCandidate, ConstructSpec, LanguageSpec, PatternPart};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, DeclarativeBackend, LanguageBackend, run_connection,
};
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

struct Backend;

impl LanguageBackend for Backend {
    type Document = String;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
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

#[test]
fn serves_diagnostics_completion_navigation_and_full_document_changes() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, Backend);
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
                "general": { "positionEncodings": ["utf-16", "utf-8"] }
            }
        }),
    );
    let initialized = receive_response(&client);
    assert_eq!(initialized["capabilities"]["positionEncoding"], "utf-16");
    assert!(initialized["capabilities"]["completionProvider"].is_object());
    assert_eq!(initialized["capabilities"]["definitionProvider"], true);
    assert_eq!(initialized["capabilities"]["referencesProvider"], true);
    assert!(initialized["capabilities"]["hoverProvider"].is_null());
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
        3,
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
        4,
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
            "contentChanges": [{ "text": "fixed" }]
        }),
    );
    let changed = receive_diagnostics(&client);
    assert!(changed["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(changed["version"], 2);

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

    send_request(&client, 5, "shutdown", Value::Null);
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
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///declarative.lm";
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
