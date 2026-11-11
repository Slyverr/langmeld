use super::*;
use serde_json::json;

#[derive(Clone)]
struct CloseBackend(Arc<Mutex<Option<(String, String)>>>);

impl LanguageBackend for CloseBackend {
    type Document = String;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        BackendDocument::new(source, source.text().to_owned(), Vec::new())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn did_close(&self, source: &SourceFile, document: &BackendDocument<Self::Document>) {
        *self.0.lock().unwrap() = Some((source.text().to_owned(), document.state().clone()));
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
