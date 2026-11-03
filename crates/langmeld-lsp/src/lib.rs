//! A small LSP transport and diagnostic server for declarative Langmeld languages.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use langmeld_core::{BehaviorStage, Diagnostic, Severity, SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{
    AstNode, BehaviorRegistrationError, BehaviorRegistrationIssue, CompletionPart,
    ConfiguredLanguageParser, ConfiguredSemanticHook, ConfiguredSemanticHookRegistration,
    DefinitionLimits, FormatOptions, ParseLimits, ParseResult, PreparedLanguage, Program,
    ProjectSnapshot, SemanticHook, SemanticHookRegistration, SyntaxTokenKind, WorkspaceAnalysis,
    WorkspaceSymbolResolverRegistration, WorkspaceSymbolResolverRegistry,
    WorkspaceTypeAnalysisRegistration, WorkspaceTypeAnalysisRegistry,
    analyze_workspace_with_host_callbacks, format_parsed_source, langmeld_definition_language,
    load_project, parse_language_definition_project_with_overlays_and_diagnostics,
};
use serde_json::{Value, json};

mod navigation;
mod semantic_tokens;

const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMPLETION_ITEMS: usize = 200;
const MAX_COMPLETION_PROVIDERS: usize = 32;
const MAX_COMPLETION_LABEL_BYTES: usize = 256;
const MAX_COMPLETION_DETAIL_BYTES: usize = 1024;
const MAX_COMPLETION_INSERT_BYTES: usize = 4096;
const MAX_DOCUMENT_SYMBOLS: usize = 10_000;
const MAX_FORMAT_TAB_SIZE: usize = 32;
const MAX_RENAME_NAME_BYTES: usize = 1024;
const MAX_RENAME_EDITS: usize = 100_000;
const MAX_PROJECT_NAVIGATION_AST_NODES: usize = 1_000_000;
const MAX_PROJECT_NAVIGATION_SYNTAX_TOKENS: usize = 2_000_000;
const MAX_PROJECT_NAVIGATION_PATTERN_STEPS: usize = 20_000_000;
const PROJECT_WATCHER_REGISTRATION_ID: &str = "langmeld-project-watchers";
const PROJECT_WATCHER_REFRESH_ID: &str = "langmeld-project-watchers-refresh";

/// LSP completion item kind supported by the host completion API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionItemKind {
    /// Plain text suggestion.
    #[default]
    Text,
    /// Method suggestion.
    Method,
    /// Function suggestion.
    Function,
    /// Field suggestion.
    Field,
    /// Variable suggestion.
    Variable,
    /// Class or construct suggestion.
    Class,
    /// Enum suggestion.
    Enum,
    /// Enum member suggestion.
    EnumMember,
    /// Keyword suggestion.
    Keyword,
    /// Constant suggestion.
    Constant,
}

/// Format used for a completion candidate's inserted text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionInsertTextFormat {
    /// Insert text literally.
    #[default]
    PlainText,
    /// Interpret inserted text as an LSP snippet.
    Snippet,
}

impl CompletionItemKind {
    const fn lsp_number(self) -> u8 {
        match self {
            Self::Text => 1,
            Self::Method => 2,
            Self::Function => 3,
            Self::Field => 5,
            Self::Variable => 6,
            Self::Class => 7,
            Self::Enum => 13,
            Self::EnumMember => 12,
            Self::Keyword => 14,
            Self::Constant => 21,
        }
    }
}

/// A host completion suggestion. Replacement spans use UTF-8 byte offsets in
/// the current source snapshot and must end at the request cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Visible completion label.
    pub label: String,
    /// Optional short description shown by the client.
    pub detail: Option<String>,
    /// LSP completion category.
    pub kind: CompletionItemKind,
    /// Text inserted when the suggestion is accepted; defaults to `label`.
    pub insert_text: Option<String>,
    /// Whether inserted text is plain text or an LSP snippet.
    pub insert_text_format: CompletionInsertTextFormat,
    /// Optional UTF-8 byte range replaced by the suggestion.
    pub replacement: Option<Span>,
}

impl CompletionCandidate {
    /// Creates a text suggestion with the supplied label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }
}

/// One open source document included in completion context.
pub struct CompletionDocument<'a> {
    /// Stable document URI.
    pub uri: &'a str,
    /// Current source snapshot.
    pub source: &'a SourceFile,
    /// Parse result corresponding to the source snapshot.
    pub parsed: &'a ParseResult,
}

/// Read-only request data supplied to an embedded completion provider.
pub struct CompletionContext<'a> {
    /// Validated language definition.
    pub language: &'a ValidatedLanguageSpec,
    /// Current document URI.
    pub uri: &'a str,
    /// Current source snapshot.
    pub source: &'a SourceFile,
    /// Parse result corresponding to the source snapshot.
    pub parsed: &'a ParseResult,
    /// Open documents in deterministic URI order.
    pub workspace: &'a [CompletionDocument<'a>],
    /// Current cursor as a UTF-8 byte offset.
    pub byte_offset: usize,
    /// Current cursor line, zero-based.
    pub line: u32,
    /// Current cursor UTF-16 character offset.
    pub character: u32,
    /// Identifier-like text immediately before the cursor.
    pub prefix: &'a str,
}

type CompletionProvider = Box<
    dyn for<'context> Fn(&CompletionContext<'context>) -> Vec<CompletionCandidate> + Send + Sync,
>;

/// A registered host completion callback for an embedded language server.
pub struct CompletionProviderRegistration {
    provider: CompletionProvider,
}

impl CompletionProviderRegistration {
    /// Registers a callback whose typed suggestions are validated and capped
    /// before the server writes the completion response.
    #[must_use]
    pub fn new(
        provider: impl for<'context> Fn(&CompletionContext<'context>) -> Vec<CompletionCandidate>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            provider: Box::new(provider),
        }
    }
}

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
                    .is_none_or(|extension| extension != "lm")
                {
                    return Err(format!(
                        "Langmeld definition root '{}' must have a `.lm` extension",
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

/// Serves the built-in `.lm` definition grammar for editing language source.
///
/// This mode provides generic parser-backed LSP features for each open `.lm`
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

/// Serves the built-in `.lm` grammar with project-level definition validation.
///
/// Open buffers inside the root definition directory override their disk
/// snapshots during validation. Clients may receive bounded file watchers for
/// changes to other `.lm` modules in the same directory tree.
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

struct NavigationRequest<'a> {
    uris: Vec<String>,
    files: Vec<(&'a SourceFile, &'a Program)>,
    source_index: usize,
    occurrence: navigation::SymbolOccurrence,
    analysis: WorkspaceAnalysis,
}

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
                                "globPattern": { "baseUri": base_uri, "pattern": "**/*.lm" },
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
        let mut watchers = vec![
            json!({ "globPattern": { "baseUri": base_uri, "pattern": "**/*.lm" }, "kind": 7 }),
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
                    .is_some_and(|extension| extension == "lm")
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

    fn supports_navigation(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| construct.symbol.is_some())
    }

    fn supports_rename(&self) -> bool {
        self.supports_navigation() && self.symbol_resolver_hooks.is_empty()
    }

    fn supports_hover(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| !construct.fields.is_empty() || construct.symbol.is_some())
    }

    fn supports_builtin_formatting(&self) -> bool {
        !self.parser.has_custom_lexer()
            && self
                .language
                .spec()
                .constructs
                .iter()
                .all(|construct| construct.custom_parser.is_none())
    }

    fn supports_formatting(&self) -> bool {
        self.formatting_provider.is_some() || self.supports_builtin_formatting()
    }

    fn supports_missing_closer_quickfix(&self) -> bool {
        self.supports_builtin_formatting()
    }

    fn format_document(
        &self,
        uri: &str,
        document: &OpenDocument,
        options: FormatOptions,
    ) -> Option<String> {
        if document.parsed.resource_limited {
            return None;
        }
        if let Some(provider) = &self.formatting_provider {
            let formatted =
                provider(&self.language, &document.source, &document.parsed, options).ok()?;
            return if self.validate_formatted_document(uri, document, &formatted) {
                Some(formatted)
            } else {
                None
            };
        }
        if !self.supports_builtin_formatting() || !document.parsed.custom_parser_spans.is_empty() {
            return None;
        }
        format_parsed_source(&self.language, &document.source, &document.parsed, options).ok()
    }

    fn validate_formatted_document(
        &self,
        uri: &str,
        document: &OpenDocument,
        formatted: &str,
    ) -> bool {
        if formatted.len() > MAX_DOCUMENT_BYTES {
            return false;
        }
        let source = SourceFile::new(document.source.name.as_str(), formatted);
        let parsed = self.parser.parse(&source);
        if parsed.resource_limited
            || parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return false;
        }

        let mut files = Vec::with_capacity(self.documents.len());
        let mut formatted_index = None;
        for (open_uri, open_document) in &self.documents {
            if open_uri == uri {
                formatted_index = Some(files.len());
                files.push((&source, &parsed.program));
            } else {
                files.push((&open_document.source, &open_document.parsed.program));
            }
        }
        let Some(formatted_index) = formatted_index else {
            return false;
        };
        let analysis = self.analyze_files(&files);
        !analysis.resource_limited
            && !analysis.diagnostics.iter().any(|diagnostic| {
                diagnostic.source_index == formatted_index
                    && diagnostic.diagnostic.severity == Severity::Error
            })
    }

    fn formatting_edits(&self, params: &Value) -> Vec<Value> {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        if !requested_version_matches(params, document) {
            return Vec::new();
        }
        let Some(options) = formatting_options(params) else {
            return Vec::new();
        };
        let Some(formatted) = self.format_document(uri, document, options) else {
            return Vec::new();
        };
        if formatted == document.source.text {
            return Vec::new();
        }
        vec![json!({
            "range": lsp_range(
                &document.source.text,
                Span::new(0, document.source.text.len())
            ),
            "newText": formatted
        })]
    }

    fn code_actions(&self, params: &Value) -> Vec<Value> {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        if !requested_version_matches(params, document)
            || !valid_request_range(params, &document.source.text)
        {
            return Vec::new();
        }
        let mut actions = Vec::new();
        if requested_action_kind(params, "quickfix")
            && let Some(action) = self.missing_closer_action(uri, document, params)
        {
            actions.push(action);
        }
        if requested_action_kind(params, "source")
            && let Some(formatted) = self.format_document(uri, document, FormatOptions::default())
            && formatted != document.source.text
        {
            let edits = [json!({
                "range": lsp_range(
                    &document.source.text,
                    Span::new(0, document.source.text.len())
                ),
                "newText": formatted
            })];
            actions.push(json!({
                "title": "Format document",
                "kind": "source",
                "edit": {
                    "documentChanges": [{
                        "textDocument": { "uri": uri, "version": document.version },
                        "edits": edits
                    }]
                }
            }));
        }
        actions
    }

    fn missing_closer_action(
        &self,
        uri: &str,
        document: &OpenDocument,
        params: &Value,
    ) -> Option<Value> {
        if !self.supports_missing_closer_quickfix() || document.parsed.resource_limited {
            return None;
        }
        let missing: Vec<_> = document
            .parsed
            .syntax_tokens
            .iter()
            .filter(|token| {
                matches!(
                    token.kind,
                    SyntaxTokenKind::MissingRightBrace | SyntaxTokenKind::MissingRightBracket
                )
            })
            .collect();
        let [token] = missing.as_slice() else {
            return None;
        };
        let (code, closer) = match token.kind {
            SyntaxTokenKind::MissingRightBrace => ("LM1204", "}"),
            SyntaxTokenKind::MissingRightBracket => ("LM1219", "]"),
            _ => return None,
        };
        let errors: Vec<_> = document
            .parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .collect();
        let [diagnostic] = errors.as_slice() else {
            return None;
        };
        if diagnostic.code != code {
            return None;
        }
        let diagnostic_value = lsp_diagnostic(&document.source, diagnostic);
        let requested = params.pointer("/context/diagnostics")?.as_array()?;
        if !requested.iter().any(|candidate| {
            candidate["code"] == diagnostic_value["code"]
                && candidate["range"] == diagnostic_value["range"]
        }) {
            return None;
        }
        let insertion = if closer == "}"
            && !document.source.text.ends_with('\n')
            && !document.source.text.ends_with('\r')
        {
            format!("\n{closer}")
        } else {
            closer.to_owned()
        };
        Some(json!({
            "title": format!("Insert missing `{closer}`"),
            "kind": "quickfix",
            "diagnostics": [diagnostic_value],
            "edit": {
                "documentChanges": [{
                    "textDocument": { "uri": uri, "version": document.version },
                    "edits": [{
                        "range": lsp_range(
                            &document.source.text,
                            Span::new(document.source.text.len(), document.source.text.len())
                        ),
                        "newText": insertion
                    }]
                }]
            }
        }))
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

    fn hover_result(&self, params: &Value) -> Value {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Value::Null;
        };
        let Some(line) = params.pointer("/position/line").and_then(Value::as_u64) else {
            return Value::Null;
        };
        let Some(character) = params
            .pointer("/position/character")
            .and_then(Value::as_u64)
        else {
            return Value::Null;
        };
        let (Ok(line), Ok(character)) = (u32::try_from(line), u32::try_from(character)) else {
            return Value::Null;
        };
        let Some(document) = self.documents.get(uri) else {
            return Value::Null;
        };
        let Some(offset) = offset_at_position(&document.source.text, line, character) else {
            return Value::Null;
        };
        if let Some(occurrence) =
            navigation::occurrence_at(&self.language, &document.parsed.program, offset)
        {
            let symbol_kind = declaration_symbol_kind(
                &self.language,
                &document.parsed.program.nodes,
                occurrence.span,
            );
            let (uris, files) = self.workspace_snapshot();
            let Some(source_index) = uris.iter().position(|open_uri| open_uri == uri) else {
                return Value::Null;
            };
            let analysis = self.analyze_files(&files);
            let resolved = analysis.resolved_references.iter().find(|reference| {
                reference.source_index == source_index
                    && reference.reference_span == occurrence.span
            });
            let name = document
                .source
                .text
                .get(occurrence.span.start..occurrence.span.end)
                .unwrap_or_default();
            if let Some(resolved) = resolved {
                return hover_markdown(format!(
                    "`{} {}`\n\nResolved to `{}`.",
                    resolved.kind, name, resolved.name
                ));
            }
            if let Some(kind) = symbol_kind {
                return hover_markdown(format!("`{kind} {name}`\n\nDeclared symbol."));
            }
            if !occurrence.is_declaration {
                return hover_markdown(format!("`{name}`\n\nUnresolved reference."));
            }
        }
        let files = [(&document.source, &document.parsed.program)];
        let analysis = self.analyze_files(&files);
        let Some(fact) = analysis
            .type_facts
            .iter()
            .filter(|fact| {
                fact.source_index == 0 && fact.span.start <= offset && offset < fact.span.end
            })
            .min_by_key(|fact| fact.span.end - fact.span.start)
        else {
            return Value::Null;
        };
        let path = fact
            .path
            .iter()
            .map(|segment| match segment {
                langmeld_language::TypePathSegment::Field(name) => name.clone(),
                langmeld_language::TypePathSegment::ListItem(index)
                | langmeld_language::TypePathSegment::MapItem(index) => format!("[{index}]"),
                langmeld_language::TypePathSegment::MapKey(key) => format!("key `{key}`"),
                langmeld_language::TypePathSegment::MapValue(key) => format!("value `{key}`"),
            })
            .collect::<Vec<_>>()
            .join(" → ");
        let spelling = document
            .source
            .text
            .get(fact.span.start..fact.span.end)
            .unwrap_or_default();
        hover_markdown(format!(
            "`{}.{}`\n\nDeclared: `{}`  \nParsed as: `{}`  \nValue: `{}`",
            fact.construct,
            path,
            describe_field_kind(&fact.expected),
            describe_value_type(fact.actual),
            spelling
        ))
    }

    fn completion_items(&self, params: &Value) -> Vec<Value> {
        let (Some(uri), Some(line), Some(character)) = (
            params.pointer("/textDocument/uri").and_then(Value::as_str),
            params.pointer("/position/line").and_then(Value::as_u64),
            params
                .pointer("/position/character")
                .and_then(Value::as_u64),
        ) else {
            return Vec::new();
        };
        let (Ok(line), Ok(character)) = (u32::try_from(line), u32::try_from(character)) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some(offset) = offset_at_position(&document.source.text, line, character) else {
            return Vec::new();
        };
        let text = &document.source.text;
        let (word_start, prefix) = completion_prefix(text, offset);
        let mut items = Vec::new();
        let mut labels = BTreeSet::new();
        let has_contextual_fields = self
            .language
            .spec()
            .constructs
            .iter()
            .flat_map(|construct| &construct.fields)
            .any(|field| field_kind_has_enum(&field.kind) || field_kind_has_reference(&field.kind));
        let (uris, files) = self.workspace_snapshot();
        let Some(source_index) = uris.iter().position(|open_uri| open_uri == uri) else {
            return Vec::new();
        };
        if has_contextual_fields {
            let analysis = self.analyze_files(&files);
            if !analysis.resource_limited
                && let Some(fact) = analysis
                    .type_facts
                    .iter()
                    .filter(|fact| {
                        fact.source_index == source_index
                            && fact.span.start <= offset
                            && offset <= fact.span.end
                    })
                    .min_by_key(|fact| fact.span.end - fact.span.start)
            {
                if let langmeld_core::FieldKind::Enum { values }
                | langmeld_core::FieldKind::NamedEnum { values, .. } = &fact.expected
                {
                    for value in values
                        .iter()
                        .filter(|value| value.starts_with(prefix))
                        .take(MAX_COMPLETION_ITEMS)
                    {
                        if let Some(item) = completion_item(
                            value,
                            value,
                            CompletionItemKind::EnumMember,
                            None,
                            Span::new(word_start, offset),
                            text,
                        ) {
                            push_unique_completion(&mut items, &mut labels, item);
                        }
                    }
                } else if let langmeld_core::FieldKind::Reference { target } = &fact.expected
                    && let Some((symbol_scope, ancestors)) = find_reference_context(
                        &self.language,
                        &document.parsed.program.nodes,
                        fact.span,
                    )
                    && symbol_scope == *target
                {
                    let Some(scope) = self
                        .language
                        .spec()
                        .constructs
                        .iter()
                        .filter_map(|construct| construct.symbol.as_ref())
                        .find(|symbol| symbol.kind == *target)
                        .map(|symbol| symbol.scope.as_ref())
                    else {
                        return items;
                    };
                    let visible_scopes = visible_symbol_scopes(scope, &ancestors);
                    for (workspace_uri, (_, program)) in uris.iter().zip(&files) {
                        let mut ancestor_nodes = Vec::new();
                        collect_visible_declarations(
                            &self.language,
                            &program.nodes,
                            &mut ancestor_nodes,
                            target,
                            &visible_scopes,
                            workspace_uri,
                            prefix,
                            &mut items,
                            &mut labels,
                            text,
                            offset,
                            word_start,
                        );
                        if items.len() >= MAX_COMPLETION_ITEMS {
                            break;
                        }
                    }
                }
            }
        }
        let frontier = document
            .parsed
            .completion_frontiers
            .iter()
            .filter(|frontier| frontier.span.start <= offset && offset <= frontier.span.end)
            .min_by_key(|frontier| frontier.span.end - frontier.span.start);
        let has_frontier = frontier.is_some();
        if let Some(frontier) = frontier.filter(|frontier| !frontier.ambiguous) {
            for capture in &frontier.captures {
                if let langmeld_core::FieldKind::Enum { values }
                | langmeld_core::FieldKind::NamedEnum { values, .. } = &capture.kind
                {
                    for value in values
                        .iter()
                        .filter(|value| value.starts_with(prefix))
                        .take(MAX_COMPLETION_ITEMS.saturating_sub(items.len()))
                    {
                        if let Some(item) = completion_item(
                            value,
                            value,
                            CompletionItemKind::EnumMember,
                            Some(&capture.field),
                            Span::new(word_start, offset),
                            text,
                        ) {
                            push_unique_completion(&mut items, &mut labels, item);
                        }
                    }
                }
            }
        }
        if items.len() < MAX_COMPLETION_ITEMS
            && let Some(frontier) = frontier
            && let Some(parts) = &frontier.continuation
            && let Some(snippet) = completion_continuation_snippet(parts)
            && let Some(replacement) =
                continuation_replacement_range(frontier.span, offset, word_start)
            && let Some(mut item) = completion_item(
                "Continue syntax",
                &snippet,
                CompletionItemKind::Text,
                None,
                replacement,
                text,
            )
        {
            item["insertTextFormat"] = json!(2);
            push_unique_completion(&mut items, &mut labels, item);
        }
        if !has_frontier && items.len() < MAX_COMPLETION_ITEMS {
            let spec = self.language.spec();
            let block = document
                .parsed
                .completion_blocks
                .iter()
                .filter(|block| block.span.start <= offset && offset <= block.span.end)
                .min_by_key(|block| block.span.end - block.span.start);
            if let Some(block) = block {
                for construct in spec.constructs.iter().filter(|construct| {
                    block.allowed.iter().any(|allowed| {
                        allowed == &construct.id || construct.implements.contains(allowed)
                    }) && construct.id.starts_with(prefix)
                }) {
                    let Some(pattern) = construct.syntax.first() else {
                        continue;
                    };
                    let Some(snippet) = construct_pattern_snippet(pattern) else {
                        continue;
                    };
                    if let Some(mut item) = completion_item(
                        &construct.id,
                        &snippet,
                        CompletionItemKind::Class,
                        Some(&block.parent),
                        Span::new(word_start, offset),
                        text,
                    ) {
                        item["insertTextFormat"] = json!(2);
                        push_unique_completion(&mut items, &mut labels, item);
                    }
                    if items.len() >= MAX_COMPLETION_ITEMS {
                        break;
                    }
                }
            }
        }
        let workspace = uris
            .iter()
            .filter_map(|open_uri| {
                let open = self.documents.get(open_uri)?;
                Some(CompletionDocument {
                    uri: open_uri,
                    source: &open.source,
                    parsed: &open.parsed,
                })
            })
            .collect::<Vec<_>>();
        let context = CompletionContext {
            language: &self.language,
            uri,
            source: &document.source,
            parsed: &document.parsed,
            workspace: &workspace,
            byte_offset: offset,
            line,
            character,
            prefix,
        };
        for provider in &self.completion_providers {
            if items.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
            for candidate in provider(&context) {
                if items.len() >= MAX_COMPLETION_ITEMS {
                    break;
                }
                if let Some(item) = completion_candidate_item(candidate, text, offset, word_start) {
                    push_unique_completion(&mut items, &mut labels, item);
                }
            }
            if items.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
        }
        if items.len() < MAX_COMPLETION_ITEMS {
            for item in keyword_completion_items(text, offset, self.parser.language().keywords()) {
                if items.len() >= MAX_COMPLETION_ITEMS {
                    break;
                }
                push_unique_completion(&mut items, &mut labels, item);
            }
        }
        items
    }

    fn document_symbols(&self, params: &Value) -> Vec<Value> {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let mut remaining = MAX_DOCUMENT_SYMBOLS;
        document_symbol_items(
            &self.language,
            &document.source.text,
            &document.parsed.program.nodes,
            &mut remaining,
        )
    }

    fn definition_result(&self, params: &Value) -> Vec<Value> {
        let Some(request) = self.navigation_request(params) else {
            return Vec::new();
        };
        if request.analysis.resource_limited {
            return Vec::new();
        }
        let Some((target_index, span)) = navigation::resolved_target(
            request.occurrence,
            request.source_index,
            &request.analysis,
        ) else {
            return Vec::new();
        };
        let (Some(uri), Some((source, _))) = (
            request.uris.get(target_index),
            request.files.get(target_index),
        ) else {
            return Vec::new();
        };
        vec![json!({ "uri": uri, "range": lsp_range(&source.text, span) })]
    }

    fn rename_result(&self, params: &Value) -> Option<Value> {
        if !self.supports_rename() {
            return None;
        }
        let new_name = params.get("newName")?.as_str()?;
        if new_name.len() > MAX_RENAME_NAME_BYTES || !valid_symbol_name(new_name) {
            return None;
        }
        let request = self.navigation_request(params)?;
        if request.analysis.resource_limited {
            return None;
        }
        let (target_index, target_span) = navigation::resolved_target(
            request.occurrence,
            request.source_index,
            &request.analysis,
        )?;
        let target = navigation::validate_rename_target(
            &self.language,
            &request.files,
            target_index,
            target_span,
            new_name,
        )?;
        if new_name == target.name {
            return Some(json!({ "changes": {} }));
        }

        let mut occurrences = vec![(target_index, target_span)];
        for reference in request
            .analysis
            .resolved_references
            .iter()
            .filter(|reference| {
                reference.declaration_source_index == target_index
                    && reference.declaration_span == target_span
            })
        {
            if reference.name != target.name {
                return None;
            }
            occurrences.push((reference.source_index, reference.reference_span));
        }
        if occurrences.len() > MAX_RENAME_EDITS {
            return None;
        }
        occurrences.sort_by_key(|(source_index, span)| (*source_index, span.start, span.end));

        let mut changes = BTreeMap::<String, Vec<Value>>::new();
        let mut last_by_source = None::<(usize, Span)>;
        for (source_index, span) in occurrences {
            if let Some((last_source_index, last_span)) = last_by_source
                && last_source_index == source_index
                && last_span.end > span.start
            {
                return None;
            }
            let uri = request.uris.get(source_index)?;
            let (source, _) = request.files.get(source_index)?;
            if span.start >= span.end
                || source.text.get(span.start..span.end) != Some(target.name.as_str())
            {
                return None;
            }
            changes.entry(uri.clone()).or_default().push(json!({
                "range": lsp_range(&source.text, span),
                "newText": new_name
            }));
            last_by_source = Some((source_index, span));
        }
        Some(json!({ "changes": changes }))
    }

    fn reference_results(&self, params: &Value) -> Vec<Value> {
        let Some(request) = self.navigation_request(params) else {
            return Vec::new();
        };
        if request.analysis.resource_limited {
            return Vec::new();
        }
        let Some((target_index, target_span)) = navigation::resolved_target(
            request.occurrence,
            request.source_index,
            &request.analysis,
        ) else {
            return Vec::new();
        };
        let include_declaration = params
            .pointer("/context/includeDeclaration")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut locations = Vec::new();
        if include_declaration
            && let (Some(uri), Some((source, _))) = (
                request.uris.get(target_index),
                request.files.get(target_index),
            )
        {
            locations.push(json!({
                "uri": uri,
                "range": lsp_range(&source.text, target_span)
            }));
        }
        for reference in request
            .analysis
            .resolved_references
            .iter()
            .filter(|reference| {
                reference.declaration_source_index == target_index
                    && reference.declaration_span == target_span
            })
        {
            let (Some(uri), Some((source, _))) = (
                request.uris.get(reference.source_index),
                request.files.get(reference.source_index),
            ) else {
                continue;
            };
            locations.push(json!({
                "uri": uri,
                "range": lsp_range(&source.text, reference.reference_span)
            }));
        }
        locations
    }

    fn navigation_request(&self, params: &Value) -> Option<NavigationRequest<'_>> {
        if self
            .project
            .as_ref()
            .is_some_and(|project| project.navigation_index_limited)
        {
            return None;
        }
        let uri = params.pointer("/textDocument/uri")?.as_str()?;
        let document = self.documents.get(uri)?;
        let line = u32::try_from(params.pointer("/position/line")?.as_u64()?).ok()?;
        let character = u32::try_from(params.pointer("/position/character")?.as_u64()?).ok()?;
        let offset = offset_at_position(&document.source.text, line, character)?;
        let occurrence =
            navigation::occurrence_at(&self.language, &document.parsed.program, offset)?;
        let (uris, files) = self.workspace_snapshot();
        let source_index = uris.iter().position(|candidate| candidate == uri)?;
        let analysis = self.analyze_files(&files);
        Some(NavigationRequest {
            uris,
            files,
            source_index,
            occurrence,
            analysis,
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
            "tokenTypes": semantic_tokens::TOKEN_TYPES,
            "tokenModifiers": semantic_tokens::TOKEN_MODIFIERS
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

fn formatting_options(params: &Value) -> Option<FormatOptions> {
    let options = params.get("options")?;
    let defaults = FormatOptions::default();
    let tab_size = match options.get("tabSize") {
        Some(value) => usize::try_from(value.as_u64()?).ok()?,
        None => defaults.tab_size,
    };
    if tab_size == 0 || tab_size > MAX_FORMAT_TAB_SIZE {
        return None;
    }
    let insert_spaces = match options.get("insertSpaces") {
        Some(value) => value.as_bool()?,
        None => defaults.insert_spaces,
    };
    Some(FormatOptions {
        tab_size,
        insert_spaces,
    })
}

fn requested_version_matches(params: &Value, document: &OpenDocument) -> bool {
    params
        .pointer("/textDocument/version")
        .is_none_or(|version| version.as_i64() == Some(document.version))
}

fn requested_action_kind(params: &Value, expected: &str) -> bool {
    let Some(only) = params.pointer("/context/only") else {
        return true;
    };
    only.as_array()
        .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some(expected)))
}

fn valid_request_range(params: &Value, source: &str) -> bool {
    let position = |path: &str| {
        let line = u32::try_from(params.pointer(&format!("/range/{path}/line"))?.as_u64()?).ok()?;
        let character = u32::try_from(
            params
                .pointer(&format!("/range/{path}/character"))?
                .as_u64()?,
        )
        .ok()?;
        offset_at_position(source, line, character)
    };
    match (position("start"), position("end")) {
        (Some(start), Some(end)) => start <= end,
        _ => false,
    }
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
    let path = if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    let mut uri = if cfg!(windows) {
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
        return None;
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

fn hover_markdown(value: String) -> Value {
    json!({
        "contents": { "kind": "markdown", "value": value }
    })
}

fn describe_field_kind(kind: &langmeld_core::FieldKind) -> String {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Identifier => "identifier".to_owned(),
        FieldKind::Number => "number".to_owned(),
        FieldKind::Text => "text".to_owned(),
        FieldKind::Enum { values } => format!("enum {{{}}}", values.join(", ")),
        FieldKind::NamedEnum { name, .. } => format!("enum {name}"),
        FieldKind::Reference { target } => format!("reference to {target}"),
        FieldKind::Properties { name, fields } => name.clone().unwrap_or_else(|| {
            format!(
                "record {{{}}}",
                fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, describe_field_kind(&field.kind)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        FieldKind::List { item } => format!("list of {}", describe_field_kind(item)),
        FieldKind::Map { key, value } => format!(
            "map of {} keys to {}",
            match key {
                langmeld_core::MapKeyKind::Identifier => "identifier",
                langmeld_core::MapKeyKind::Text => "text",
            },
            describe_field_kind(value)
        ),
    }
}

fn describe_value_type(kind: langmeld_language::ValueType) -> &'static str {
    use langmeld_language::ValueType;
    match kind {
        ValueType::Identifier => "identifier",
        ValueType::Number => "number",
        ValueType::Text => "text",
        ValueType::Enum => "enum member",
        ValueType::Reference => "reference",
        ValueType::Properties => "property record",
        ValueType::List => "list",
        ValueType::Map => "map",
    }
}

fn document_symbol_items(
    spec: &ValidatedLanguageSpec,
    source: &str,
    nodes: &[AstNode],
    remaining: &mut usize,
) -> Vec<Value> {
    let mut symbols = Vec::new();
    for node in nodes {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        let construct = spec.construct(&node.construct);
        let declaration = construct
            .and_then(|construct| construct.symbol.as_ref())
            .and_then(|symbol| node.fields.get(&symbol.name_field));
        let selection_span = declaration.map_or(node.span, |value| value.span);
        let name = declaration
            .and_then(|value| source.get(value.span.start..value.span.end))
            .unwrap_or(&node.construct);
        let children = document_symbol_items(spec, source, &node.children, remaining);
        symbols.push(json!({
            "name": name,
            "detail": node.construct,
            "kind": 13,
            "range": lsp_range(source, node.span),
            "selectionRange": lsp_range(source, selection_span),
            "children": children
        }));
    }
    symbols
}

fn lsp_range(source: &str, span: Span) -> Value {
    json!({
        "start": lsp_position(source, span.start),
        "end": lsp_position(source, span.end)
    })
}

fn field_kind_has_enum(kind: &langmeld_core::FieldKind) -> bool {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Enum { .. } | FieldKind::NamedEnum { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| field_kind_has_enum(&field.kind))
        }
        FieldKind::List { item } => field_kind_has_enum(item),
        FieldKind::Map { value, .. } => field_kind_has_enum(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. } => false,
    }
}

fn field_kind_has_reference(kind: &langmeld_core::FieldKind) -> bool {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Reference { .. } => true,
        FieldKind::Properties { fields, .. } => fields
            .iter()
            .any(|field| field_kind_has_reference(&field.kind)),
        FieldKind::List { item } => field_kind_has_reference(item),
        FieldKind::Map { value, .. } => field_kind_has_reference(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. } => false,
    }
}

fn find_reference_context<'a>(
    language: &'a ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    span: Span,
) -> Option<(String, Vec<&'a AstNode>)> {
    fn visit<'a>(
        language: &'a ValidatedLanguageSpec,
        nodes: &'a [AstNode],
        span: Span,
        ancestors: &mut Vec<&'a AstNode>,
    ) -> Option<(String, Vec<&'a AstNode>)> {
        for node in nodes {
            ancestors.push(node);
            if let Some(construct) = language.construct(&node.construct) {
                for field in &construct.fields {
                    if let Some(captured) = node.fields.get(&field.name)
                        && let Some(target) = reference_target_at(&field.kind, captured, span)
                    {
                        return Some((target.to_owned(), ancestors.clone()));
                    }
                }
            }
            if let Some(found) = visit(language, &node.children, span, ancestors) {
                return Some(found);
            }
            ancestors.pop();
        }
        None
    }

    visit(language, nodes, span, &mut Vec::new())
}

fn reference_target_at<'a>(
    kind: &'a langmeld_core::FieldKind,
    captured: &'a langmeld_language::CapturedValue,
    span: Span,
) -> Option<&'a str> {
    use langmeld_core::FieldKind;
    use langmeld_language::Value;
    match (kind, &captured.value) {
        (FieldKind::Reference { target }, Value::Reference(_)) if captured.span == span => {
            Some(target)
        }
        (FieldKind::Properties { fields, .. }, Value::Properties(values)) => fields
            .iter()
            .find_map(|field| reference_target_at(&field.kind, values.get(&field.name)?, span)),
        (FieldKind::List { item }, Value::List(values)) => values
            .iter()
            .find_map(|value| reference_target_at(item, value, span)),
        (FieldKind::Map { value, .. }, Value::Map(entries)) => entries
            .iter()
            .find_map(|entry| reference_target_at(value, &entry.value, span)),
        _ => None,
    }
}

type SymbolScopeKey = Vec<(String, String)>;

fn visible_symbol_scopes(
    scope: Option<&langmeld_core::ScopeSpec>,
    ancestors: &[&AstNode],
) -> BTreeSet<SymbolScopeKey> {
    let Some(scope) = scope else {
        return [Vec::new()].into_iter().collect();
    };
    let matching = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            let value = &ancestor.fields.get(&scope.name_field)?.value;
            let langmeld_language::Value::Identifier(name) = value else {
                return None;
            };
            Some((scope.construct.clone(), name.clone()))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return BTreeSet::new();
    }
    match scope.lookup {
        langmeld_core::ScopeLookup::Exact => {
            [vec![matching.last().expect("nonempty scope").clone()]]
                .into_iter()
                .collect()
        }
        langmeld_core::ScopeLookup::LexicalParents => (1..=matching.len())
            .rev()
            .map(|length| matching[..length].to_vec())
            .collect(),
    }
}

fn declaration_scope_key(
    scope: Option<&langmeld_core::ScopeSpec>,
    ancestors: &[&AstNode],
) -> Option<SymbolScopeKey> {
    let Some(scope) = scope else {
        return Some(Vec::new());
    };
    let matching = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            let value = &ancestor.fields.get(&scope.name_field)?.value;
            let langmeld_language::Value::Identifier(name) = value else {
                return None;
            };
            Some((scope.construct.clone(), name.clone()))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        None
    } else if scope.lookup == langmeld_core::ScopeLookup::Exact {
        Some(vec![matching.last()?.clone()])
    } else {
        Some(matching)
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_visible_declarations<'a>(
    language: &ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    ancestors: &mut Vec<&'a AstNode>,
    target: &str,
    visible_scopes: &BTreeSet<SymbolScopeKey>,
    uri: &str,
    prefix: &str,
    items: &mut Vec<Value>,
    labels: &mut BTreeSet<String>,
    source: &str,
    offset: usize,
    word_start: usize,
) {
    for node in nodes {
        if items.len() >= MAX_COMPLETION_ITEMS {
            break;
        }
        ancestors.push(node);
        let declaration = language
            .construct(&node.construct)
            .and_then(|construct| construct.symbol.as_ref())
            .filter(|symbol| symbol.kind == target)
            .and_then(|symbol| {
                let langmeld_language::Value::Identifier(name) =
                    &node.fields.get(&symbol.name_field)?.value
                else {
                    return None;
                };
                Some((symbol.scope.as_ref(), name.as_str()))
            });
        if let Some((declaration_scope, name)) = declaration
            && name.starts_with(prefix)
            && declaration_scope_key(declaration_scope, ancestors)
                .is_some_and(|key| visible_scopes.contains(&key))
            && let Some(item) = completion_item(
                name,
                name,
                CompletionItemKind::Variable,
                Some(&format!("{target} in {uri}")),
                Span::new(word_start, offset),
                source,
            )
        {
            push_unique_completion(items, labels, item);
        }
        collect_visible_declarations(
            language,
            &node.children,
            ancestors,
            target,
            visible_scopes,
            uri,
            prefix,
            items,
            labels,
            source,
            offset,
            word_start,
        );
        ancestors.pop();
    }
}

fn completion_prefix(text: &str, offset: usize) -> (usize, &str) {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return (offset.min(text.len()), "");
    }
    let line_start = text[..offset]
        .rfind(['\r', '\n'])
        .map_or(0, |index| index + 1);
    let line_prefix = &text[line_start..offset];
    let word_start = line_prefix
        .char_indices()
        .rev()
        .find(|(_, value)| !value.is_alphanumeric() && *value != '_' && *value != '-')
        .map_or(line_start, |(index, value)| {
            line_start + index + value.len_utf8()
        });
    (word_start, &text[word_start..offset])
}

fn valid_symbol_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters
            .all(|character| character == '_' || character == '-' || character.is_alphanumeric())
}

fn construct_pattern_snippet(pattern: &str) -> Option<String> {
    let mut output = String::with_capacity(pattern.len());
    let mut characters = pattern.char_indices().peekable();
    let mut tab_stop = 1;
    while let Some((offset, character)) = characters.next() {
        match character {
            '\\' => {
                let (_, escaped) = characters.next()?;
                push_snippet_literal(&mut output, escaped);
            }
            '{' => {
                let remaining = &pattern[offset + character.len_utf8()..];
                let Some(end) = remaining.find('}') else {
                    if offset + character.len_utf8() == pattern.len() {
                        push_snippet_literal(&mut output, character);
                        continue;
                    }
                    return None;
                };
                let field = &remaining[..end];
                if field.is_empty()
                    || !field
                        .chars()
                        .all(|value| value.is_ascii_alphanumeric() || value == '_')
                {
                    return None;
                }
                for _ in 0..=field.chars().count() {
                    characters.next()?;
                }
                output.push_str(&format!("${{{tab_stop}:{field}}}"));
                tab_stop += 1;
            }
            '[' | ']' | '(' | ')' | '|' | '*' | '+' | '@' => return None,
            value => push_snippet_literal(&mut output, value),
        }
    }
    Some(output)
}

fn completion_continuation_snippet(parts: &[CompletionPart]) -> Option<String> {
    let mut output = String::new();
    let mut tab_stop = 1;
    let mut previous = None::<String>;
    for part in parts {
        let current = match part {
            CompletionPart::Literal(value) => value.as_str(),
            CompletionPart::Capture(value) => value.as_str(),
        };
        let word_like = |value: &str| {
            value
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric() || character == '_')
        };
        if let Some(previous) = previous.as_deref()
            && (previous == "->"
                || current == "->"
                || (word_like(previous) && word_like(current))
                || matches!(previous, "," | ";"))
        {
            output.push(' ');
        }
        match part {
            CompletionPart::Literal(value) => {
                for character in value.chars() {
                    push_snippet_literal(&mut output, character);
                }
            }
            CompletionPart::Capture(field) => {
                output.push_str(&format!("${{{tab_stop}:"));
                for character in field.chars() {
                    push_snippet_literal(&mut output, character);
                }
                output.push('}');
                tab_stop += 1;
            }
        }
        if output.len() > MAX_COMPLETION_INSERT_BYTES {
            return None;
        }
        previous = Some(current.to_owned());
    }
    (!output.is_empty()).then_some(output)
}

fn continuation_replacement_range(
    frontier: Span,
    offset: usize,
    word_start: usize,
) -> Option<Span> {
    if frontier.start > frontier.end || offset > frontier.end {
        return None;
    }
    if frontier.start < offset {
        Some(Span::new(word_start.max(frontier.start), offset))
    } else {
        Some(Span::new(offset, offset))
    }
}

fn push_snippet_literal(output: &mut String, value: char) {
    if matches!(value, '$' | '}' | '\\') {
        output.push('\\');
    }
    output.push(value);
}

fn completion_item(
    label: &str,
    insert_text: &str,
    kind: CompletionItemKind,
    detail: Option<&str>,
    replacement: Span,
    source: &str,
) -> Option<Value> {
    if label.is_empty()
        || label.len() > MAX_COMPLETION_LABEL_BYTES
        || insert_text.len() > MAX_COMPLETION_INSERT_BYTES
        || detail.is_some_and(|value| value.len() > MAX_COMPLETION_DETAIL_BYTES)
        || replacement.start > replacement.end
        || replacement.end > source.len()
        || !source.is_char_boundary(replacement.start)
        || !source.is_char_boundary(replacement.end)
    {
        return None;
    }
    let mut item = json!({
        "label": label,
        "kind": kind.lsp_number(),
        "filterText": label,
        "textEdit": {
            "range": {
                "start": lsp_position(source, replacement.start),
                "end": lsp_position(source, replacement.end),
            },
            "newText": insert_text,
        }
    });
    if let Some(detail) = detail {
        item["detail"] = json!(detail);
    }
    Some(item)
}

fn completion_candidate_item(
    candidate: CompletionCandidate,
    source: &str,
    offset: usize,
    word_start: usize,
) -> Option<Value> {
    let replacement = candidate
        .replacement
        .unwrap_or_else(|| Span::new(word_start, offset));
    if replacement.end != offset {
        return None;
    }
    let insertion = candidate.insert_text.as_deref().unwrap_or(&candidate.label);
    let mut item = completion_item(
        &candidate.label,
        insertion,
        candidate.kind,
        candidate.detail.as_deref(),
        replacement,
        source,
    )?;
    if candidate.insert_text_format == CompletionInsertTextFormat::Snippet {
        item["insertTextFormat"] = json!(2);
    }
    Some(item)
}

fn push_unique_completion(items: &mut Vec<Value>, labels: &mut BTreeSet<String>, item: Value) {
    if items.len() >= MAX_COMPLETION_ITEMS {
        return;
    }
    if let Some(label) = item["label"].as_str()
        && labels.insert(label.to_owned())
    {
        items.push(item);
    }
}

fn keyword_completion_items<'a>(
    text: &str,
    offset: usize,
    keywords: impl Iterator<Item = &'a str>,
) -> Vec<Value> {
    let (word_start, prefix) = completion_prefix(text, offset);
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Vec::new();
    }
    keywords
        .filter(|keyword| keyword.starts_with(prefix))
        .take(MAX_COMPLETION_ITEMS)
        .filter_map(|keyword| {
            completion_item(
                keyword,
                keyword,
                CompletionItemKind::Keyword,
                None,
                Span::new(word_start, offset),
                text,
            )
        })
        .collect()
}

fn lsp_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) -> Value {
    let span = diagnostic.span.unwrap_or(Span::new(0, 0));
    let severity = match diagnostic.severity {
        langmeld_core::Severity::Error => 1,
        langmeld_core::Severity::Warning => 2,
        langmeld_core::Severity::Info => 3,
    };
    json!({
        "range": {
            "start": lsp_position(&source.text, span.start),
            "end": lsp_position(&source.text, span.end),
        },
        "severity": severity,
        "code": diagnostic.code,
        "source": "langmeld",
        "message": diagnostic.message,
    })
}

fn apply_content_changes(current: &str, changes: &[Value]) -> Option<String> {
    let mut text = current.to_owned();
    for change in changes {
        let replacement = change.get("text")?.as_str()?;
        let Some(range) = change.get("range").filter(|range| !range.is_null()) else {
            if replacement.len() > MAX_DOCUMENT_BYTES {
                return None;
            }
            text.clear();
            text.push_str(replacement);
            continue;
        };
        let start_line = u32::try_from(range.pointer("/start/line")?.as_u64()?).ok()?;
        let start_character = u32::try_from(range.pointer("/start/character")?.as_u64()?).ok()?;
        let end_line = u32::try_from(range.pointer("/end/line")?.as_u64()?).ok()?;
        let end_character = u32::try_from(range.pointer("/end/character")?.as_u64()?).ok()?;
        let start = offset_at_position(&text, start_line, start_character)?;
        let end = offset_at_position(&text, end_line, end_character)?;
        if start > end {
            return None;
        }
        let result_len = text
            .len()
            .checked_sub(end - start)?
            .checked_add(replacement.len())?;
        if result_len > MAX_DOCUMENT_BYTES {
            return None;
        }
        if let Some(length) = change.get("rangeLength").filter(|length| !length.is_null()) {
            let length = usize::try_from(length.as_u64()?).ok()?;
            if length != text[start..end].encode_utf16().count() {
                return None;
            }
        }
        text.replace_range(start..end, replacement);
        if text.len() > MAX_DOCUMENT_BYTES {
            return None;
        }
    }
    Some(text)
}

fn offset_at_position(source: &str, target_line: u32, target_character: u32) -> Option<usize> {
    let (line_start, line_end) = line_bounds(source, target_line)?;
    let content = &source[line_start..line_end];
    let mut character = 0_u32;
    for (offset, value) in content.char_indices() {
        if character == target_character {
            return Some(line_start + offset);
        }
        let units = u32::try_from(value.len_utf16()).ok()?;
        let next = character.checked_add(units)?;
        if target_character < next {
            return None;
        }
        character = next;
    }
    (character == target_character).then_some(line_end)
}

fn lsp_position(text: &str, offset: usize) -> Value {
    let mut boundary = offset.min(text.len());
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let (line, line_start) = position_line_start(text, boundary);
    let line_start = &text[line_start..boundary];
    json!({
        "line": line,
        "character": line_start.encode_utf16().count(),
    })
}

fn line_bounds(source: &str, target_line: u32) -> Option<(usize, usize)> {
    let mut line = 0_u32;
    let mut start = 0;
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            if line == target_line {
                return Some((start, index));
            }
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            line = line.checked_add(1)?;
            start = index + 1;
        }
        index += 1;
    }
    (line == target_line).then_some((start, source.len()))
}

fn position_line_start(source: &str, offset: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut line = 0;
    let mut start = 0;
    let mut index = 0;
    while index < offset {
        if matches!(bytes[index], b'\r' | b'\n') {
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') && index + 1 < offset {
                index += 1;
            }
            line += 1;
            start = index + 1;
        }
        index += 1;
    }
    (line, start)
}

fn declaration_symbol_kind<'language>(
    language: &'language ValidatedLanguageSpec,
    nodes: &[AstNode],
    span: Span,
) -> Option<&'language str> {
    for node in nodes {
        if let Some(symbol) = language
            .construct(&node.construct)
            .and_then(|construct| construct.symbol.as_ref())
            && node
                .fields
                .get(&symbol.name_field)
                .is_some_and(|field| field.span == span)
        {
            return Some(&symbol.kind);
        }
        if let Some(kind) = declaration_symbol_kind(language, &node.children, span) {
            return Some(kind);
        }
    }
    None
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut total_header_bytes = 0_usize;
    loop {
        let mut line = Vec::new();
        let read = (&mut *reader)
            .take((MAX_HEADER_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            if total_header_bytes == 0 {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EOF inside LSP headers",
            ));
        }
        total_header_bytes = total_header_bytes.saturating_add(read);
        if total_header_bytes > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP headers exceed the size limit",
            ));
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let header = std::str::from_utf8(&line)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let Some((name, value)) = header.trim_end().split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed LSP header",
            ));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            if content_length.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate Content-Length header",
                ));
            }
            let length = value
                .trim()
                .parse::<usize>()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            if length > MAX_MESSAGE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "LSP message exceeds the size limit",
                ));
            }
            content_length = Some(length);
        }
    }

    let length = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor, time::SystemTime};

    fn project_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "langmeld lsp project reload-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("language/constructs"))
            .expect("project definition directory is created");
        fs::create_dir_all(root.join("src")).expect("project source directory is created");
        fs::write(
            root.join("langmeld.json"),
            r#"{"schema_version":1,"language":"language/language.lm","source_roots":["src"]}"#,
        )
        .expect("project manifest is written");
        fs::write(
            root.join("language/language.lm"),
            "language ReloadDemo {\n  version: \"0.1.0\"\n  root: program\n  source_extension: \".demo\"\n}\n",
        )
        .expect("language definition is written");
        write_project_constructs(&root, "print");
        fs::write(
            root.join("src/main.demo"),
            "program {\nprint \"project source\"\n}\n",
        )
        .expect("project source is written");
        root
    }

    fn write_project_constructs(root: &Path, keyword: &str) {
        fs::write(
            root.join("language/constructs/program.lm"),
            format!(
                "constructs {{\n  construct program {{\n    syntax {{ default: \"program {{\" }}\n    block: [statement]\n  }}\n  construct statement {{\n    syntax {{ default: \"{keyword} {{message}}\" }}\n    fields {{ message: text required }}\n  }}\n}}\n"
            ),
        )
        .expect("construct definition is written");
    }

    fn project_server(root: &Path) -> Server {
        let manifest = root.join("langmeld.json");
        let snapshot = load_project(&manifest).expect("project snapshot loads");
        let parser = configure_language(snapshot.language.clone()).expect("parser configures");
        LanguageServerConfig::new(parser)
            .with_project_snapshot(manifest, snapshot)
            .into_server()
            .expect("project server configures")
    }

    fn definition_project_server(root: &Path) -> Server {
        let root_path = root.join("language/language.lm");
        let language = langmeld_definition_language().expect("definition grammar validates");
        let parser = configure_language(language).expect("definition parser configures");
        LanguageServerConfig::new(parser)
            .with_definition_root(root_path)
            .into_server()
            .expect("definition project server configures")
    }

    fn add_definition_import(root: &Path, version: &str) -> PathBuf {
        let definitions = root.join("language");
        fs::create_dir_all(definitions.join("extensions"))
            .expect("definition extension directory is created");
        let root_path = definitions.join("language.lm");
        fs::write(
            &root_path,
            "language ReloadDemo {\n  version: \"0.1.0\"\n  root: program\n  source_extension: \".demo\"\n  import: \"extensions/extra.lm\"\n}\n",
        )
        .expect("root definition imports its extension");
        let extension_path = definitions.join("extensions/extra.lm");
        fs::write(&extension_path, extension_definition(version))
            .expect("extension definition is written");
        extension_path
    }

    fn extension_definition(version: &str) -> String {
        format!(
            "language ReloadDemo {{\n  version: \"{version}\"\n  root: program\n  construct note {{\n    syntax {{ default: \"note\" }}\n  }}\n}}\n"
        )
    }

    fn change_document(uri: &str, version: i64, text: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }
        })
    }

    #[test]
    fn project_navigation_index_discards_partial_results_when_a_source_uri_is_unavailable() {
        let root = project_directory();
        fs::write(
            root.join("src/second.demo"),
            "program {\nprint \"second source\"\n}\n",
        )
        .expect("second project source is written");
        let manifest = root.join("langmeld.json");
        let mut snapshot = load_project(&manifest).expect("project snapshot loads");
        assert_eq!(snapshot.sources.len(), 2);
        snapshot.sources[1].name = root.join("src/removed.demo").display().to_string();
        let parser = configure_language(snapshot.language.clone()).expect("parser configures");

        let (documents, limited) = index_project_sources(&parser, &snapshot);

        assert!(documents.is_empty());
        assert!(limited);
        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    fn open_document(uri: &str, version: i64, text: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "reload-demo",
                "version": version,
                "text": text
            }}
        })
    }

    fn watched_file(path: &Path) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWatchedFiles",
            "params": { "changes": [{ "uri": path_to_file_uri(path).unwrap(), "type": 2 }] }
        })
    }

    #[test]
    fn file_uri_round_trips_spaces_and_unicode_paths() {
        let root = project_directory();
        let path = root.join("language/constructs/café notes.lm");
        fs::write(&path, "constructs {}\n").expect("test definition file is written");
        let uri = path_to_file_uri(&path).expect("path converts to a file URI");

        assert!(uri.contains("%20"));
        assert_eq!(file_uri_to_path(&uri).as_deref(), Some(path.as_path()));
        assert!(file_uri_to_path("https://example.test/file.lm").is_none());
        assert!(file_uri_to_path("file:///tmp/bad%2Gpath.lm").is_none());

        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    #[test]
    fn definition_project_diagnostics_follow_open_imported_files_and_clear_after_edits() {
        let root = project_directory();
        let extension_path = add_definition_import(&root, "0.1.0");
        let extension_uri = path_to_file_uri(&extension_path).unwrap();
        let root_uri = path_to_file_uri(&root.join("language/language.lm")).unwrap();
        let mut server = definition_project_server(&root);

        let notifications =
            server.handle(&open_document(&extension_uri, 1, "language ReloadDemo {"));
        let imported_diagnostics = diagnostics_for(&notifications, &extension_uri)
            .expect("imported module diagnostics are published");
        assert!(!imported_diagnostics.is_empty());
        assert!(imported_diagnostics.iter().all(|diagnostic| {
            diagnostic.pointer("/range/start/line").is_some()
                && diagnostic.pointer("/range/end/line").is_some()
        }));

        let valid_extension = extension_definition("0.1.0");
        let notifications = server.handle(&change_document(&extension_uri, 2, &valid_extension));
        assert_eq!(
            diagnostics_for(&notifications, &extension_uri)
                .expect("import diagnostics are refreshed")
                .len(),
            0
        );

        let incompatible_extension = extension_definition("0.2.0");
        let notifications =
            server.handle(&change_document(&extension_uri, 3, &incompatible_extension));
        let root_diagnostics = diagnostics_for(&notifications, &root_uri)
            .expect("unlocated cross-module errors are published at the project root");
        assert!(root_diagnostics.iter().any(|diagnostic| {
            diagnostic["message"].as_str().is_some_and(|message| {
                message.contains("different language name, version, or root")
            })
        }));
        assert_eq!(
            root_diagnostics[0].pointer("/range/start"),
            Some(&json!({ "line": 0, "character": 0 }))
        );
        assert_eq!(
            root_diagnostics[0].pointer("/range/end"),
            Some(&json!({ "line": 0, "character": 0 }))
        );

        let notifications = server.handle(&change_document(&extension_uri, 4, &valid_extension));
        assert_eq!(
            diagnostics_for(&notifications, &root_uri)
                .expect("stale root diagnostics are cleared")
                .len(),
            0
        );
        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    #[test]
    fn definition_project_watches_imports_and_revalidates_closed_files() {
        let root = project_directory();
        let extension_path = add_definition_import(&root, "0.1.0");
        let root_uri = path_to_file_uri(&root.join("language/language.lm")).unwrap();
        let mut server = definition_project_server(&root);
        server.handle(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": { "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": true }
            }}}
        }));
        let registrations = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }));
        let watchers =
            &registrations[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
        assert!(watchers.as_array().unwrap().iter().any(|watcher| {
            watcher["globPattern"]["pattern"] == "**/*.lm"
                && watcher["globPattern"]["baseUri"]
                    == path_to_file_uri(&root.join("language")).unwrap()
        }));

        fs::write(&extension_path, extension_definition("0.2.0"))
            .expect("incompatible extension is written");
        let notifications = server.handle(&watched_file(&extension_path));
        assert!(
            diagnostics_for(&notifications, &root_uri).is_some_and(|diagnostics| {
                diagnostics.iter().any(|diagnostic| {
                    diagnostic["message"].as_str().is_some_and(|message| {
                        message.contains("different language name, version, or root")
                    })
                })
            })
        );

        fs::write(&extension_path, extension_definition("0.1.0"))
            .expect("compatible extension is restored");
        let notifications = server.handle(&watched_file(&extension_path));
        assert_eq!(
            diagnostics_for(&notifications, &root_uri)
                .expect("watch event clears the project diagnostic")
                .len(),
            0
        );
        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    #[test]
    fn definition_project_rejects_import_cycles_root_escapes_and_oversized_modules() {
        let root = project_directory();
        let extension_path = add_definition_import(&root, "0.1.0");
        let extension_uri = path_to_file_uri(&extension_path).unwrap();
        let root_uri = path_to_file_uri(&root.join("language/language.lm")).unwrap();
        let mut server = definition_project_server(&root);

        let cycle = extension_definition("0.1.0").replace(
            "  construct note {",
            "  import: \"../language.lm\"\n  construct note {",
        );
        let notifications = server.handle(&open_document(&extension_uri, 1, &cycle));
        let root_diagnostics = diagnostics_for(&notifications, &root_uri)
            .expect("import-cycle diagnostic is published within the project");
        assert!(root_diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("import cycle"))
        }));

        let outside_path = root.join("outside.lm");
        fs::write(&outside_path, "language Outside {}\n").expect("outside file is written");
        let escaped = extension_definition("0.1.0").replace(
            "  construct note {",
            "  import: \"../../outside.lm\"\n  construct note {",
        );
        let notifications = server.handle(&change_document(&extension_uri, 2, &escaped));
        let root_diagnostics = diagnostics_for(&notifications, &root_uri)
            .expect("root-escape diagnostic is kept within the project");
        assert!(root_diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("escapes the language directory"))
        }));
        assert!(
            diagnostics_for(&notifications, &path_to_file_uri(&outside_path).unwrap()).is_none()
        );

        let oversized = format!(
            "{}{}",
            extension_definition("0.1.0"),
            " ".repeat(DefinitionLimits::default().max_source_bytes)
        );
        server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": extension_uri } }
        }));
        fs::write(&extension_path, &oversized).expect("oversized module is written");
        let notifications = server.handle(&watched_file(&extension_path));
        let extension_diagnostics = diagnostics_for(&notifications, &extension_uri)
            .expect("resource-limit diagnostic is published for the imported file");
        assert!(
            extension_diagnostics.iter().any(|diagnostic| {
                diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("total parsing limit"))
            }),
            "unexpected diagnostics: {extension_diagnostics:?}"
        );

        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    fn diagnostics_for<'a>(messages: &'a [Value], uri: &str) -> Option<&'a Vec<Value>> {
        messages
            .iter()
            .find(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message.pointer("/params/uri").and_then(Value::as_str) == Some(uri)
            })
            .and_then(|message| message.pointer("/params/diagnostics"))
            .and_then(Value::as_array)
    }

    #[test]
    fn watched_project_reload_updates_open_documents_and_keeps_last_valid_snapshot() {
        let root = project_directory();
        let construct_path = root.join("language/constructs/program.lm");
        let mut server = project_server(&root);

        let initialized = server.handle(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": { "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": true }
            }}}
        }));
        assert_eq!(
            initialized[0]["result"]["capabilities"]["textDocumentSync"]["change"],
            2
        );
        let registration = server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }));
        assert_eq!(registration.len(), 1);
        assert_eq!(registration[0]["method"], "client/registerCapability");
        let watchers =
            &registration[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
        assert!(
            watchers
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| { watcher["globPattern"]["pattern"] == "**/*.lm" })
        );
        assert!(
            watchers
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| { watcher["globPattern"]["pattern"] == "langmeld.json" })
        );
        assert!(
            watchers
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| { watcher["globPattern"]["pattern"] == "src/**" })
        );
        assert_eq!(
            watchers[0]["globPattern"]["baseUri"],
            path_to_file_uri(&root).unwrap()
        );
        assert!(
            watchers[0]["globPattern"]["baseUri"]
                .as_str()
                .unwrap()
                .contains("%20")
        );

        let old_uri = "file:///reload-old.demo";
        let old_open = server.handle(&open_document(
            old_uri,
            1,
            "program {\nprint \"old syntax\"\n}\n",
        ));
        assert_eq!(diagnostics_for(&old_open, old_uri).unwrap().len(), 0);

        write_project_constructs(&root, "say");
        let reloaded = server.handle(&watched_file(&construct_path));
        let old_diagnostics = diagnostics_for(&reloaded, old_uri).unwrap();
        assert!(old_diagnostics.iter().any(|item| item["severity"] == 1));
        let new_uri = "file:///reload-new.demo";
        let new_open = server.handle(&open_document(
            new_uri,
            1,
            "program {\nsay \"new syntax\"\n}\n",
        ));
        assert_eq!(diagnostics_for(&new_open, new_uri).unwrap().len(), 0);

        let project_source = root.join("src/main.demo");
        fs::write(&project_source, "program {\nsay \"updated source\"\n}\n")
            .expect("project source edit is written");
        server.handle(&watched_file(&project_source));
        assert_eq!(
            server.project.as_ref().unwrap().snapshot.sources[0].text,
            "program {\nsay \"updated source\"\n}\n"
        );
        let project_source_uri = path_to_file_uri(&project_source).unwrap();
        assert!(
            server
                .project
                .as_ref()
                .unwrap()
                .source_documents
                .get(&project_source_uri)
                .unwrap()
                .parsed
                .diagnostics
                .is_empty()
        );

        fs::create_dir_all(root.join("programs")).expect("replacement source root is created");
        fs::write(
            root.join("programs/next.demo"),
            "program {\nsay \"new root\"\n}\n",
        )
        .expect("replacement source is written");
        fs::write(
            root.join("langmeld.json"),
            r#"{"schema_version":1,"language":"language/language.lm","source_roots":["programs"]}"#,
        )
        .expect("updated project roots are written");
        let root_change = server.handle(&watched_file(&root.join("langmeld.json")));
        assert!(root_change.iter().any(|notification| {
            notification["method"] == "client/unregisterCapability"
                && notification["id"] == PROJECT_WATCHER_REFRESH_ID
        }));
        let registration = server.handle(&json!({
            "jsonrpc": "2.0",
            "id": PROJECT_WATCHER_REFRESH_ID,
            "result": null
        }));
        let watchers =
            &registration[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
        assert!(
            watchers
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| { watcher["globPattern"]["pattern"] == "programs/**" })
        );
        assert!(
            !watchers
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| { watcher["globPattern"]["pattern"] == "src/**" })
        );

        fs::write(&construct_path, "constructs {\n").expect("invalid edit is written");
        let failed = server.handle(&watched_file(&construct_path));
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0]["method"], "window/logMessage");
        assert_eq!(failed[0]["params"]["type"], 2);
        assert!(
            failed[0]["params"]["message"]
                .as_str()
                .unwrap()
                .contains("keeping the last valid language")
        );

        let retained_uri = "file:///reload-retained.demo";
        let retained = server.handle(&open_document(
            retained_uri,
            1,
            "program {\nsay \"still valid\"\n}\n",
        ));
        assert_eq!(diagnostics_for(&retained, retained_uri).unwrap().len(), 0);
        assert!(old_diagnostics.iter().any(|item| item["severity"] == 1));

        fs::remove_dir_all(root).expect("temporary project is removed");
    }

    #[test]
    fn lsp_positions_count_utf16_code_units() {
        assert_eq!(
            lsp_position("😀x\nπ", 4),
            json!({ "line": 0, "character": 2 })
        );
        assert_eq!(
            lsp_position("😀x\nπ", 7),
            json!({ "line": 1, "character": 0 })
        );
    }

    #[test]
    fn content_changes_apply_utf16_ranges_sequentially() {
        let updated = apply_content_changes(
            "a😀b\r\nnext",
            &[json!({
                "range": {
                    "start": { "line": 0, "character": 1 },
                    "end": { "line": 0, "character": 3 }
                },
                "rangeLength": 2,
                "text": "x"
            })],
        );
        assert_eq!(updated.as_deref(), Some("axb\r\nnext"));
    }

    #[test]
    fn content_changes_reject_invalid_ranges_without_partial_output() {
        let invalid = apply_content_changes(
            "a😀b",
            &[json!({
                "range": {
                    "start": { "line": 0, "character": 2 },
                    "end": { "line": 0, "character": 3 }
                },
                "text": "x"
            })],
        );
        assert_eq!(invalid, None);

        let invalid_length = apply_content_changes(
            "abc",
            &[json!({
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "rangeLength": 1,
                "text": "x"
            })],
        );
        assert_eq!(invalid_length, None);
    }

    #[test]
    fn content_changes_replace_full_document_and_enforce_size_limit() {
        let updated = apply_content_changes("old", &[json!({ "text": "new" })]);
        assert_eq!(updated.as_deref(), Some("new"));
        let too_large = "x".repeat(MAX_DOCUMENT_BYTES + 1);
        assert_eq!(
            apply_content_changes("old", &[json!({ "text": too_large })]),
            None
        );
        assert_eq!(offset_at_position("a\n", 1, 0), Some(2));
        assert_eq!(offset_at_position("a", 1, 0), None);
    }

    #[test]
    fn keyword_completion_filters_prefix_and_uses_utf16_edit_ranges() {
        let text = "😀 λ";
        let items =
            keyword_completion_items(text, text.len(), ["if", "λambda", "return"].into_iter());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["label"], "λambda");
        assert_eq!(
            items[0]["textEdit"]["range"]["start"],
            json!({ "line": 0, "character": 3 })
        );
        assert_eq!(
            items[0]["textEdit"]["range"]["end"],
            json!({ "line": 0, "character": 4 })
        );
    }

    #[test]
    fn keyword_completion_is_empty_without_keywords_and_initialization_is_gated() {
        let items = keyword_completion_items("", 0, std::iter::empty());
        assert!(items.is_empty());
        let response = initialize_response(json!(1), false, false, false, false, false, false);
        assert!(
            response["result"]["capabilities"]
                .get("completionProvider")
                .is_none()
        );
        assert!(
            response["result"]["capabilities"]
                .get("definitionProvider")
                .is_none()
        );
        assert!(
            response["result"]["capabilities"]
                .get("documentFormattingProvider")
                .is_none()
        );
        assert!(
            response["result"]["capabilities"]
                .get("referencesProvider")
                .is_none()
        );
        assert!(
            response["result"]["capabilities"]
                .get("renameProvider")
                .is_none()
        );
    }

    #[test]
    fn construct_pattern_snippets_expand_fields_and_skip_pattern_operators() {
        assert_eq!(
            construct_pattern_snippet(r#"paint {color} at \({x}, {y}\)"#),
            Some(r#"paint ${1:color} at (${2:x}, ${3:y})"#.to_owned())
        );
        assert_eq!(construct_pattern_snippet("if [{condition}]"), None);
        assert_eq!(
            construct_pattern_snippet("block {"),
            Some("block {".to_owned())
        );
    }

    #[test]
    fn parser_continuation_snippets_render_fields_literals_and_spacing() {
        let snippet = completion_continuation_snippet(&[
            CompletionPart::Literal("fn".to_owned()),
            CompletionPart::Capture("name".to_owned()),
            CompletionPart::Literal("(".to_owned()),
            CompletionPart::Capture("parameters".to_owned()),
            CompletionPart::Literal(")".to_owned()),
            CompletionPart::Literal("->".to_owned()),
            CompletionPart::Capture("result".to_owned()),
        ]);
        assert_eq!(
            snippet,
            Some("fn ${1:name}(${2:parameters}) -> ${3:result}".to_owned())
        );
    }

    #[test]
    fn parser_continuation_snippets_escape_snippet_metacharacters_and_bound_size() {
        assert_eq!(
            completion_continuation_snippet(&[
                CompletionPart::Capture("field$name".to_owned()),
                CompletionPart::Literal("}".to_owned()),
            ]),
            Some(r"${1:field\$name}\}".to_owned())
        );
        assert_eq!(
            completion_continuation_snippet(&[CompletionPart::Literal("x".repeat(4097))]),
            None
        );
    }

    #[test]
    fn embedded_server_runs_registered_custom_parser_and_analysis_callbacks() {
        use langmeld_core::{
            BehaviorStage, ConstructBuilder, Diagnostic, FieldKind, FieldSpec, LanguageBuilder,
            Span, SymbolSpec,
        };
        use langmeld_language::{
            CapturedValue, ConfiguredSemanticHookRegistration, CustomConstructMatch,
            CustomConstructParser, SemanticHookRegistration, Value as LangValue,
            WorkspaceDiagnostic, WorkspaceSymbolResolverRegistration,
            WorkspaceTypeAnalysisRegistration,
        };

        fn plain_semantic_hook(
            _: &Program,
            _: &langmeld_core::SourceFile,
            _: &mut Vec<Diagnostic>,
        ) {
        }

        let language = LanguageBuilder::new("Embedded")
            .version("0.1.0")
            .root("program")
            .behavior("semantic-check", BehaviorStage::SemanticAnalysis)
            .behavior("semantic-note", BehaviorStage::SemanticAnalysis)
            .behavior("resolve", BehaviorStage::SymbolResolution)
            .behavior("workspace-types", BehaviorStage::WorkspaceTypeAnalysis)
            .construct(
                ConstructBuilder::new("program")
                    .syntax("program {")
                    .block(["native"]),
            )
            .construct(
                ConstructBuilder::new("native")
                    .syntax_variant("native", "host syntax {name}")
                    .field(FieldSpec::required("name", FieldKind::Identifier))
                    .symbol(SymbolSpec::new("native", "name"))
                    .custom_parser("parse-native"),
            )
            .build()
            .expect("host language definition validates");
        let parser = PreparedLanguage::new(language.clone())
            .expect("syntax patterns compile")
            .with_custom_parsers([CustomConstructParser::new(
                "parse-native",
                |source, offset| {
                    let Some(rest) = source.text.get(offset..) else {
                        return Ok(None);
                    };
                    let line_end = rest.find('\n').unwrap_or(rest.len());
                    let line = rest[..line_end].trim_end_matches('\r');
                    let Some(name) = line.strip_prefix("native ") else {
                        return Ok(None);
                    };
                    let name_start = offset + "native ".len();
                    let end = offset + line.len();
                    Ok(Some(
                        CustomConstructMatch::new(Span::new(offset, end), "native").with_field(
                            "name",
                            CapturedValue::new(
                                LangValue::Identifier(name.to_owned()),
                                Span::new(name_start, end),
                            ),
                        ),
                    ))
                },
            )])
            .expect("custom parser registration matches the language");
        assert!(configure_language(language).is_err());

        let missing_hook = serve_configured(
            Cursor::new(Vec::<u8>::new()),
            Vec::<u8>::new(),
            LanguageServerConfig::new(parser.clone()),
        )
        .expect_err("missing declared behavior is rejected before input is read");
        assert!(missing_hook.contains("semantic-check"));
        let duplicate_hook = serve_configured(
            Cursor::new(Vec::<u8>::new()),
            Vec::<u8>::new(),
            LanguageServerConfig::new(parser.clone())
                .with_semantic_hooks([SemanticHookRegistration::new(
                    "semantic-note",
                    plain_semantic_hook,
                )])
                .with_configured_semantic_hooks([ConfiguredSemanticHookRegistration::new(
                    "semantic-note",
                    |_, _, _| {},
                )]),
        )
        .expect_err("duplicate behavior IDs are rejected");
        assert!(duplicate_hook.contains("registered more than once"));
        let undeclared_hook = serve_configured(
            Cursor::new(Vec::<u8>::new()),
            Vec::<u8>::new(),
            LanguageServerConfig::new(parser.clone()).with_semantic_hooks([
                SemanticHookRegistration::new("semantic-check", plain_semantic_hook),
                SemanticHookRegistration::new("semantic-note", plain_semantic_hook),
                SemanticHookRegistration::new("extra", plain_semantic_hook),
            ]),
        )
        .expect_err("undeclared behavior IDs are rejected");
        assert!(undeclared_hook.contains("does not declare"));

        let config = LanguageServerConfig::new(parser)
            .with_semantic_hooks([SemanticHookRegistration::new(
                "semantic-note",
                plain_semantic_hook,
            )])
            .with_symbol_resolvers([WorkspaceSymbolResolverRegistration::new(
                "resolve",
                Box::new(|_, _, analysis| {
                    analysis.diagnostics.push(WorkspaceDiagnostic {
                        source_index: 0,
                        source_id: None,
                        source_revision: None,
                        diagnostic: Diagnostic::error(
                            "HOST0002",
                            "host symbol resolver ran",
                            Span::new(10, 15),
                        ),
                    });
                }),
            )])
            .with_type_analysis_hooks([WorkspaceTypeAnalysisRegistration::new(
                "workspace-types",
                Box::new(|_, _, _, _, _, diagnostics| {
                    diagnostics.push(WorkspaceDiagnostic {
                        source_index: 0,
                        source_id: None,
                        source_revision: None,
                        diagnostic: Diagnostic::error(
                            "HOST0003",
                            "host type-analysis hook ran",
                            Span::new(10, 15),
                        ),
                    });
                }),
            )])
            .with_configured_semantic_hooks([ConfiguredSemanticHookRegistration::new(
                "semantic-check",
                |program, source, diagnostics| {
                    let has_native_node = program
                        .nodes
                        .first()
                        .and_then(|root| root.children.first())
                        .is_some_and(|node| node.construct == "native");
                    if has_native_node {
                        diagnostics.push(Diagnostic::error(
                            "HOST0001",
                            "host semantic callback ran",
                            Span::new(10, source.text.len().saturating_sub(2)),
                        ));
                    }
                },
            )])
            .with_completion_providers([CompletionProviderRegistration::new(|context| {
                let mut candidate = CompletionCandidate::new("host-value");
                candidate.kind = CompletionItemKind::Constant;
                candidate.detail = Some(format!(
                    "{}:{}:{}",
                    context.uri,
                    context.workspace.len(),
                    context.prefix
                ));
                let duplicate = CompletionCandidate::new("host-value");
                let oversized =
                    CompletionCandidate::new("x".repeat(MAX_COMPLETION_LABEL_BYTES + 1));
                let mut candidates = vec![candidate, duplicate, oversized];
                candidates.extend(
                    (0..MAX_COMPLETION_ITEMS + 10)
                        .map(|index| CompletionCandidate::new(format!("extra-{index}"))),
                );
                candidates
            })]);
        let requests = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": { "textDocument": {
                    "uri": "file:///embedded.demo",
                    "languageId": "embedded",
                    "version": 1,
                    "text": "program {\nnative node\n}\n"
                }}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": { "uri": "file:///embedded.demo" },
                    "position": { "line": 1, "character": 11 }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "textDocument/rename",
                "params": {
                    "textDocument": { "uri": "file:///embedded.demo" },
                    "position": { "line": 1, "character": 11 },
                    "newName": "renamed"
                }
            }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
            json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
        ];
        let mut input = Vec::new();
        for request in &requests {
            write_message(&mut input, request).expect("request framing succeeds");
        }
        let mut output = Vec::new();
        assert!(
            serve_configured(Cursor::new(input), &mut output, config)
                .expect("embedded server handles protocol requests")
        );
        let mut reader = Cursor::new(output);
        let mut responses = Vec::new();
        while let Some(message) = read_message(&mut reader).expect("response framing is valid") {
            responses.push(serde_json::from_slice::<Value>(&message).expect("response is JSON"));
        }
        let diagnostics = responses
            .iter()
            .find(|response| response["method"] == "textDocument/publishDiagnostics")
            .and_then(|response| response.pointer("/params/diagnostics"))
            .and_then(Value::as_array)
            .expect("host diagnostics are published");
        let completion = responses
            .iter()
            .find(|response| response["id"] == 4)
            .expect("host completion response exists");
        assert_eq!(completion["result"][0]["label"], "host-value");
        assert_eq!(
            completion["result"].as_array().unwrap().len(),
            MAX_COMPLETION_ITEMS
        );
        assert_eq!(
            completion["result"][0]["detail"],
            "file:///embedded.demo:1:node"
        );
        assert_eq!(
            completion["result"][0]["textEdit"]["range"]["start"],
            json!({ "line": 1, "character": 7 })
        );
        assert_eq!(
            responses
                .iter()
                .find(|response| response["id"] == 1)
                .unwrap()["result"]["capabilities"]["completionProvider"]["resolveProvider"],
            false
        );
        let initialize = responses
            .iter()
            .find(|response| response["id"] == 1)
            .expect("initialize response exists");
        assert!(
            initialize["result"]["capabilities"]
                .get("renameProvider")
                .is_none(),
            "host resolver callbacks do not advertise generic rename"
        );
        assert_eq!(
            responses
                .iter()
                .find(|response| response["id"] == 5)
                .expect("rename response exists")["result"],
            Value::Null
        );
        for code in ["HOST0001", "HOST0002", "HOST0003"] {
            assert!(
                diagnostics.iter().any(|item| item["code"] == code),
                "{code}"
            );
        }
    }

    #[test]
    fn framing_reads_a_message_and_detects_oversized_bodies() {
        let body = br#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let framed = format!(
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        );
        let mut reader = Cursor::new(framed.into_bytes());
        assert_eq!(read_message(&mut reader).unwrap().unwrap(), body);

        let mut oversized =
            Cursor::new(format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).into_bytes());
        assert!(read_message(&mut oversized).is_err());
    }

    #[test]
    fn framing_rejects_duplicate_content_length_headers() {
        let mut reader = Cursor::new(b"Content-Length: 0\r\ncontent-length: 0\r\n\r\n".to_vec());
        assert!(read_message(&mut reader).is_err());
    }
}
