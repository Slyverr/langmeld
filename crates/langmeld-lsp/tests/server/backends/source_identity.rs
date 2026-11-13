use super::*;
use serde_json::json;

struct MismatchedDocumentBackend;

impl LanguageBackend for MismatchedDocumentBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let stale_source = SourceFile::new(source.name(), source.text());
        let stale_diagnostic = Diagnostic::error(
            "STALE-DIAGNOSTIC",
            "diagnostic for an obsolete snapshot",
            &stale_source,
            Span::new(0, stale_source.text().len()),
        );

        BackendDocument::new(&stale_source, (), vec![stale_diagnostic])
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _offset: usize,
    ) -> Vec<CompletionCandidate> {
        vec![CompletionCandidate::new("stale", Span::new(0, 0))]
    }
}

#[test]
fn mismatched_single_document_results_are_diagnosed_and_editor_queries_suppressed() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, MismatchedDocumentBackend);
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
        json!({
            "textDocument": {
                "uri": "file:///stale.test",
                "languageId": "test",
                "version": 1,
                "text": "current"
            }
        }),
    );

    let diagnostics = receive_diagnostics(&client);
    assert_eq!(diagnostics["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(diagnostics["diagnostics"][0]["code"], "LM-LSP-002");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///stale.test" },
            "position": { "line": 0, "character": 0 }
        }),
    );
    assert_eq!(receive_response(&client), json!([]));

    send_request(&client, 3, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
