//! A small LSP transport and diagnostic server for declarative Langmeld languages.

use std::{
    collections::{BTreeSet, HashMap},
    io::{self, BufRead, Read, Write},
};

use langmeld_core::{BehaviorStage, Diagnostic, SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{
    AstNode, BehaviorRegistrationError, BehaviorRegistrationIssue, ConfiguredLanguageParser,
    ConfiguredSemanticHook, ConfiguredSemanticHookRegistration, ParseResult, PreparedLanguage,
    Program, SemanticHook, SemanticHookRegistration, WorkspaceAnalysis,
    WorkspaceSymbolResolverRegistration, WorkspaceSymbolResolverRegistry,
    WorkspaceTypeAnalysisRegistration, WorkspaceTypeAnalysisRegistry,
    analyze_workspace_with_host_callbacks,
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
    semantic_hooks: Vec<SemanticHookRegistration>,
    configured_semantic_hooks: Vec<ConfiguredSemanticHookRegistration>,
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
            semantic_hooks: Vec::new(),
            configured_semantic_hooks: Vec::new(),
            symbol_resolvers: Vec::new(),
            type_analysis_hooks: Vec::new(),
            completion_providers: Vec::new(),
        }
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
        Ok(Server {
            language,
            parser: self.parser,
            semantic_hooks,
            configured_semantic_hooks,
            symbol_resolver_hooks,
            type_analysis_hooks,
            completion_providers: self
                .completion_providers
                .into_iter()
                .map(|registration| registration.provider)
                .collect(),
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
    symbol_resolver_hooks: Vec<langmeld_language::WorkspaceSymbolResolverHook>,
    type_analysis_hooks: Vec<langmeld_language::WorkspaceTypeAnalysisHook>,
    completion_providers: Vec<CompletionProvider>,
    documents: HashMap<String, OpenDocument>,
    shutdown: bool,
}

struct OpenDocument {
    version: i64,
    source: SourceFile,
    parsed: ParseResult,
}

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
            return Vec::new();
        };
        let id = message.get("id").cloned();
        match method {
            "initialize" => id
                .into_iter()
                .map(|id| {
                    initialize_response(
                        id,
                        self.supports_completion(),
                        self.supports_navigation(),
                        self.supports_hover(),
                    )
                })
                .collect(),
            "initialized" => Vec::new(),
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

    fn workspace_snapshot(&self) -> (Vec<String>, Vec<(&SourceFile, &langmeld_language::Program)>) {
        let mut uris: Vec<_> = self.documents.keys().cloned().collect();
        uris.sort();
        let files = uris
            .iter()
            .filter_map(|uri| {
                self.documents
                    .get(uri)
                    .map(|document| (&document.source, &document.parsed.program))
            })
            .collect();
        (uris, files)
    }

    fn publish_workspace_documents(&self) -> Vec<Value> {
        let (uris, files) = self.workspace_snapshot();
        let analysis = self.analyze_files(&files);
        uris.iter()
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
            .collect()
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

    fn supports_hover(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| !construct.fields.is_empty() || construct.symbol.is_some())
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
                return hover_markdown(format!("`{} {}`\n\nDeclared symbol.", kind, name));
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
    supports_hover: bool,
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
    if supports_hover {
        capabilities["hoverProvider"] = json!(true);
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
    use std::io::Cursor;

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
        let response = initialize_response(json!(1), false, false, false);
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
                .get("referencesProvider")
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
    fn embedded_server_runs_registered_custom_parser_and_analysis_callbacks() {
        use langmeld_core::{
            BehaviorStage, ConstructBuilder, Diagnostic, FieldKind, FieldSpec, LanguageBuilder,
            Span,
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
