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

use langmeld_core::{Diagnostic, SourceDatabase, SourceFile, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, CompletionInsertTextFormat, CompletionKind,
    ConstructSpec, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageSpec, LanguageWorkspaceAnalyzer, ParsedDocument, PatternPart,
};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, BackendSource, BackendWorkspace,
    BackendWorkspaceCapabilities, DeclarativeBackend, DocumentSymbolInfo, DocumentSymbolKind,
    HostLanguageBackend, HoverInfo, LanguageBackend, LanguageDefinitionBackend, SemanticToken,
    SemanticTokenLegend, WorkspaceSymbolInfo, run_connection,
};
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::Uri;
use serde_json::Value;

#[derive(Default)]
struct Backend(Arc<Mutex<HashMap<String, SourceFile>>>);

impl LanguageBackend for Backend {
    type Document = String;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        self.0
            .lock()
            .unwrap()
            .insert(source.name().to_owned(), source.clone());
        let diagnostics = if source.text().contains("bad") {
            vec![Diagnostic::error(
                "TEST-001",
                "replace this token",
                source,
                Span::new(0, "😀".len()),
            )]
        } else {
            Vec::new()
        };
        BackendDocument::new(source, source.text().to_owned(), diagnostics)
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
            workspace_symbols: true,
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<String>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        assert_eq!(document.state(), source.text());
        assert_eq!(offset, "😀".len());
        vec![
            CompletionCandidate::new("good", Span::new("😀".len(), source.text().len()))
                .with_label("Good value")
                .with_detail("Suggested item"),
            CompletionCandidate::new("great", Span::new("😀".len(), source.text().len()))
                .with_label("Great value")
                .with_kind(CompletionKind::Struct)
                .with_filter_text("preferred"),
            CompletionCandidate::new("template", Span::new("😀".len(), source.text().len()))
                .with_label("Snippet category")
                .with_kind(CompletionKind::Snippet),
            CompletionCandidate::new(
                "call(${1:argument})$0",
                Span::new("😀".len(), source.text().len()),
            )
            .with_label("Function call")
            .with_kind(CompletionKind::Function)
            .with_insert_text_format(CompletionInsertTextFormat::Snippet),
        ]
    }

    fn definition(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        _offset: usize,
    ) -> Vec<SourceLocation> {
        vec![SourceLocation::new(source, Span::new(0, "😀".len()))]
    }

    fn references(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
        _offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        include_declaration
            .then(|| SourceLocation::new(source, Span::new(0, "😀".len())))
            .into_iter()
            .collect()
    }

    fn source_for_location(&self, location: &SourceLocation) -> Option<BackendSource> {
        if location.source_name() != "file:///closed.test" {
            return None;
        }
        let source = self.0.lock().unwrap().get(location.source_name())?.clone();
        if source.identity() != location.source_identity() {
            return None;
        }
        let uri = location.source_name().parse::<Uri>().ok()?;
        Some(BackendSource::new(uri, Arc::new(source)))
    }

    fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbolInfo> {
        if !"Record".to_lowercase().contains(&query.to_lowercase()) {
            return Vec::new();
        }
        let sources = self.0.lock().unwrap();
        let Some(source) = sources.get("file:///closed.test") else {
            return Vec::new();
        };
        vec![WorkspaceSymbolInfo::new(
            "Record",
            DocumentSymbolKind::Struct,
            SourceLocation::new(source, Span::new(0, source.text().len())),
        )]
    }

    fn semantic_token_legend(&self) -> SemanticTokenLegend {
        SemanticTokenLegend::new(["variable"], ["declaration"])
    }

    fn semantic_tokens(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
    ) -> Vec<SemanticToken> {
        if source.text().contains('\n') {
            return vec![SemanticToken::new(
                Span::new(0, source.text().len()),
                "variable",
            )];
        }
        vec![
            SemanticToken::new(Span::new(0, "😀".len()), "variable").with_modifier("declaration"),
            SemanticToken::new(Span::new("😀".len(), "😀bad".len()), "variable"),
            SemanticToken::new(Span::new(2, 5), "variable"),
            SemanticToken::new(Span::new(4, 7), "unknown"),
            SemanticToken::new(Span::new(7, 2), "variable"),
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
        Some(format!("{indentation}{}", source.text()))
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
                    .map(|(source, span)| SourceLocation::new(source, span))
                    .collect(),
            );
        }
        if new_name == "closed" {
            let other = self.0.lock().unwrap().get("file:///closed.test").cloned()?;
            return Some(
                [
                    (source, Span::new(4, 7)),
                    (&other, Span::new(0, other.text().len())),
                ]
                .into_iter()
                .map(|(source, span)| SourceLocation::new(source, span))
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
            .map(|span| SourceLocation::new(source, span))
            .collect::<Vec<_>>();
        if new_name == "missing" {
            let missing_source = SourceFile::new("file:///closed.test", "");
            locations.push(SourceLocation::new(&missing_source, Span::new(0, 4)));
        }
        Some(locations)
    }

    fn document_symbols(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<String>,
    ) -> Vec<DocumentSymbolInfo> {
        if source.name() == "file:///invalid-symbol.test" {
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

#[path = "server/backends.rs"]
mod backends;
#[path = "server/documents.rs"]
mod documents;
#[path = "server/lifecycle.rs"]
mod lifecycle;
#[path = "server/workspace.rs"]
mod workspace;
