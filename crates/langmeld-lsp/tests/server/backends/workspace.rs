use super::*;
use serde_json::json;

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
                let mut diagnostics = analyzer.analyze(source, parsed).diagnostics().to_vec();
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
    let backend =
        HostLanguageBackend::new_with_analyzer(DiagnosticParser, WorkspaceDiagnosticAnalyzer)
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
                .map(|source| {
                    let standalone_source = SourceFile::new(source.name(), source.text());

                    BackendDocument::new(&standalone_source, (), Vec::new())
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
