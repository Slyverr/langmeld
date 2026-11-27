//! The language server: it takes the messages of the Language Server Protocol and
//! answers them, whatever carries them.

mod features;
mod names;
mod paths;
mod position;
mod project_file;
mod transport;

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use langmeld_meta::Editor;
use langmeld_syntax::{
    Diagnostic, FileId, Language, Parse, Severity, SourceFile, TextRange, TextSize,
};
use lsp_server::{ErrorCode, Notification, Request, RequestId, Response};
use lsp_types as lsp;
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::Served;
use crate::bind::{Bindings, Project, Resolver};
use crate::project::{DiskFiles, FileProvider};
use features::View;
use position::Encoding;

pub use lsp_server::Message;
pub use project_file::ProjectFileKind;
pub use transport::{StdioTransport, Transport, serve, serve_with};

/// The most files of a workspace the server reads.
const WORKSPACE: usize = 10_000;

/// How many more files than the workspace's the project may hold: the documents
/// that are open, and files that are imported from elsewhere.
const EXTRA_FILES: usize = 1_000;

/// What a language adds to its server. Every method has a default that adds nothing.
///
/// A method that panics does not stop the server: the panic is logged to the editor and
/// the method counts as having added nothing.
pub trait LanguageHooks {
    /// The language's own checks of a document, shown with the problems of the
    /// parser. Problems the parser also reports are shown once. The name of the file is
    /// its path if the document's address names a file, and otherwise the address.
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        let _ = (file, parse);
        Vec::new()
    }

    /// Called when the editor closes the document called `name`, as its
    /// [`SourceFile::name`] has it: its path if the address names a file, and otherwise the
    /// address. The language can forget what it keeps about it.
    fn closed(&self, name: &str) {
        let _ = name;
    }

    /// Whether [`definition`](Self::definition) finds anything.
    fn provides_definitions(&self) -> bool {
        false
    }

    /// Where, in the same file, what is named at `offset` is declared. This is asked
    /// before the names the `bind` rules know.
    fn definition(&self, file: &SourceFile, parse: &Parse, offset: TextSize) -> Option<TextRange> {
        let _ = (file, parse, offset);
        None
    }

    /// Words of the language's own to offer where `offset` is, besides the keywords the
    /// grammar allows there and the names that are visible. They replace the word being
    /// written, as keywords do.
    fn completions(&self, file: &SourceFile, parse: &Parse, offset: TextSize) -> Vec<String> {
        let _ = (file, parse, offset);
        Vec::new()
    }

    /// How names are resolved, where the declared rules are not enough. Giving a
    /// resolver turns the name features on, even for a language without `bind` rules.
    fn resolver(&self) -> Option<&dyn Resolver> {
        None
    }

    /// Checks of one file that need the names of the whole project, such as warnings
    /// about names that nothing uses ([`Bindings::unused`]).
    fn project_diagnostics(
        &self,
        project: &Project<'_>,
        bindings: &Bindings,
        file: FileId,
    ) -> Vec<Diagnostic> {
        let _ = (project, bindings, file);
        Vec::new()
    }

    /// What hovering at `offset` shows, as Markdown. `None` shows what is known about
    /// the name there: its kind, where it is defined, the line that introduces it and
    /// the documentation comments above.
    fn hover(&self, project: &Project<'_>, file: FileId, offset: TextSize) -> Option<String> {
        let _ = (project, file, offset);
        None
    }
}

/// The most text, in bytes, that a server parses for one document unless told otherwise.
/// Larger documents get one problem, `LM-E001`, and nothing else.
pub const DEFAULT_LIMIT: usize = 16 << 20;

/// A language with nothing to add.
impl LanguageHooks for () {}

/// A document the client has open.
struct Document {
    uri: lsp::Uri,
    path: String,
    version: i32,
    /// The problems last published.
    shown: Vec<lsp::Diagnostic>,
    /// For a project file of the language's users, which kind it is and its text.
    /// Other documents are files of the project.
    manifest: Option<(usize, SourceFile)>,
    /// Set while the text is too large to parse: its size, in bytes. Such a document
    /// is not a file of the project.
    oversized: Option<usize>,
    /// The text of an oversized document, kept so that the changes the client sends
    /// can be applied until it is small enough again.
    big: Option<SourceFile>,
}

/// The files that are there: those of a provider that were not deleted since.
struct Present<'a> {
    files: &'a dyn FileProvider,
    gone: &'a BTreeSet<String>,
}

impl FileProvider for Present<'_> {
    fn read(&self, path: &str) -> Option<String> {
        (!self.gone.contains(path))
            .then(|| self.files.read(path))
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Starting,
    Running,
    ShutDown,
}

/// A language server, one client at a time. Give it each message with
/// [`handle`](Self::handle), or serve a [`Transport`] with [`serve_with`].
///
/// Every message is answered before the next is read, so an answer is always about the
/// latest text the client sent. A change with a version older than the document's is
/// ignored.
///
/// For a language that binds names, the server reads the files of the client's
/// workspace and the files they import, so that names are known across them.
pub struct Server<'l, H = ()> {
    served: Served<'l>,
    language: &'l Language,
    editor: &'l Editor,
    hooks: H,
    state: State,
    encoding: Encoding,
    /// Every open document, and the other files the names are looked up in.
    project: Project<'l>,
    bindings: Bindings,
    open: BTreeMap<String, Document>,
    files: Box<dyn FileProvider>,
    /// The folders of the workspace.
    roots: Vec<String>,
    gone: BTreeSet<String>,
    manifests: Vec<ProjectFileKind>,
    /// Whether the client can be asked to report changes to files.
    watches: bool,
    exit: Option<u8>,
    limit: usize,
}

/// A request that cannot be answered, with why.
struct Refusal(ErrorCode, String);

fn parameters<P: DeserializeOwned>(value: Value) -> Result<P, Refusal> {
    serde_json::from_value(value)
        .map_err(|problem| Refusal(ErrorCode::InvalidParams, problem.to_string()))
}

fn answer(value: impl Serialize) -> Result<Value, Refusal> {
    serde_json::to_value(value)
        .map_err(|problem| Refusal(ErrorCode::InternalError, problem.to_string()))
}

fn notification(method: &str, params: impl Serialize) -> Message {
    Message::Notification(Notification::new(method.to_owned(), params))
}

pub(crate) fn log(text: String) -> Message {
    notification(
        lsp::notification::LogMessage::METHOD,
        lsp::LogMessageParams {
            typ: lsp::MessageType::ERROR,
            message: text,
        },
    )
}

/// The message of a panic, if it has one.
fn panic_text(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("an unknown failure")
}

/// Runs the code of the language, and keeps a panic in it from being more than a
/// message to the editor, which `fallback` stands in for.
fn guarded<T>(what: &str, logged: &mut Vec<Message>, fallback: T, run: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|panic| {
        let text = panic_text(&*panic);
        logged.push(log(format!("{what} failed: {text}")));
        fallback
    })
}

impl<'l, H: LanguageHooks> Server<'l, H> {
    /// A server for `language`, waiting for a client to initialize it. A language read
    /// from a file is served with the rules it carries; see [`Served`]. It reads the
    /// files of this computer.
    pub fn new(language: impl Into<Served<'l>>, hooks: H) -> Self {
        let served = language.into();
        Self {
            served,
            language: served.language(),
            editor: served.editor_rules(),
            hooks,
            state: State::Starting,
            encoding: Encoding::Utf16,
            project: Self::empty(served, DEFAULT_LIMIT),
            bindings: Bindings::default(),
            open: BTreeMap::new(),
            files: Box::new(DiskFiles),
            roots: Vec::new(),
            gone: BTreeSet::new(),
            manifests: Vec::new(),
            watches: false,
            exit: None,
            limit: DEFAULT_LIMIT,
        }
    }

    fn empty(served: Served<'l>, limit: usize) -> Project<'l> {
        Project::new(served)
            .largest_file(limit)
            .most_files(WORKSPACE + EXTRA_FILES)
    }

    /// Sets the most bytes of text that a document may have. A larger one is not parsed,
    /// and is reported as one problem instead; see [`DEFAULT_LIMIT`].
    #[must_use]
    pub fn limit(mut self, bytes: usize) -> Self {
        self.limit = bytes;
        self.project = Self::empty(self.served, bytes);
        self
    }

    /// Reads files that are not open from `files` instead of from this computer, as
    /// a server in a browser must.
    #[must_use]
    pub fn files(mut self, files: impl FileProvider + 'static) -> Self {
        self.files = Box::new(files);
        self
    }

    /// Checks documents named `name` as the project file of the language's users, read
    /// into `T`: problems are shown inside it, and its keys are completed.
    #[must_use]
    pub fn project_file<T: DeserializeOwned>(self, name: impl Into<String>) -> Self {
        self.project_file_of(ProjectFileKind::new::<T>(name))
    }

    /// Like [`project_file`](Self::project_file), for a kind made beforehand.
    #[must_use]
    pub fn project_file_of(mut self, kind: ProjectFileKind) -> Self {
        self.manifests.push(kind);
        self
    }

    /// The exit code the client asked for: 0 after a proper shutdown, 1 otherwise.
    /// `None` while the server should go on.
    pub fn exit_code(&self) -> Option<u8> {
        self.exit
    }

    /// Takes a message and gives the messages to send back. A failure while handling
    /// one is logged to the client, a request that failed gets an empty answer, and the
    /// server goes on.
    pub fn handle(&mut self, message: Message) -> Vec<Message> {
        match message {
            Message::Request(request) => self.request(request),
            Message::Notification(notification) => {
                let method = notification.method.clone();
                catch_unwind(AssertUnwindSafe(|| self.notified(notification))).unwrap_or_else(
                    |panic| {
                        let text = panic_text(&*panic);
                        vec![log(format!("`{method}` failed: {text}"))]
                    },
                )
            }
            // The answers to what the server asks change nothing.
            Message::Response(_) => Vec::new(),
        }
    }

    /// Like [`handle`](Self::handle), with messages as JSON text, as a browser
    /// passes them between a page and a worker with `postMessage`.
    pub fn handle_json(&mut self, message: &str) -> Vec<String> {
        let replies = match serde_json::from_str(message) {
            Ok(message) => self.handle(message),
            Err(problem) => {
                // Without an id to answer, the reply names none.
                return vec![format!(
                    r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{},"message":{}}}}}"#,
                    ErrorCode::ParseError as i32,
                    Value::String(problem.to_string())
                )];
            }
        };
        replies
            .iter()
            .filter_map(|reply| {
                let mut value = serde_json::to_value(reply).ok()?;
                value
                    .as_object_mut()?
                    .insert("jsonrpc".to_owned(), Value::String("2.0".to_owned()));
                Some(value.to_string())
            })
            .collect()
    }

    fn request(&mut self, request: Request) -> Vec<Message> {
        let Request { id, method, params } = request;
        match catch_unwind(AssertUnwindSafe(|| self.answered(&method, params))) {
            Ok(result) => vec![Message::Response(reply(id, result))],
            Err(panic) => {
                let text = format!("`{method}` failed: {}", panic_text(&*panic));
                // The request gets the answer of a document with nothing to say.
                vec![
                    log(text),
                    Message::Response(Response::new_ok(id, Value::Null)),
                ]
            }
        }
    }

    fn answered(&mut self, method: &str, params: Value) -> Result<Value, Refusal> {
        use lsp::request as ask;
        let refuse = |code, text: &str| Err(Refusal(code, text.to_owned()));
        match self.state {
            State::Starting if method != ask::Initialize::METHOD => {
                return refuse(ErrorCode::ServerNotInitialized, "initialize first");
            }
            State::ShutDown => {
                return refuse(ErrorCode::InvalidRequest, "the server is shutting down");
            }
            State::Running if method == ask::Initialize::METHOD => {
                return refuse(ErrorCode::InvalidRequest, "already initialized");
            }
            State::Starting | State::Running => {}
        }
        match method {
            ask::Initialize::METHOD => {
                let params: lsp::InitializeParams = parameters(params)?;
                answer(self.initialize(&params))
            }
            ask::Shutdown::METHOD => {
                self.state = State::ShutDown;
                Ok(Value::Null)
            }
            ask::DocumentSymbolRequest::METHOD => {
                let params: lsp::DocumentSymbolParams = parameters(params)?;
                answer(
                    self.view(&params.text_document.uri)
                        .map(|view| view.symbols()),
                )
            }
            ask::FoldingRangeRequest::METHOD => {
                let params: lsp::FoldingRangeParams = parameters(params)?;
                answer(
                    self.view(&params.text_document.uri)
                        .map(|view| view.folds()),
                )
            }
            ask::SelectionRangeRequest::METHOD => {
                let params: lsp::SelectionRangeParams = parameters(params)?;
                answer(self.view(&params.text_document.uri).map(|view| {
                    params
                        .positions
                        .iter()
                        .map(|&at| view.selection(position::offset(view.file, at, view.encoding)))
                        .collect::<Vec<_>>()
                }))
            }
            ask::SemanticTokensFullRequest::METHOD => {
                let params: lsp::SemanticTokensParams = parameters(params)?;
                answer(self.semantic_tokens(&params.text_document.uri))
            }
            ask::Completion::METHOD => {
                let params: lsp::CompletionParams = parameters(params)?;
                answer(self.completion(&params.text_document_position))
            }
            ask::GotoDefinition::METHOD => {
                let params: lsp::GotoDefinitionParams = parameters(params)?;
                answer(self.definition(&params.text_document_position_params))
            }
            ask::References::METHOD => {
                let params: lsp::ReferenceParams = parameters(params)?;
                let declaration = params.context.include_declaration;
                answer(self.references(&params.text_document_position, declaration))
            }
            ask::PrepareRenameRequest::METHOD => {
                let params: lsp::TextDocumentPositionParams = parameters(params)?;
                answer(self.prepare_rename(&params))
            }
            ask::Rename::METHOD => {
                let params: lsp::RenameParams = parameters(params)?;
                answer(self.rename(&params)?)
            }
            ask::HoverRequest::METHOD => {
                let params: lsp::HoverParams = parameters(params)?;
                answer(self.hover(&params.text_document_position_params))
            }
            ask::WorkspaceSymbolRequest::METHOD => {
                let params: lsp::WorkspaceSymbolParams = parameters(params)?;
                answer(self.workspace_symbols(&params.query))
            }
            ask::CodeActionRequest::METHOD => {
                let params: lsp::CodeActionParams = parameters(params)?;
                answer(self.code_actions(&params))
            }
            _ => Err(Refusal(
                ErrorCode::MethodNotFound,
                format!("`{method}` is not supported"),
            )),
        }
    }

    /// The open document at `uri` and its file, if it is written in the language and
    /// small enough to parse.
    fn view(&self, uri: &lsp::Uri) -> Option<View<'_>> {
        let document = self.open.get(uri.as_str())?;
        if document.manifest.is_some() || document.oversized.is_some() {
            return None;
        }
        let file = self.project.file_at(&document.path)?;
        Some(View {
            language: self.language,
            editor: self.editor,
            file: file.source(),
            parse: file.parse(),
            encoding: self.encoding,
        })
    }

    /// Whether names are looked up across files.
    fn binds(&self) -> bool {
        !self.project.rules().rules.is_empty() || self.hooks.resolver().is_some()
    }

    fn initialize(&mut self, params: &lsp::InitializeParams) -> lsp::InitializeResult {
        let offered = params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref());
        // Counting bytes costs nothing, so it is preferred.
        let bytes =
            offered.is_some_and(|offered| offered.contains(&lsp::PositionEncodingKind::UTF8));
        let (encoding, named) = if bytes {
            (Encoding::Utf8, lsp::PositionEncodingKind::UTF8)
        } else {
            (Encoding::Utf16, lsp::PositionEncodingKind::UTF16)
        };
        self.encoding = encoding;
        self.state = State::Running;
        self.watches = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files)
            .and_then(|watched| watched.dynamic_registration)
            == Some(true);
        let folders = params.workspace_folders.iter().flatten();
        let mut roots: Vec<_> = folders.map(|folder| folder.uri.as_str()).collect();
        #[allow(deprecated)] // Older clients name only one folder.
        if roots.is_empty() {
            roots.extend(params.root_uri.as_ref().map(|uri| uri.as_str()));
        }
        self.roots = roots
            .into_iter()
            .map(|root| paths::path_of(root).trim_end_matches('/').to_owned())
            .collect();
        if self.binds() {
            self.read_workspace(&mut Vec::new());
        }
        let legend = lsp::SemanticTokensLegend {
            token_types: features::TOKEN_TYPES.to_vec(),
            token_modifiers: features::TOKEN_MODIFIERS.to_vec(),
        };
        let binds = self.binds();
        let capabilities = lsp::ServerCapabilities {
            position_encoding: Some(named),
            text_document_sync: Some(lsp::TextDocumentSyncCapability::Kind(
                lsp::TextDocumentSyncKind::INCREMENTAL,
            )),
            document_symbol_provider: Some(lsp::OneOf::Left(true)),
            folding_range_provider: Some(lsp::FoldingRangeProviderCapability::Simple(true)),
            selection_range_provider: Some(lsp::SelectionRangeProviderCapability::Simple(true)),
            semantic_tokens_provider: Some(
                lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                    lsp::SemanticTokensOptions {
                        legend,
                        full: Some(lsp::SemanticTokensFullOptions::Bool(true)),
                        ..lsp::SemanticTokensOptions::default()
                    },
                ),
            ),
            completion_provider: Some(lsp::CompletionOptions::default()),
            definition_provider: (binds || self.hooks.provides_definitions())
                .then_some(lsp::OneOf::Left(true)),
            references_provider: binds.then_some(lsp::OneOf::Left(true)),
            rename_provider: binds.then(|| {
                lsp::OneOf::Right(lsp::RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
                })
            }),
            workspace_symbol_provider: binds.then_some(lsp::OneOf::Left(true)),
            hover_provider: binds.then_some(lsp::HoverProviderCapability::Simple(true)),
            code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
            ..lsp::ServerCapabilities::default()
        };
        lsp::InitializeResult {
            capabilities,
            server_info: Some(lsp::ServerInfo {
                name: format!("{} language server", self.language.name()),
                version: None,
            }),
        }
    }

    /// Reads the files of the language in the folders of the workspace.
    fn read_workspace(&mut self, logged: &mut Vec<Message>) {
        let extensions: Vec<&str> = self.language.extensions().collect();
        let found: Vec<String> = self
            .roots
            .iter()
            .flat_map(|root| self.files.list(root, &extensions))
            .take(WORKSPACE)
            .collect();
        for path in &found {
            if let Some(text) = self.files.read(path) {
                self.project.set(path, text);
            }
        }
        for path in &found {
            self.load(path);
        }
        self.rebind(logged);
    }

    /// Reads the file at `path`, if the project lacks it, and what it imports.
    fn load(&mut self, path: &str) {
        let present = Present {
            files: &*self.files,
            gone: &self.gone,
        };
        self.project.open(path, &present);
    }

    /// Whether `path` is a file of the language inside the workspace.
    fn in_workspace(&self, path: &str) -> bool {
        let extension = path.rsplit_once('.').map(|(_, extension)| extension);
        let ours = self
            .language
            .extensions()
            .any(|ours| Some(ours) == extension);
        ours && self.roots.iter().any(|root| {
            path.strip_prefix(root.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    /// Binds the names again, after reading the files that imports name and the
    /// project lacks.
    fn rebind(&mut self, logged: &mut Vec<Message>) {
        self.bind_again(logged);
        let before = self.project.files().count();
        for path in self.bindings.missing().to_vec() {
            self.load(&path);
        }
        if self.project.files().count() > before {
            self.bind_again(logged);
        }
    }

    fn bind_again(&mut self, logged: &mut Vec<Message>) {
        if !self.binds() {
            return;
        }
        // Cleared first, so that a failing resolver leaves no stale names.
        self.bindings = Bindings::default();
        let (hooks, project) = (&self.hooks, &self.project);
        let bound = guarded("the language's resolver", logged, None, || {
            Some(match hooks.resolver() {
                Some(resolver) => resolver.resolve(project),
                None => project.bind(),
            })
        });
        self.bindings = bound.unwrap_or_default();
    }

    fn notified(&mut self, message: Notification) -> Vec<Message> {
        use lsp::notification as told;
        if message.method == told::Exit::METHOD {
            self.exit = Some(u8::from(self.state != State::ShutDown));
            return Vec::new();
        }
        if self.state != State::Running {
            return Vec::new();
        }
        let method = message.method.as_str();
        let params = message.params;
        fn read<P: DeserializeOwned>(
            method: &str,
            params: Value,
            act: impl FnOnce(P) -> Vec<Message>,
        ) -> Vec<Message> {
            match serde_json::from_value(params) {
                Ok(params) => act(params),
                Err(problem) => vec![log(format!("`{method}`: {problem}"))],
            }
        }
        match method {
            told::Initialized::METHOD => self.watch(),
            told::DidOpenTextDocument::METHOD => {
                read(method, params, |params: lsp::DidOpenTextDocumentParams| {
                    let opened = params.text_document;
                    self.update(opened.uri, opened.version, opened.text)
                })
            }
            told::DidChangeTextDocument::METHOD => {
                read(method, params, |params| self.change(params))
            }
            told::DidCloseTextDocument::METHOD => {
                read(method, params, |params: lsp::DidCloseTextDocumentParams| {
                    self.close(params.text_document.uri)
                })
            }
            told::DidChangeWatchedFiles::METHOD => {
                read(method, params, |params| self.changed_on_disk(&params))
            }
            _ => Vec::new(),
        }
    }

    /// Asks the client to report changes to files of the language.
    fn watch(&self) -> Vec<Message> {
        if !self.watches || !self.binds() {
            return Vec::new();
        }
        let watchers = self
            .language
            .extensions()
            .map(|extension| lsp::FileSystemWatcher {
                glob_pattern: lsp::GlobPattern::String(format!("**/*.{extension}")),
                kind: None,
            })
            .collect();
        let options = lsp::DidChangeWatchedFilesRegistrationOptions { watchers };
        let registration = lsp::Registration {
            id: "files".to_owned(),
            method: lsp::notification::DidChangeWatchedFiles::METHOD.to_owned(),
            register_options: serde_json::to_value(options).ok(),
        };
        vec![Message::Request(Request::new(
            RequestId::from("watch".to_owned()),
            lsp::request::RegisterCapability::METHOD.to_owned(),
            lsp::RegistrationParams {
                registrations: vec![registration],
            },
        ))]
    }

    /// The text a document holds now, whatever sort of document it is.
    fn current<'a>(&'a self, document: &'a Document) -> Option<&'a SourceFile> {
        match &document.manifest {
            Some((_, file)) => Some(file),
            None => match &document.big {
                Some(file) => Some(file),
                None => self.project.file_at(&document.path).map(|f| f.source()),
            },
        }
    }

    fn change(&mut self, params: lsp::DidChangeTextDocumentParams) -> Vec<Message> {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let Some(document) = self.open.get(uri.as_str()) else {
            return vec![log(format!("`{}` is not open", uri.as_str()))];
        };
        // A change that is not newer than what the document holds is out of date: taking
        // it would show problems of the wrong text.
        if version <= document.version {
            return vec![log(format!(
                "ignored a change of `{}` to version {version}: it is at version {}",
                uri.as_str(),
                document.version
            ))];
        }
        let mut file = self.current(document).cloned();
        let mut text = file.as_ref().map(|file| file.text().to_owned());
        for change in params.content_changes {
            match (change.range, text.as_mut()) {
                // Without a range, the change is the whole new text.
                (None, _) => text = Some(change.text),
                (Some(range), Some(current)) => {
                    let Some(base) = &file else { continue };
                    let start = position::offset(base, range.start, self.encoding);
                    let end = position::offset(base, range.end, self.encoding).max(start);
                    // The offsets are on character boundaries, so this cannot panic.
                    if current.is_char_boundary(usize::from(start))
                        && current.is_char_boundary(usize::from(end))
                    {
                        current.replace_range(usize::from(start)..usize::from(end), &change.text);
                    }
                }
                (Some(_), None) => {
                    return vec![log(format!(
                        "`{}` was too large to keep, so a change of part of it is lost",
                        uri.as_str()
                    ))];
                }
            }
            // The next change is placed in the text the last one made.
            file = text
                .as_ref()
                .and_then(|text| SourceFile::new(FileId::new(0), uri.as_str(), text.clone()).ok());
        }
        let text = text.unwrap_or_default();
        self.update(uri, version, text)
    }

    /// Takes the new text of a document and publishes what changes.
    fn update(&mut self, uri: lsp::Uri, version: i32, text: String) -> Vec<Message> {
        let path = paths::path_of(uri.as_str());
        let name = path.rsplit('/').next().unwrap_or(&path);
        let kind = self.manifests.iter().position(|kind| kind.name() == name);
        let size = text.len();
        let (mut manifest, mut oversized, mut big) = (None, None, None);
        if let Some(kind) = kind {
            match SourceFile::new(FileId::new(u32::MAX), path.as_str(), text) {
                Ok(file) => manifest = Some((kind, file)),
                Err(_) => oversized = Some(size),
            }
        } else if size > self.limit {
            self.project.remove(&path);
            big = SourceFile::new(FileId::new(u32::MAX), path.as_str(), text).ok();
            oversized = Some(size);
        } else if self.project.set(&path, text).is_none() {
            self.project.remove(&path);
            oversized = Some(size);
        } else {
            self.gone.remove(&path);
        }
        let key = uri.as_str().to_owned();
        let shown = self.open.remove(&key).map(|document| document.shown);
        let document = Document {
            uri,
            path,
            version,
            shown: shown.unwrap_or_default(),
            manifest,
            oversized,
            big,
        };
        self.open.insert(key.clone(), document);
        self.publish_changes(Some(&key))
    }

    fn close(&mut self, uri: lsp::Uri) -> Vec<Message> {
        let mut messages = Vec::new();
        // The language knows the document by the name its file has.
        let name = (self.open.get(uri.as_str())).map_or_else(
            || paths::path_of(uri.as_str()),
            |document| document.path.clone(),
        );
        if catch_unwind(AssertUnwindSafe(|| self.hooks.closed(&name))).is_err() {
            messages.push(log("the language failed to close a document".to_owned()));
        }
        messages.push(publish(uri.clone(), None, Vec::new()));
        let Some(closed) = self.open.remove(uri.as_str()) else {
            return messages;
        };
        let still_open = self.open.values().any(|other| other.path == closed.path);
        if closed.manifest.is_none() && !still_open {
            // The file goes back to what the disk holds, if it matters to others.
            let kept = (self.binds() && self.in_workspace(&closed.path))
                .then(|| self.files.read(&closed.path))
                .flatten();
            match kept {
                Some(text) => {
                    self.project.set(&closed.path, text);
                }
                None => self.project.remove(&closed.path),
            }
        }
        if self.binds() {
            messages.extend(self.publish_changes(None));
        }
        messages
    }

    fn changed_on_disk(&mut self, params: &lsp::DidChangeWatchedFilesParams) -> Vec<Message> {
        if !self.binds() {
            return Vec::new();
        }
        for change in &params.changes {
            let path = paths::path_of(change.uri.as_str());
            // The editor's text of an open document wins.
            if self.open.values().any(|document| document.path == path) {
                continue;
            }
            if change.typ == lsp::FileChangeType::DELETED {
                // A folder takes its files with it.
                let folder = format!("{path}/");
                let inside: Vec<_> = self
                    .project
                    .files()
                    .map(|file| file.source().name().to_owned())
                    .filter(|name| *name == path || name.starts_with(&folder))
                    .collect();
                for name in inside {
                    self.project.remove(&name);
                    self.gone.insert(name);
                }
            } else {
                self.gone.remove(&path);
                let known = self.project.file_at(&path).is_some();
                if (known || self.in_workspace(&path))
                    && let Some(text) = self.files.read(&path)
                {
                    self.project.set(&path, text);
                }
            }
        }
        self.publish_changes(None)
    }

    /// The problem shown for a document too large to parse.
    fn too_large(&self, size: usize) -> lsp::Diagnostic {
        lsp::Diagnostic {
            range: lsp::Range::default(),
            severity: Some(lsp::DiagnosticSeverity::ERROR),
            code: Some(lsp::NumberOrString::String("LM-E001".to_owned())),
            source: Some(self.language.name().to_lowercase()),
            message: format!(
                "the file is too large for the language server: it has {size} bytes, and the \
                 most is {}, so no problems are shown for it\nTo fix this, split the file \
                 into smaller ones.",
                self.limit
            ),
            ..lsp::Diagnostic::default()
        }
    }

    /// The problems of an open document, in the order of its text.
    fn problems(&self, document: &Document, logged: &mut Vec<Message>) -> Vec<Diagnostic> {
        if let Some((kind, file)) = &document.manifest {
            let check = self.manifests.get(*kind);
            return check
                .and_then(|kind| kind.check(file.clone()))
                .into_iter()
                .collect();
        }
        let Some(file) = self.project.file_at(&document.path) else {
            return Vec::new();
        };
        let (source, parse) = (file.source(), file.parse());
        let id = source.id();
        let hooks = &self.hooks;
        let mut extra = guarded("the language's checks", logged, Vec::new(), || {
            hooks.diagnostics(source, parse)
        });
        if self.binds() {
            let bound = self.bindings.diagnostics().iter();
            extra.extend(bound.filter(|found| found.file == id).cloned());
        }
        extra.extend(guarded(
            "the language's project checks",
            logged,
            Vec::new(),
            || hooks.project_diagnostics(&self.project, &self.bindings, id),
        ));
        let mut found = parse.diagnostics.clone();
        let key = |d: &Diagnostic| (d.code, d.range, d.message.clone());
        let mut known: BTreeSet<_> = found
            .iter()
            .map(|d| {
                let (code, range, message) = key(d);
                (
                    code,
                    u32::from(range.start()),
                    u32::from(range.end()),
                    message,
                )
            })
            .collect();
        for more in extra {
            let (code, range, message) = key(&more);
            let at = (
                code,
                u32::from(range.start()),
                u32::from(range.end()),
                message,
            );
            if more.file == id && known.insert(at) {
                found.push(more);
            }
        }
        found.sort_by_key(|d| (d.range.start(), d.range.end(), d.code));
        found
    }

    /// The address and text of a file.
    fn place(&self, id: FileId) -> Option<(lsp::Uri, &SourceFile)> {
        let manifest = self
            .open
            .values()
            .filter_map(|document| {
                let (_, file) = document.manifest.as_ref()?;
                Some((document, file))
            })
            .find(|(_, file)| file.id() == id);
        if let Some((document, file)) = manifest {
            return Some((document.uri.clone(), file));
        }
        let file = self.project.file(id)?.source();
        let open = self
            .open
            .values()
            .find(|document| document.path == file.name());
        let uri = match open {
            Some(document) => document.uri.clone(),
            None => paths::uri_of(file.name())?,
        };
        Some((uri, file))
    }

    fn shown(&self, found: &Diagnostic) -> Option<lsp::Diagnostic> {
        let (_, file) = self.place(found.file)?;
        let mut message = found.message.clone();
        for note in &found.notes {
            message.push('\n');
            message.push_str(note);
        }
        let related: Vec<_> = found
            .labels
            .iter()
            .filter_map(|label| {
                let (uri, file) = self.place(label.file)?;
                Some(lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri,
                        range: position::range(file, label.range, self.encoding),
                    },
                    message: label.message.clone(),
                })
            })
            .collect();
        Some(lsp::Diagnostic {
            range: position::range(file, found.range, self.encoding),
            severity: Some(match found.severity {
                Severity::Warning => lsp::DiagnosticSeverity::WARNING,
                Severity::Info => lsp::DiagnosticSeverity::INFORMATION,
                _ => lsp::DiagnosticSeverity::ERROR,
            }),
            code: Some(lsp::NumberOrString::String(found.code.to_owned())),
            source: Some(self.language.name().to_lowercase()),
            message,
            related_information: (!related.is_empty()).then_some(related),
            ..lsp::Diagnostic::default()
        })
    }

    /// What the client is shown for a document.
    fn shown_for(&self, document: &Document, logged: &mut Vec<Message>) -> Vec<lsp::Diagnostic> {
        if let Some(size) = document.oversized {
            return vec![self.too_large(size)];
        }
        self.problems(document, logged)
            .iter()
            .filter_map(|found| self.shown(found))
            .collect()
    }

    /// Binds the names again and publishes the problems of the document `changed`,
    /// and of every other open document whose problems differ.
    fn publish_changes(&mut self, changed: Option<&str>) -> Vec<Message> {
        let mut messages = Vec::new();
        self.rebind(&mut messages);
        // Without names, no file says anything about another.
        let others = self.binds().then(|| {
            self.open
                .keys()
                .map(String::as_str)
                .filter(|&key| Some(key) != changed)
        });
        let keys: Vec<String> = changed
            .into_iter()
            .chain(others.into_iter().flatten())
            .map(str::to_owned)
            .collect();
        for key in keys {
            let Some(document) = self.open.get(&key) else {
                continue;
            };
            let shown = self.shown_for(document, &mut messages);
            if Some(key.as_str()) != changed && document.shown == shown {
                continue;
            }
            if let Some(document) = self.open.get_mut(&key) {
                let uri = document.uri.clone();
                messages.push(publish(uri, Some(document.version), shown.clone()));
                document.shown = shown;
            }
        }
        messages
    }

    /// The addresses of the documents that are open.
    pub fn documents(&self) -> impl Iterator<Item = &str> {
        self.open.values().map(|document| document.uri.as_str())
    }
}

fn reply(id: RequestId, result: Result<Value, Refusal>) -> Response {
    match result {
        Ok(value) => Response::new_ok(id, value),
        Err(Refusal(code, text)) => Response::new_err(id, code as i32, text),
    }
}

fn publish(uri: lsp::Uri, version: Option<i32>, found: Vec<lsp::Diagnostic>) -> Message {
    notification(
        lsp::notification::PublishDiagnostics::METHOD,
        lsp::PublishDiagnosticsParams {
            uri,
            diagnostics: found,
            version,
        },
    )
}
