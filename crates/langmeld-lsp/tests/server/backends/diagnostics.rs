use super::*;
use serde_json::json;

struct MismatchedDiagnosticBackend;

impl LanguageBackend for MismatchedDiagnosticBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let stale_source = SourceFile::new(source.name(), source.text());
        let diagnostics = vec![
            Diagnostic::warning("CURRENT", "current diagnostic", source, Span::new(0, 1)),
            Diagnostic::error(
                "STALE-DIAGNOSTIC",
                "diagnostic for an obsolete snapshot",
                &stale_source,
                Span::new(0, stale_source.text().len()),
            ),
            Diagnostic::error(
                "INVALID-RANGE",
                "diagnostic beyond the end of the source",
                source,
                Span::new(0, source.text().len() + 1),
            ),
        ];

        BackendDocument::new(source, (), diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}

#[test]
fn diagnostics_with_invalid_locations_are_suppressed_and_reported() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, MismatchedDiagnosticBackend);
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
                "uri": "file:///diagnostics.test",
                "languageId": "test",
                "version": 1,
                "text": "current"
            }
        }),
    );

    let diagnostics = receive_diagnostics(&client);
    let diagnostics = diagnostics["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0]["code"], "CURRENT");
    assert_eq!(diagnostics[1]["code"], "LM-LSP-003");

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
