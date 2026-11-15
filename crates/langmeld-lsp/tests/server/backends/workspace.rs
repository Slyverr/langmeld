use super::*;
use serde_json::json;

#[test]
fn host_workspace_diagnostics_publish_closed_sources_and_clear_stale_findings() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let backend =
            HostLanguageBackend::new(DiagnosticParser).with_workspace_diagnostics(|open_sources| {
                if open_sources
                    .iter()
                    .any(|source| source.text() == "unavailable")
                {
                    return None;
                }

                if open_sources.iter().any(|source| source.text() == "clear") {
                    return Some(Vec::new());
                }

                let closed_source = SourceFile::new("file:///closed.host", "closed");
                let closed_uri = "file:///closed.host"
                    .parse()
                    .expect("closed source URI should be valid");
                let mut reports = vec![BackendWorkspaceDiagnostics::new(
                    BackendSource::new(closed_uri, closed_source.clone()),
                    vec![Diagnostic::error(
                        "CLOSED-1",
                        "closed source finding",
                        &closed_source,
                        Span::new(0, closed_source.text().len()),
                    )],
                )];

                reports.extend(open_sources.iter().map(|source| {
                    let uri = source
                        .name()
                        .parse()
                        .expect("open source name should be a URI");
                    BackendWorkspaceDiagnostics::new(
                        BackendSource::new(uri, source.clone()),
                        vec![Diagnostic::error(
                            "OPEN-OVERWRITE",
                            "must not replace versioned document findings",
                            source,
                            Span::new(0, source.text().len()),
                        )],
                    )
                }));

                Some(reports)
            });
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));

    let initial_closed_diagnostics = receive_diagnostics(&client);
    assert_eq!(initial_closed_diagnostics["uri"], "file:///closed.host");
    assert_eq!(
        initial_closed_diagnostics["diagnostics"][0]["code"],
        "CLOSED-1"
    );

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///open.host",
                "languageId": "test",
                "version": 1,
                "text": "syntax"
            }
        }),
    );
    let open_diagnostics = receive_diagnostics(&client);
    assert_eq!(open_diagnostics["uri"], "file:///open.host");
    assert_eq!(open_diagnostics["version"], 1);
    assert_eq!(open_diagnostics["diagnostics"][0]["code"], "HOST-PARSE");
    assert_eq!(open_diagnostics["diagnostics"].as_array().unwrap().len(), 1);

    let closed_diagnostics = receive_diagnostics(&client);
    assert_eq!(closed_diagnostics["uri"], "file:///closed.host");
    assert_eq!(closed_diagnostics["diagnostics"][0]["code"], "CLOSED-1");
    assert!(client.receiver.try_recv().is_err());

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": "file:///open.host", "version": 2},
            "contentChanges": [{"text": "unavailable"}]
        }),
    );
    assert_eq!(receive_diagnostics(&client)["version"], 2);
    assert!(client.receiver.try_recv().is_err());

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": "file:///open.host", "version": 3},
            "contentChanges": [{"text": "clear"}]
        }),
    );
    let open_update = receive_diagnostics(&client);
    assert_eq!(open_update["uri"], "file:///open.host");
    assert_eq!(open_update["version"], 3);
    assert!(open_update["diagnostics"].as_array().unwrap().is_empty());

    let closed_clear = receive_diagnostics(&client);
    assert_eq!(closed_clear["uri"], "file:///closed.host");
    assert!(closed_clear["diagnostics"].as_array().unwrap().is_empty());

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": "file:///open.host", "version": 4},
            "contentChanges": [{"text": "syntax again"}]
        }),
    );
    assert_eq!(receive_diagnostics(&client)["version"], 4);
    assert_eq!(receive_diagnostics(&client)["uri"], "file:///closed.host");

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///closed.host",
                "languageId": "test",
                "version": 1,
                "text": "syntax"
            }
        }),
    );
    let opened_closed_source = receive_diagnostics(&client);
    assert_eq!(opened_closed_source["uri"], "file:///closed.host");
    assert_eq!(opened_closed_source["version"], 1);
    assert_eq!(opened_closed_source["diagnostics"][0]["code"], "HOST-PARSE");
    assert!(client.receiver.try_recv().is_err());

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
fn workspace_diagnostic_reports_reject_conflicting_snapshots_and_stale_locations() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let backend =
            HostLanguageBackend::new(DiagnosticParser).with_workspace_diagnostics(|open_sources| {
                if open_sources.is_empty() {
                    return None;
                }

                let duplicate_uri: Uri = "file:///duplicate.host"
                    .parse()
                    .expect("duplicate source URI should be valid");
                let first_source = SourceFile::new("file:///duplicate.host", "first");
                let second_source = SourceFile::new("file:///duplicate.host", "second");
                let current_source = SourceFile::new("file:///invalid.host", "current");
                let stale_source = SourceFile::new("file:///invalid.host", "stale");

                Some(vec![
                    BackendWorkspaceDiagnostics::new(
                        BackendSource::new(duplicate_uri.clone(), first_source.clone()),
                        vec![Diagnostic::error(
                            "DUPLICATE-ONE",
                            "first snapshot",
                            &first_source,
                            Span::new(0, first_source.text().len()),
                        )],
                    ),
                    BackendWorkspaceDiagnostics::new(
                        BackendSource::new(duplicate_uri, second_source.clone()),
                        vec![Diagnostic::error(
                            "DUPLICATE-TWO",
                            "second snapshot",
                            &second_source,
                            Span::new(0, second_source.text().len()),
                        )],
                    ),
                    BackendWorkspaceDiagnostics::new(
                        BackendSource::new(
                            "file:///invalid.host"
                                .parse()
                                .expect("invalid source URI should be valid"),
                            current_source,
                        ),
                        vec![Diagnostic::error(
                            "STALE-DIAGNOSTIC",
                            "finding belongs to an obsolete source snapshot",
                            &stale_source,
                            Span::new(0, stale_source.text().len()),
                        )],
                    ),
                ])
            });
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
                "uri": "file:///open.host",
                "languageId": "test",
                "version": 1,
                "text": "valid"
            }
        }),
    );

    assert_eq!(receive_diagnostics(&client)["uri"], "file:///open.host");

    let duplicate = receive_diagnostics(&client);
    assert_eq!(duplicate["uri"], "file:///duplicate.host");
    assert_eq!(duplicate["diagnostics"][0]["code"], "LM-LSP-004");
    assert_eq!(duplicate["diagnostics"].as_array().unwrap().len(), 1);

    let invalid = receive_diagnostics(&client);
    assert_eq!(invalid["uri"], "file:///invalid.host");
    assert_eq!(invalid["diagnostics"][0]["code"], "LM-LSP-003");
    assert_eq!(invalid["diagnostics"].as_array().unwrap().len(), 1);

    send_request(&client, 2, "shutdown", Value::Null);
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
    let server_thread = thread::spawn(move || {
        let backend =
            HostLanguageBackend::new_with_analyzer(DiagnosticParser, WorkspaceDiagnosticAnalyzer)
                .with_workspace_analyzer(WorkspaceDiagnosticAnalyzer);
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
    assert!(
        first["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "LM-LSP-001")
    );

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
    assert!(refreshed.iter().all(|diagnostics| {
        diagnostics["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "LM-LSP-001")
    }));

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

struct MismatchedFallbackBackend;

impl LanguageBackend for MismatchedFallbackBackend {
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

    fn analyze_workspace(
        &self,
        _sources: &[SourceFile],
    ) -> Option<Vec<BackendDocument<Self::Document>>> {
        Some(Vec::new())
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
fn invalid_workspace_fallback_preserves_mismatched_analysis_identity() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, MismatchedFallbackBackend);
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
                "uri": "file:///workspace-fallback.test",
                "languageId": "test",
                "version": 1,
                "text": "current"
            }
        }),
    );

    let diagnostics = receive_diagnostics(&client);
    let diagnostics = diagnostics["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "LM-LSP-001")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "LM-LSP-002")
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic["code"] != "STALE-DIAGNOSTIC")
    );

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///workspace-fallback.test" },
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
