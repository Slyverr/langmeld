//! A small LSP transport and diagnostic server for declarative Langmeld languages.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use langmeld_core::{BehaviorStage, SourceFile, ValidatedLanguageSpec};
#[cfg(test)]
use langmeld_language::CompletionPart;
use langmeld_language::{
    AstNode, BehaviorRegistrationError, BehaviorRegistrationIssue, ConfiguredLanguageParser,
    ConfiguredSemanticHook, ConfiguredSemanticHookRegistration, DefinitionLimits, FormatOptions,
    LANGUAGE_DEFINITION_EXTENSION, ParseLimits, ParseResult, PreparedLanguage, Program,
    ProjectSnapshot, SemanticHook, SemanticHookRegistration, WorkspaceAnalysis,
    WorkspaceSymbolResolverRegistration, WorkspaceSymbolResolverRegistry,
    WorkspaceTypeAnalysisRegistration, WorkspaceTypeAnalysisRegistry,
    analyze_workspace_with_host_callbacks, langmeld_definition_language, load_project,
    parse_language_definition_project_with_overlays_and_diagnostics,
};
use serde_json::{Value, json};

use completion::CompletionProvider;
pub use completion::{
    CompletionCandidate, CompletionContext, CompletionDocument, CompletionInsertTextFormat,
    CompletionItemKind, CompletionProviderRegistration,
};
#[cfg(test)]
use completion::{
    completion_continuation_snippet, construct_pattern_snippet, keyword_completion_items,
};
use completion::{field_kind_has_enum, field_kind_has_reference};

mod completion;
mod formatting;
mod hover;
mod navigation;
mod protocol;
mod semantic_tokens;

use protocol::{
    apply_content_changes, lsp_diagnostic, lsp_position, lsp_range, offset_at_position,
    read_message, write_message,
};

const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMPLETION_ITEMS: usize = 200;
const MAX_COMPLETION_PROVIDERS: usize = 32;
const MAX_COMPLETION_LABEL_BYTES: usize = 256;
const MAX_COMPLETION_DETAIL_BYTES: usize = 1024;
const MAX_COMPLETION_INSERT_BYTES: usize = 4096;
const MAX_DOCUMENT_SYMBOLS: usize = 10_000;
const MAX_RENAME_NAME_BYTES: usize = 1024;
const MAX_RENAME_EDITS: usize = 100_000;
const MAX_PROJECT_NAVIGATION_AST_NODES: usize = 1_000_000;
const MAX_PROJECT_NAVIGATION_SYNTAX_TOKENS: usize = 2_000_000;
const MAX_PROJECT_NAVIGATION_PATTERN_STEPS: usize = 20_000_000;
const PROJECT_WATCHER_REGISTRATION_ID: &str = "langmeld-project-watchers";
const PROJECT_WATCHER_REFRESH_ID: &str = "langmeld-project-watchers-refresh";

/// Creates a generic parser for a language that does not require host callbacks.
///
/// # Errors
/// Returns a diagnostic setup error when patterns are invalid or the language
/// declares custom parser or analysis behavior that this generic server cannot
/// implement.
pub fn configure_language(
    language: ValidatedLanguageSpec,
) -> Result<ConfiguredLanguageParser, String> {
    if !language.spec().behaviors.is_empty() {
        return Err(
            "language declares host behavior; configure its callbacks and use `serve_configured`"
                .to_owned(),
        );
    }
    PreparedLanguage::new(language)
        .map_err(|error| error.to_string())?
        .configure()
        .map_err(|error| error.to_string())
}

/// Configuration for an LSP server embedded in a language host.
///
/// The parser must already have its custom construct callbacks and lexer
/// registered. Analysis registrations are checked against the parser's
/// language before the server reads protocol input.
pub struct LanguageServerConfig {
    parser: ConfiguredLanguageParser,
    project_snapshot: Option<(PathBuf, ProjectSnapshot)>,
    definition_root: Option<PathBuf>,
    semantic_hooks: Vec<SemanticHookRegistration>,
    configured_semantic_hooks: Vec<ConfiguredSemanticHookRegistration>,
    formatting_provider: Option<Arc<FormattingProviderFn>>,
    symbol_resolvers: Vec<WorkspaceSymbolResolverRegistration>,
    type_analysis_hooks: Vec<WorkspaceTypeAnalysisRegistration>,
    completion_providers: Vec<CompletionProviderRegistration>,
}

impl LanguageServerConfig {
    /// Creates an embedded-server configuration from a host-configured parser.
    #[must_use]
    pub fn new(parser: ConfiguredLanguageParser) -> Self {
        Self {
            parser,
            project_snapshot: None,
            definition_root: None,
            semantic_hooks: Vec::new(),
            configured_semantic_hooks: Vec::new(),
            formatting_provider: None,
            symbol_resolvers: Vec::new(),
            type_analysis_hooks: Vec::new(),
            completion_providers: Vec::new(),
        }
    }

    /// Preloads a validated project snapshot into this embedded language server.
    ///
    /// The snapshot language must match the supplied parser. The manifest path
    /// anchors project file watching and reloads when the client supports it.
    #[must_use]
    pub fn with_project_snapshot(
        mut self,
        manifest_path: impl Into<PathBuf>,
        snapshot: ProjectSnapshot,
    ) -> Self {
        self.project_snapshot = Some((manifest_path.into(), snapshot));
        self
    }

    fn with_definition_root(mut self, root_path: impl Into<PathBuf>) -> Self {
        self.definition_root = Some(root_path.into());
        self
    }

    /// Adds per-file semantic callback registrations.
    #[must_use]
    pub fn with_semantic_hooks(
        mut self,
        registrations: impl IntoIterator<Item = SemanticHookRegistration>,
    ) -> Self {
        self.semantic_hooks.extend(registrations);
        self
    }

    /// Adds configured per-file semantic callback registrations.
    #[must_use]
    pub fn with_configured_semantic_hooks(
        mut self,
        registrations: impl IntoIterator<Item = ConfiguredSemanticHookRegistration>,
    ) -> Self {
        self.configured_semantic_hooks.extend(registrations);
        self
    }

    /// Adds workspace symbol-resolver callback registrations.
    #[must_use]
    pub fn with_symbol_resolvers(
        mut self,
        registrations: impl IntoIterator<Item = WorkspaceSymbolResolverRegistration>,
    ) -> Self {
        self.symbol_resolvers.extend(registrations);
        self
    }

    /// Adds workspace type-analysis callback registrations.
    #[must_use]
    pub fn with_type_analysis_hooks(
        mut self,
        registrations: impl IntoIterator<Item = WorkspaceTypeAnalysisRegistration>,
    ) -> Self {
        self.type_analysis_hooks.extend(registrations);
        self
    }

    /// Adds typed completion providers to an embedded language server.
    #[must_use]
    pub fn with_completion_providers(
        mut self,
        registrations: impl IntoIterator<Item = CompletionProviderRegistration>,
    ) -> Self {
        self.completion_providers.extend(registrations);
        self
    }

    /// Registers a host formatter for languages whose syntax needs custom
    /// formatting. The returned text is reparsed and analyzed before an edit
    /// is offered; the host remains responsible for preserving semantics.
    #[must_use]
    pub fn with_formatting_provider(
        mut self,
        provider: impl Fn(
            &ValidatedLanguageSpec,
            &SourceFile,
            &ParseResult,
            FormatOptions,
        ) -> Result<String, String>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.formatting_provider = Some(Arc::new(provider));
        self
    }

    fn into_server(self) -> Result<Server, String> {
        if self.completion_providers.len() > MAX_COMPLETION_PROVIDERS {
            return Err(format!(
                "too many completion providers: maximum is {MAX_COMPLETION_PROVIDERS}"
            ));
        }
        let language = self.parser.language().spec().clone();
        validate_semantic_hook_registrations(
            &language,
            &self.semantic_hooks,
            &self.configured_semantic_hooks,
        )
        .map_err(|error| format!("invalid semantic hooks: {error}"))?;
        let semantic_hooks = self
            .semantic_hooks
            .into_iter()
            .map(|registration| registration.hook)
            .collect();
        let configured_semantic_hooks = self
            .configured_semantic_hooks
            .into_iter()
            .map(|registration| registration.hook)
            .collect();
        let symbol_resolver_hooks =
            WorkspaceSymbolResolverRegistry::new(&language, self.symbol_resolvers)
                .map_err(|error| format!("invalid symbol resolvers: {error}"))?
                .into_hooks();
        let type_analysis_hooks =
            WorkspaceTypeAnalysisRegistry::new(&language, self.type_analysis_hooks)
                .map_err(|error| format!("invalid type-analysis hooks: {error}"))?
                .into_hooks();
        if self.project_snapshot.is_some() && self.definition_root.is_some() {
            return Err("a language server cannot serve two project roots".to_owned());
        }
        let definition_project = self
            .definition_root
            .map(|path| {
                let root_path = path.canonicalize().map_err(|error| {
                    format!(
                        "cannot resolve Langmeld definition root '{}': {error}",
                        path.display()
                    )
                })?;
                if root_path
                    .extension()
                    .is_none_or(|extension| extension != LANGUAGE_DEFINITION_EXTENSION)
                {
                    return Err(format!(
                        "Langmeld definition root '{}' must have a `.{LANGUAGE_DEFINITION_EXTENSION}` extension",
                        root_path.display()
                    ));
                }
                Ok(DefinitionProjectState {
                    root_path,
                    last_diagnostic_uris: BTreeSet::new(),
                })
            })
            .transpose()?;
        let project = self
            .project_snapshot
            .map(|(manifest_path, snapshot)| {
                if snapshot.language.spec() != self.parser.language().spec().spec() {
                    return Err(
                        "project snapshot language does not match the configured parser".to_owned(),
                    );
                }
                let (documents, navigation_index_limited) =
                    index_project_sources(&self.parser, &snapshot);
                Ok(ProjectState {
                    manifest_path,
                    snapshot,
                    source_documents: documents,
                    navigation_index_limited,
                })
            })
            .transpose()?;
        Ok(Server {
            language,
            parser: self.parser,
            semantic_hooks,
            configured_semantic_hooks,
            formatting_provider: self.formatting_provider,
            symbol_resolver_hooks,
            type_analysis_hooks,
            completion_providers: self
                .completion_providers
                .into_iter()
                .map(|registration| registration.provider)
                .collect(),
            project,
            definition_project,
            client_supports_watched_file_registration: false,
            watchers_registered: false,
            watcher_refresh_pending: false,
            documents: HashMap::new(),
            shutdown: false,
        })
    }
}

fn validate_semantic_hook_registrations(
    language: &ValidatedLanguageSpec,
    semantic: &[SemanticHookRegistration],
    configured: &[ConfiguredSemanticHookRegistration],
) -> Result<(), BehaviorRegistrationError> {
    let declared: BTreeSet<_> = language
        .spec()
        .behaviors
        .iter()
        .filter(|behavior| behavior.stage == BehaviorStage::SemanticAnalysis)
        .map(|behavior| behavior.id.as_str())
        .collect();
    let mut registered = BTreeSet::new();
    let mut issues = Vec::new();
    for id in semantic
        .iter()
        .map(|registration| registration.id.as_str())
        .chain(
            configured
                .iter()
                .map(|registration| registration.id.as_str()),
        )
    {
        if !registered.insert(id) {
            issues.push(BehaviorRegistrationIssue::Duplicate(id.to_owned()));
        }
    }
    issues.extend(
        declared
            .difference(&registered)
            .map(|id| BehaviorRegistrationIssue::Missing((*id).to_owned())),
    );
    issues.extend(
        registered
            .difference(&declared)
            .map(|id| BehaviorRegistrationIssue::Undeclared((*id).to_owned())),
    );
    if issues.is_empty() {
        Ok(())
    } else {
        issues.sort();
        Err(BehaviorRegistrationError { issues })
    }
}

/// Serves document synchronization and diagnostics messages.
///
/// Returns true when the client sent shutdown before exit.
///
/// # Errors
/// Returns malformed framing, I/O, or JSON-RPC serialization errors.
pub fn serve<R: BufRead, W: Write>(
    input: R,
    output: W,
    language: ValidatedLanguageSpec,
) -> Result<bool, String> {
    let parser = configure_language(language.clone())?;
    serve_configured(input, output, LanguageServerConfig::new(parser))
}

/// Serves the built-in `.langmeld` definition grammar for editing language source.
///
/// This mode provides generic parser-backed LSP features for each open `.langmeld`
/// file. Use [`serve_definitions_project`] to add imports and project-level
/// definition validation.
///
/// # Errors
/// Returns bootstrap grammar, parser configuration, framing, I/O, or
/// JSON-RPC serialization errors.
pub fn serve_definitions<R: BufRead, W: Write>(input: R, output: W) -> Result<bool, String> {
    let language = langmeld_definition_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    serve(input, output, language)
}

/// Serves the built-in `.langmeld` grammar with project-level definition validation.
///
/// Open buffers inside the root definition directory override their disk
/// snapshots during validation. Clients may receive bounded file watchers for
/// changes to other `.langmeld` modules in the same directory tree.
///
/// # Errors
/// Returns root-path, bootstrap grammar, parser configuration, framing, I/O,
/// or JSON-RPC serialization errors.
pub fn serve_definitions_project<R: BufRead, W: Write>(
    input: R,
    output: W,
    root_path: &Path,
) -> Result<bool, String> {
    let language = langmeld_definition_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let parser = configure_language(language)?;
    serve_configured(
        input,
        output,
        LanguageServerConfig::new(parser).with_definition_root(root_path),
    )
}

/// Serves a declarative project and reloads its language after watched-file
/// notifications when the changed definitions remain valid.
///
/// The last valid language snapshot remains active after a failed reload.
///
/// # Errors
/// Returns project-loading, parser-configuration, framing, I/O, or
/// JSON-RPC serialization errors.
pub fn serve_project<R: BufRead, W: Write>(
    input: R,
    output: W,
    manifest_path: &Path,
) -> Result<bool, String> {
    let snapshot = load_project(manifest_path)?;
    let parser = configure_language(snapshot.language.clone())?;
    let config =
        LanguageServerConfig::new(parser).with_project_snapshot(manifest_path.to_owned(), snapshot);
    serve_configured(input, output, config)
}

/// Serves LSP requests using a host-configured parser and analysis callbacks.
///
/// Callback behavior IDs are validated before protocol input is read. This is
/// the embedding API for languages with custom parsers or semantic behavior;
/// the standalone binary intentionally uses [`serve`] instead.
///
/// Returns true when the client sent shutdown before exit.
///
/// # Errors
/// Returns callback setup, malformed framing, I/O, or JSON-RPC serialization errors.
pub fn serve_configured<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    config: LanguageServerConfig,
) -> Result<bool, String> {
    let mut server = config.into_server()?;

    while let Some(message) = read_message(&mut input).map_err(|error| error.to_string())? {
        let value: Value = match serde_json::from_slice(&message) {
            Ok(value) => value,
            Err(error) => {
                write_message(
                    &mut output,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("invalid JSON: {error}") }
                    }),
                )
                .map_err(|error| error.to_string())?;
                continue;
            }
        };
        let responses = server.handle(&value);
        for response in responses {
            write_message(&mut output, &response).map_err(|error| error.to_string())?;
        }
        if value.get("method").and_then(Value::as_str) == Some("exit") {
            break;
        }
    }
    Ok(server.shutdown)
}

struct Server {
    language: ValidatedLanguageSpec,
    parser: ConfiguredLanguageParser,
    semantic_hooks: Vec<SemanticHook>,
    configured_semantic_hooks: Vec<ConfiguredSemanticHook>,
    formatting_provider: Option<Arc<FormattingProviderFn>>,
    symbol_resolver_hooks: Vec<langmeld_language::WorkspaceSymbolResolverHook>,
    type_analysis_hooks: Vec<langmeld_language::WorkspaceTypeAnalysisHook>,
    completion_providers: Vec<CompletionProvider>,
    project: Option<ProjectState>,
    definition_project: Option<DefinitionProjectState>,
    client_supports_watched_file_registration: bool,
    watchers_registered: bool,
    watcher_refresh_pending: bool,
    documents: HashMap<String, OpenDocument>,
    shutdown: bool,
}

struct ProjectState {
    manifest_path: PathBuf,
    snapshot: ProjectSnapshot,
    source_documents: BTreeMap<String, ProjectDocument>,
    navigation_index_limited: bool,
}

struct DefinitionProjectState {
    root_path: PathBuf,
    last_diagnostic_uris: BTreeSet<String>,
}

struct ProjectDocument {
    source_index: usize,
    parsed: ParseResult,
}

struct OpenDocument {
    version: i64,
    source: SourceFile,
    parsed: ParseResult,
}

type FormattingProviderFn = dyn Fn(&ValidatedLanguageSpec, &SourceFile, &ParseResult, FormatOptions) -> Result<String, String>
    + Send
    + Sync;

impl Server {
    fn handle(&mut self, message: &Value) -> Vec<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            if message.get("id").and_then(Value::as_str) == Some(PROJECT_WATCHER_REFRESH_ID) {
                self.watcher_refresh_pending = false;
                if let Some(error) = message.get("error") {
                    let detail = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("client rejected the watched-file refresh");
                    return vec![log_message(
                        2,
                        format!("project file watchers could not be refreshed: {detail}"),
                    )];
                }
                self.watchers_registered = false;
                return self.register_project_watchers();
            }
            if message.get("id").and_then(Value::as_str) == Some(PROJECT_WATCHER_REGISTRATION_ID)
                && message.get("error").is_some()
            {
                self.watchers_registered = false;
                let detail = message
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("client rejected the watched-file registration");
                return vec![log_message(
                    2,
                    format!("project file watching is unavailable: {detail}"),
                )];
            }
            return Vec::new();
        };
        let id = message.get("id").cloned();
        match method {
            "initialize" => {
                self.client_supports_watched_file_registration = message
                    .pointer("/params/capabilities/workspace/didChangeWatchedFiles/dynamicRegistration")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                id.into_iter()
                    .map(|id| {
                        initialize_response(
                            id,
                            self.supports_completion(),
                            self.supports_navigation(),
                            self.supports_rename(),
                            self.supports_hover(),
                            self.supports_formatting(),
                            self.supports_missing_closer_quickfix(),
                        )
                    })
                    .collect()
            }
            "initialized" => {
                let mut notifications = self.register_project_watchers();
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| project.navigation_index_limited)
                {
                    notifications.insert(
                        0,
                        log_message(
                            2,
                            "project navigation index exceeded its safety bounds; cross-file navigation is disabled for this project snapshot".to_owned(),
                        ),
                    );
                }
                notifications
            }
            "shutdown" => {
                self.shutdown = true;
                id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": null }))
                    .into_iter()
                    .collect()
            }
            "exit" => Vec::new(),
            "textDocument/didOpen" => {
                if let Some(notification) = message.get("params") {
                    self.open_document(notification)
                } else {
                    Vec::new()
                }
            }
            "textDocument/didChange" => {
                if let Some(notification) = message.get("params") {
                    self.change_document(notification)
                } else {
                    Vec::new()
                }
            }
            "workspace/didChangeWatchedFiles" => {
                let changed = message
                    .pointer("/params/changes")
                    .and_then(Value::as_array)
                    .is_some_and(|changes| !changes.is_empty());
                if changed && self.definition_project.is_some() {
                    self.publish_workspace_documents()
                } else if changed && self.project.is_some() {
                    self.reload_project()
                } else {
                    Vec::new()
                }
            }
            "textDocument/completion" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.completion_items(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/documentSymbol" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.document_symbols(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/definition" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.definition_result(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/references" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.reference_results(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/rename" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.rename_result(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/hover" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.hover_result(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/semanticTokens/full" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.semantic_tokens(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/formatting" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.formatting_edits(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/codeAction" => id
                .map(|id| {
                    let params = message.get("params").unwrap_or(&Value::Null);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": self.code_actions(params)
                    })
                })
                .into_iter()
                .collect(),
            "textDocument/didClose" => {
                if let Some(uri) = message
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                {
                    self.documents.remove(uri);
                    let mut notifications = vec![json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": [] }
                    })];
                    notifications.extend(self.publish_workspace_documents());
                    notifications
                } else {
                    Vec::new()
                }
            }
            _ => id
                .map(|id| {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("method not found: {method}") }
                    })
                })
                .into_iter()
                .collect(),
        }
    }

    fn open_document(&mut self, params: &Value) -> Vec<Value> {
        let Some(document) = params.get("textDocument") else {
            return Vec::new();
        };
        let (Some(uri), Some(version), Some(text)) = (
            document.get("uri").and_then(Value::as_str),
            document.get("version").and_then(Value::as_i64),
            document.get("text").and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        if self
            .documents
            .get(uri)
            .is_some_and(|open| version <= open.version)
        {
            return Vec::new();
        }
        if text.len() > MAX_DOCUMENT_BYTES {
            return Vec::new();
        }
        let source = SourceFile::new(uri, text);
        let parsed = self.parser.parse(&source);
        self.documents.insert(
            uri.to_owned(),
            OpenDocument {
                version,
                source,
                parsed,
            },
        );
        self.publish_workspace_documents()
    }

    fn change_document(&mut self, params: &Value) -> Vec<Value> {
        let (Some(uri), Some(version), Some(changes)) = (
            params.pointer("/textDocument/uri").and_then(Value::as_str),
            params
                .pointer("/textDocument/version")
                .and_then(Value::as_i64),
            params.get("contentChanges").and_then(Value::as_array),
        ) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        if version <= document.version || changes.is_empty() {
            return Vec::new();
        }
        let Some(text) = apply_content_changes(&document.source.text, changes) else {
            return Vec::new();
        };
        if text.len() > MAX_DOCUMENT_BYTES {
            return Vec::new();
        }
        let source = SourceFile::new(uri, text);
        let parsed = self.parser.parse(&source);
        self.documents.insert(
            uri.to_owned(),
            OpenDocument {
                version,
                source,
                parsed,
            },
        );
        self.publish_workspace_documents()
    }

    fn register_project_watchers(&mut self) -> Vec<Value> {
        if !self.client_supports_watched_file_registration
            || self.watchers_registered
            || (self.project.is_none() && self.definition_project.is_none())
        {
            return Vec::new();
        }
        if let Some(definition_project) = &self.definition_project {
            let Some(root) = definition_project.root_path.parent() else {
                return vec![log_message(
                    2,
                    "cannot register definition watchers: root has no parent directory".to_owned(),
                )];
            };
            let Some(base_uri) = path_to_file_uri(root) else {
                return vec![log_message(
                    2,
                    "cannot register definition watchers: root cannot be represented as a file URI"
                        .to_owned(),
                )];
            };
            self.watchers_registered = true;
            return vec![json!({
                "jsonrpc": "2.0",
                "id": PROJECT_WATCHER_REGISTRATION_ID,
                "method": "client/registerCapability",
                "params": {
                    "registrations": [{
                        "id": PROJECT_WATCHER_REGISTRATION_ID,
                        "method": "workspace/didChangeWatchedFiles",
                        "registerOptions": {
                            "watchers": [{
                                "globPattern": {
                                    "baseUri": base_uri,
                                    "pattern": format!("**/*.{LANGUAGE_DEFINITION_EXTENSION}")
                                },
                                "kind": 7
                            }]
                        }
                    }]
                }
            })];
        }
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let Ok(manifest_path) = project.manifest_path.canonicalize() else {
            return vec![log_message(
                2,
                "cannot register project watchers: manifest path cannot be resolved".to_owned(),
            )];
        };
        let Some(root) = manifest_path.parent() else {
            return vec![log_message(
                2,
                "cannot register project watchers: manifest has no parent directory".to_owned(),
            )];
        };
        let Some(base_uri) = path_to_file_uri(root) else {
            return vec![log_message(
                2,
                "cannot register project watchers: project path cannot be represented as a file URI"
                    .to_owned(),
            )];
        };
        let manifest_name = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(escape_glob_literal)
            .unwrap_or_else(|| "langmeld.json".to_owned());
        let mut source_patterns = BTreeSet::new();
        for source_root in &project.snapshot.manifest.source_roots {
            let root = source_root.to_string_lossy().replace('\\', "/");
            source_patterns.insert(format!("{}/**", escape_glob_literal(&root)));
        }
        for source in &project.snapshot.manifest.sources {
            let source = source.to_string_lossy().replace('\\', "/");
            source_patterns.insert(escape_glob_literal(&source));
        }
        let definition_pattern = format!("**/*.{LANGUAGE_DEFINITION_EXTENSION}");
        let mut watchers = vec![
            json!({ "globPattern": { "baseUri": base_uri, "pattern": definition_pattern }, "kind": 7 }),
            json!({ "globPattern": { "baseUri": base_uri, "pattern": "**/*.json" }, "kind": 7 }),
            json!({ "globPattern": { "baseUri": base_uri, "pattern": manifest_name }, "kind": 7 }),
            json!({ "globPattern": { "baseUri": base_uri, "pattern": "langmeld.lock" }, "kind": 7 }),
        ];
        watchers.extend(source_patterns.into_iter().map(|pattern| {
            json!({
                "globPattern": { "baseUri": base_uri, "pattern": pattern },
                "kind": 7
            })
        }));
        self.watchers_registered = true;
        vec![json!({
            "jsonrpc": "2.0",
            "id": PROJECT_WATCHER_REGISTRATION_ID,
            "method": "client/registerCapability",
            "params": {
                "registrations": [{
                    "id": PROJECT_WATCHER_REGISTRATION_ID,
                    "method": "workspace/didChangeWatchedFiles",
                    "registerOptions": {
                                "watchers": watchers
                    }
                }]
            }
        })]
    }

    fn refresh_project_watchers(&mut self) -> Vec<Value> {
        if !self.client_supports_watched_file_registration
            || !self.watchers_registered
            || self.watcher_refresh_pending
        {
            return Vec::new();
        }
        self.watcher_refresh_pending = true;
        vec![json!({
            "jsonrpc": "2.0",
            "id": PROJECT_WATCHER_REFRESH_ID,
            "method": "client/unregisterCapability",
            "params": {
                "unregisterations": [{
                    "id": PROJECT_WATCHER_REGISTRATION_ID,
                    "method": "workspace/didChangeWatchedFiles"
                }]
            }
        })]
    }

    fn reload_project(&mut self) -> Vec<Value> {
        let Some(manifest_path) = self
            .project
            .as_ref()
            .map(|project| project.manifest_path.clone())
        else {
            return Vec::new();
        };
        let snapshot = match load_project(&manifest_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return vec![log_message(
                    2,
                    format!("project reload failed; keeping the last valid language: {error}"),
                )];
            }
        };
        let watcher_configuration_changed = self.project.as_ref().is_some_and(|project| {
            project.snapshot.manifest.sources != snapshot.manifest.sources
                || project.snapshot.manifest.source_roots != snapshot.manifest.source_roots
        });
        let language_changed = self
            .project
            .as_ref()
            .is_some_and(|project| project.snapshot.language.spec() != snapshot.language.spec());
        let parser = if language_changed {
            match configure_language(snapshot.language.clone()) {
                Ok(parser) => Some(parser),
                Err(error) => {
                    return vec![log_message(
                        2,
                        format!("project reload failed; keeping the last valid language: {error}"),
                    )];
                }
            }
        } else {
            None
        };
        let parser_ref = parser.as_ref().unwrap_or(&self.parser);
        let (source_documents, navigation_index_limited) =
            index_project_sources(parser_ref, &snapshot);

        self.language = snapshot.language.clone();
        if let Some(parser) = parser {
            self.parser = parser;
        }
        if let Some(project) = &mut self.project {
            project.snapshot = snapshot;
            project.source_documents = source_documents;
            project.navigation_index_limited = navigation_index_limited;
        }
        if language_changed {
            for document in self.documents.values_mut() {
                document.parsed = self.parser.parse(&document.source);
            }
        }
        let mut notifications = self.publish_workspace_documents();
        if navigation_index_limited {
            notifications.insert(
                0,
                log_message(
                    2,
                    "project navigation index exceeded its safety bounds; cross-file navigation is disabled for this project snapshot".to_owned(),
                ),
            );
        }
        if watcher_configuration_changed {
            notifications.extend(self.refresh_project_watchers());
        }
        notifications
    }

    fn workspace_snapshot(&self) -> (Vec<String>, Vec<(&SourceFile, &langmeld_language::Program)>) {
        let mut documents = Vec::new();
        if let Some(project) = &self.project {
            for (uri, indexed) in &project.source_documents {
                if self.documents.contains_key(uri) {
                    continue;
                }
                if let Some(source) = project.snapshot.sources.get(indexed.source_index) {
                    documents.push((uri.clone(), (source, &indexed.parsed.program)));
                }
            }
        }
        documents.extend(
            self.documents
                .iter()
                .map(|(uri, document)| (uri.clone(), (&document.source, &document.parsed.program))),
        );
        documents.sort_by(|left, right| left.0.cmp(&right.0));
        documents.into_iter().unzip()
    }

    fn publish_workspace_documents(&mut self) -> Vec<Value> {
        let (uris, files) = self.workspace_snapshot();
        let analysis = self.analyze_files(&files);
        let mut notifications: Vec<_> = uris
            .iter()
            .enumerate()
            .filter_map(|(source_index, uri)| {
                let document = self.documents.get(uri)?;
                let mut diagnostics = document.parsed.diagnostics.clone();
                diagnostics.extend(
                    analysis
                        .diagnostics
                        .iter()
                        .filter(|item| item.source_index == source_index)
                        .map(|item| item.diagnostic.clone()),
                );
                Some(json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": document.source.name.as_str(),
                        "version": document.version,
                        "diagnostics": diagnostics.iter().map(|item| lsp_diagnostic(&document.source, item)).collect::<Vec<_>>()
                    }
                }))
            })
            .collect();
        self.append_definition_project_diagnostics(&mut notifications);
        notifications
    }

    fn append_definition_project_diagnostics(&mut self, notifications: &mut Vec<Value>) {
        let Some((root_path, previous_uris)) = self.definition_project.as_ref().map(|project| {
            (
                project.root_path.clone(),
                project.last_diagnostic_uris.clone(),
            )
        }) else {
            return;
        };
        let Some(root_directory) = root_path.parent() else {
            return;
        };
        let Some(root_uri) = path_to_file_uri(&root_path) else {
            return;
        };
        let mut overlays = BTreeMap::new();
        for (uri, document) in &self.documents {
            let Some(path) = file_uri_to_path(uri) else {
                continue;
            };
            let Ok(canonical_path) = path.canonicalize() else {
                continue;
            };
            if canonical_path.starts_with(root_directory)
                && canonical_path
                    .extension()
                    .is_some_and(|extension| extension == LANGUAGE_DEFINITION_EXTENSION)
            {
                overlays.insert(path, document.source.text.clone());
            }
        }
        let mut diagnostics_by_uri = BTreeMap::<String, Vec<Value>>::new();
        let diagnostics = parse_language_definition_project_with_overlays_and_diagnostics(
            &root_path,
            &overlays,
            DefinitionLimits::default(),
        )
        .err()
        .unwrap_or_default();
        for project_diagnostic in diagnostics {
            let source_is_in_project = project_diagnostic
                .source_path
                .as_deref()
                .is_some_and(|path| path.starts_with(root_directory));
            let uri = if source_is_in_project {
                project_diagnostic
                    .source_path
                    .as_deref()
                    .and_then(path_to_file_uri)
                    .unwrap_or_else(|| root_uri.clone())
            } else {
                root_uri.clone()
            };
            if source_is_in_project
                && self.documents.get(&uri).is_some_and(|document| {
                    document.parsed.diagnostics.iter().any(|diagnostic| {
                        diagnostic.span == project_diagnostic.diagnostic.span
                            && diagnostic.severity == project_diagnostic.diagnostic.severity
                    })
                })
            {
                continue;
            }
            let source = project_diagnostic
                .source
                .as_deref()
                .cloned()
                .or_else(|| {
                    self.documents
                        .get(&uri)
                        .map(|document| document.source.clone())
                })
                .unwrap_or_else(|| SourceFile::new(uri.as_str(), ""));
            let mut diagnostic = project_diagnostic.diagnostic;
            if !source_is_in_project {
                diagnostic.span = None;
            }
            diagnostics_by_uri
                .entry(uri)
                .or_default()
                .push(lsp_diagnostic(&source, &diagnostic));
        }

        let current_uris: BTreeSet<_> = diagnostics_by_uri.keys().cloned().collect();
        for notification in notifications.iter_mut() {
            let Some(uri) = notification
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            if let Some(project_diagnostics) = diagnostics_by_uri.remove(&uri)
                && let Some(existing) = notification
                    .pointer_mut("/params/diagnostics")
                    .and_then(Value::as_array_mut)
            {
                existing.extend(project_diagnostics);
            }
        }
        for (uri, project_diagnostics) in diagnostics_by_uri {
            let version = self.documents.get(&uri).map(|document| document.version);
            let mut params = json!({ "uri": uri, "diagnostics": project_diagnostics });
            if let Some(version) = version {
                params["version"] = json!(version);
            }
            notifications.push(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": params,
            }));
        }
        for uri in previous_uris.difference(&current_uris) {
            if notifications.iter().any(|notification| {
                notification["method"] == "textDocument/publishDiagnostics"
                    && notification.pointer("/params/uri").and_then(Value::as_str)
                        == Some(uri.as_str())
            }) {
                continue;
            }
            notifications.push(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": uri, "diagnostics": [] },
            }));
        }
        if let Some(project) = &mut self.definition_project {
            project.last_diagnostic_uris = current_uris;
        }
    }

    fn analyze_files(&self, files: &[(&SourceFile, &Program)]) -> WorkspaceAnalysis {
        analyze_workspace_with_host_callbacks(
            &self.language,
            files,
            &self.semantic_hooks,
            &self.configured_semantic_hooks,
            &self.symbol_resolver_hooks,
            &self.type_analysis_hooks,
        )
    }

    fn supports_completion(&self) -> bool {
        !self.completion_providers.is_empty()
            || self.parser.language().keywords().next().is_some()
            || self
                .language
                .spec()
                .constructs
                .iter()
                .flat_map(|construct| &construct.fields)
                .any(|field| {
                    field_kind_has_enum(&field.kind) || field_kind_has_reference(&field.kind)
                })
    }

    fn semantic_tokens(&self, params: &Value) -> Value {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return json!({ "data": [] });
        };
        let Some(document) = self.documents.get(uri) else {
            return json!({ "data": [] });
        };
        let classifications = langmeld_language::classify_parse_result_prepared(
            self.parser.language(),
            &document.source,
            &document.parsed,
        );
        json!({
            "data": semantic_tokens::encode(&document.source.text, &classifications)
        })
    }
}

fn initialize_response(
    id: Value,
    supports_completion: bool,
    supports_navigation: bool,
    supports_rename: bool,
    supports_hover: bool,
    supports_formatting: bool,
    supports_missing_closer_quickfix: bool,
) -> Value {
    let mut capabilities = json!({
        "textDocumentSync": { "openClose": true, "change": 2 },
        "documentSymbolProvider": true
    });
    if supports_completion {
        capabilities["completionProvider"] = json!({ "resolveProvider": false });
    }
    if supports_navigation {
        capabilities["definitionProvider"] = json!(true);
        capabilities["referencesProvider"] = json!(true);
    }
    if supports_rename {
        capabilities["renameProvider"] = json!(true);
    }
    if supports_hover {
        capabilities["hoverProvider"] = json!(true);
    }
    if supports_formatting {
        capabilities["documentFormattingProvider"] = json!(true);
        let mut kinds = vec!["source"];
        if supports_missing_closer_quickfix {
            kinds.push("quickfix");
        }
        capabilities["codeActionProvider"] = json!({ "codeActionKinds": kinds });
    }
    capabilities["semanticTokensProvider"] = json!({
        "legend": {
            "tokenTypes": langmeld_language::SEMANTIC_TOKEN_TYPES,
            "tokenModifiers": langmeld_language::SEMANTIC_TOKEN_MODIFIERS
        },
        "full": true
    });
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "capabilities": capabilities,
            "serverInfo": {
                "name": "langmeld-lsp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn log_message(kind: u8, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "window/logMessage",
        "params": { "type": kind, "message": message }
    })
}

fn index_project_sources(
    parser: &ConfiguredLanguageParser,
    snapshot: &ProjectSnapshot,
) -> (BTreeMap<String, ProjectDocument>, bool) {
    let mut documents = BTreeMap::new();
    let mut node_count = 0_usize;
    let mut token_count = 0_usize;
    let default_limits = ParseLimits::default();
    let pattern_steps_per_file = (MAX_PROJECT_NAVIGATION_PATTERN_STEPS
        / snapshot.sources.len().max(1))
    .min(default_limits.max_pattern_steps);
    for (source_index, source) in snapshot.sources.iter().enumerate() {
        let Some(uri) = path_to_file_uri(Path::new(&source.name)) else {
            return (BTreeMap::new(), true);
        };
        let remaining_tokens = MAX_PROJECT_NAVIGATION_SYNTAX_TOKENS.saturating_sub(token_count);
        if remaining_tokens == 0 {
            return (BTreeMap::new(), true);
        }
        let parsed = parser.parse_with_limits(
            source,
            ParseLimits {
                max_tokens: default_limits.max_tokens.min(remaining_tokens),
                max_pattern_steps: pattern_steps_per_file,
                ..default_limits
            },
        );
        node_count = node_count.saturating_add(ast_node_count(&parsed.program.nodes));
        token_count = token_count.saturating_add(parsed.syntax_tokens.len());
        if parsed.resource_limited
            || node_count > MAX_PROJECT_NAVIGATION_AST_NODES
            || token_count > MAX_PROJECT_NAVIGATION_SYNTAX_TOKENS
        {
            return (BTreeMap::new(), true);
        }
        documents.insert(
            uri,
            ProjectDocument {
                source_index,
                parsed,
            },
        );
    }
    (documents, false)
}

fn ast_node_count(nodes: &[AstNode]) -> usize {
    nodes.iter().fold(0_usize, |count, node| {
        count
            .saturating_add(1)
            .saturating_add(ast_node_count(&node.children))
    })
}

fn path_to_file_uri(path: &Path) -> Option<String> {
    let path = path.canonicalize().ok()?;
    let path = path.to_str()?;
    #[cfg(windows)]
    let path = {
        if let Some(unc_path) = path.strip_prefix(r"\\?\UNC\") {
            format!("//{}", unc_path.replace('\\', "/"))
        } else {
            path.strip_prefix(r"\\?\")
                .unwrap_or(path)
                .replace('\\', "/")
        }
    };
    #[cfg(not(windows))]
    let path = path.to_owned();
    let mut uri = if cfg!(windows) && path.starts_with("//") {
        String::from("file:")
    } else if cfg!(windows) {
        String::from("file:///")
    } else {
        String::from("file://")
    };
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~') {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(uri)
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let encoded_path = uri.strip_prefix("file://")?;
    let encoded_path = if let Some(path) = encoded_path.strip_prefix("localhost/") {
        format!("/{path}")
    } else if encoded_path.starts_with('/') {
        encoded_path.to_owned()
    } else {
        #[cfg(windows)]
        {
            format!("//{encoded_path}")
        }
        #[cfg(not(windows))]
        {
            return None;
        }
    };
    if encoded_path.contains('?') || encoded_path.contains('#') {
        return None;
    }

    let bytes = encoded_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            decoded.push((hex_digit(high)? << 4) | hex_digit(low)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let path = String::from_utf8(decoded).ok()?;
    #[cfg(windows)]
    let path = {
        let mut path = path.replace('/', "\\");
        if path.starts_with("\\") && path.as_bytes().get(2) == Some(&b':') {
            path.remove(0);
        }
        path
    };
    #[cfg(not(windows))]
    if !path.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(path))
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn escape_glob_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '{' | '}' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests;
