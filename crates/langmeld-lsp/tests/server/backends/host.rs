use super::*;
use serde_json::json;

struct DiagnosticAnalyzer;

impl LanguageAnalyzer<DiagnosticParser> for DiagnosticAnalyzer {
    type Analysis = usize;

    fn analyze(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<bool>,
    ) -> AnalysisDocument<Self::Analysis> {
        let diagnostics = source
            .text()
            .contains("semantic")
            .then(|| {
                Diagnostic::error(
                    "HOST-SEMANTIC",
                    "semantic diagnostic",
                    source,
                    Span::new(7, source.text().len()),
                )
            })
            .into_iter()
            .collect();
        AnalysisDocument::new(source, source.text().len(), diagnostics)
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
        assert!(*parsed.tree());
        assert_eq!(*analysis.unwrap().result(), source.text().len());
        vec![CompletionCandidate::new(
            "suggestion",
            Span::new(7, source.text().len()),
        )]
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
        assert!(*parsed.tree());
        assert_eq!(*analysis.unwrap().result(), source.text().len());
        vec![SourceLocation::new(
            source,
            Span::new(7, source.text().len()),
        )]
    }

    fn references(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<bool>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        assert!(*parsed.tree());
        assert_eq!(*analysis.unwrap().result(), source.text().len());
        include_declaration
            .then(|| SourceLocation::new(source, Span::new(7, source.text().len())))
            .into_iter()
            .collect()
    }
}

struct ExternalLocationNavigator(SourceLocation);

impl LanguageNavigator<DiagnosticParser> for ExternalLocationNavigator {
    type Analysis = usize;

    fn definition(
        &self,
        _source: &SourceFile,
        _parsed: &ParsedDocument<bool>,
        _analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        vec![self.0.clone()]
    }

    fn references(
        &self,
        _source: &SourceFile,
        _parsed: &ParsedDocument<bool>,
        _analysis: Option<&AnalysisDocument<Self::Analysis>>,
        _offset: usize,
        _include_declaration: bool,
    ) -> Vec<SourceLocation> {
        vec![self.0.clone()]
    }
}

#[test]
fn host_language_backend_connects_parser_analysis_and_editor_queries() {
    let parser_only = HostLanguageBackend::new(DiagnosticParser);
    let parsed = parser_only.analyze(&SourceFile::new("file:///parser.test", "syntax"));
    assert_eq!(parsed.diagnostics()[0].code, "HOST-PARSE");
    assert_eq!(parser_only.capabilities(), BackendCapabilities::default());

    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let backend = HostLanguageBackend::new_with_analyzer(DiagnosticParser, DiagnosticAnalyzer)
            .with_completer(DiagnosticCompleter)
            .with_navigator(DiagnosticNavigator);
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

#[test]
fn language_definition_backend_exposes_declaration_outline_symbols() {
    let source = SourceFile::new(
        "file:///language.langmeld",
        "language \"Widget\" {\n    root \"program\";\n    import \"constructs/shared.langmeld\";\n    construct \"program\" {\n        syntax { literal \"run\"; }\n    }\n}",
    );
    let backend = LanguageDefinitionBackend;
    let document = backend.analyze(&source);

    assert!(backend.capabilities().document_symbols);
    let symbols = backend.document_symbols(&source, &document);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Widget");
    assert_eq!(symbols[0].kind, DocumentSymbolKind::Module);
    assert_eq!(
        &source.text()[symbols[0].selection_range.start..symbols[0].selection_range.end],
        "\"Widget\""
    );
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "constructs/shared.langmeld");
    assert_eq!(symbols[0].children[0].kind, DocumentSymbolKind::File);
    assert_eq!(symbols[0].children[1].name, "program");
    assert_eq!(symbols[0].children[1].kind, DocumentSymbolKind::Struct);
    assert_eq!(
        &source.text()[symbols[0].children[1].selection_range.start
            ..symbols[0].children[1].selection_range.end],
        "\"program\""
    );

    let module = SourceFile::new(
        "file:///constructs/shared.langmeld",
        r#"construct "draw \"line\"" { syntax { literal "draw"; } }"#,
    );
    let module_document = backend.analyze(&module);
    let module_symbols = backend.document_symbols(&module, &module_document);
    assert_eq!(module_symbols.len(), 1);
    assert_eq!(module_symbols[0].name, "draw \"line\"");
    assert_eq!(module_symbols[0].kind, DocumentSymbolKind::Struct);
}

#[test]
fn host_backend_supports_closed_file_navigation_symbols_and_close_hooks() {
    let mut sources = SourceDatabase::default();
    let target_id = sources
        .insert("closed.host", "declaration")
        .expect("closed source should be registered");
    let target = Arc::new(
        sources
            .get(target_id)
            .expect("closed source should exist")
            .clone(),
    );
    let target_location = SourceLocation::new(&target, Span::new(0, target.text().len()));
    let uri: Uri = "file:///closed.host"
        .parse()
        .expect("test URI should be valid");
    let location_for_resolver = target_location.clone();
    let location_for_symbols = target_location.clone();
    let closed_document = Arc::new(Mutex::new(None));
    let closed_document_handler = Arc::clone(&closed_document);
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let backend = HostLanguageBackend::new_with_analyzer(DiagnosticParser, DiagnosticAnalyzer)
            .with_navigator(ExternalLocationNavigator(target_location))
            .with_source_resolver(move |location| {
                (location == &location_for_resolver)
                    .then(|| BackendSource::new(uri.clone(), Arc::clone(&target)))
            })
            .with_workspace_symbol_provider(move |query| {
                if !"ClosedDeclaration"
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                {
                    return Vec::new();
                }
                vec![WorkspaceSymbolInfo::new(
                    "ClosedDeclaration",
                    DocumentSymbolKind::Struct,
                    location_for_symbols.clone(),
                )]
            })
            .with_document_close_handler(move |source| {
                *closed_document_handler.lock().unwrap() =
                    Some((source.name().to_owned(), source.text().to_string()));
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
                "text": "syntax semantic"
            }
        }),
    );
    receive_diagnostics(&client);
    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": "file:///open.host", "version": 2 },
            "contentChanges": [{ "text": "syntax semantic latest" }]
        }),
    );
    receive_diagnostics(&client);

    send_request(
        &client,
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": "file:///open.host" },
            "position": { "line": 0, "character": 10 }
        }),
    );
    let definition = receive_response(&client);
    assert_eq!(definition[0]["uri"], "file:///closed.host");
    assert_eq!(definition[0]["range"]["end"]["character"], 11);

    send_request(
        &client,
        3,
        "textDocument/references",
        json!({
            "textDocument": { "uri": "file:///open.host" },
            "position": { "line": 0, "character": 10 },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 1);
    assert_eq!(references[0]["uri"], "file:///closed.host");

    send_request(
        &client,
        4,
        "workspace/symbol",
        json!({ "query": "ClosedDecl" }),
    );
    let symbols = receive_response(&client);
    assert_eq!(symbols[0]["name"], "ClosedDeclaration");
    assert_eq!(symbols[0]["location"]["uri"], "file:///closed.host");

    send_notification(
        &client,
        "textDocument/didClose",
        json!({"textDocument":{"uri":"file:///open.host"}}),
    );
    receive_diagnostics(&client);
    send_request(&client, 5, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    assert_eq!(
        *closed_document.lock().unwrap(),
        Some((
            "file:///open.host".to_owned(),
            "syntax semantic latest".to_owned()
        ))
    );
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
