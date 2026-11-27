//! The language server: it takes the messages of the Language Server Protocol and
//! answers them, whatever carries them.

mod features;
mod position;
mod transport;

use std::collections::BTreeMap;
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

use features::View;
use position::Encoding;

pub use lsp_server::Message;
pub use transport::{StdioTransport, Transport, serve, serve_with};

/// What a language adds to its server. Every method has a default that adds nothing.
///
/// A method that panics does not stop the server: the panic is logged to the editor and
/// the method counts as having added nothing.
pub trait LanguageHooks {
    /// The language's own checks of a document, shown with the problems of the
    /// parser. Problems the parser also reports are shown once.
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        let _ = (file, parse);
        Vec::new()
    }

    /// Called when the editor closes the document called `name`, which is its address.
    /// The language can forget what it keeps about it.
    fn closed(&self, name: &str) {
        let _ = name;
    }

    /// Whether [`definition`](Self::definition) finds anything.
    fn provides_definitions(&self) -> bool {
        false
    }

    /// Where, in the same file, what is named at `offset` is declared.
    fn definition(&self, file: &SourceFile, parse: &Parse, offset: TextSize) -> Option<TextRange> {
        let _ = (file, parse, offset);
        None
    }

    /// Words of the language's own to offer where `offset` is, besides the keywords the
    /// grammar allows there. They replace the word being written, as keywords do.
    fn completions(&self, file: &SourceFile, parse: &Parse, offset: TextSize) -> Vec<String> {
        let _ = (file, parse, offset);
        Vec::new()
    }
}

/// The most text, in bytes, that a server parses for one document unless told otherwise.
/// Larger documents get one problem, `LM-E001`, and nothing else.
pub const DEFAULT_LIMIT: usize = 16 << 20;

/// A language with nothing to add.
impl LanguageHooks for () {}

struct Document {
    uri: lsp::Uri,
    version: i32,
    file: SourceFile,
    /// Missing while the text is over the limit.
    parse: Option<Parse>,
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
pub struct Server<'l, H = ()> {
    language: &'l Language,
    editor: &'l Editor,
    hooks: H,
    state: State,
    encoding: Encoding,
    documents: BTreeMap<String, Document>,
    opened: u32,
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

impl<'l, H: LanguageHooks> Server<'l, H> {
    /// A server for `language`, waiting for a client to initialize it. `editor` says
    /// what the language tells editors, as `Loaded::editor` has it.
    pub fn new(language: &'l Language, editor: &'l Editor, hooks: H) -> Self {
        Self {
            language,
            editor,
            hooks,
            state: State::Starting,
            encoding: Encoding::Utf16,
            documents: BTreeMap::new(),
            opened: 0,
            exit: None,
            limit: DEFAULT_LIMIT,
        }
    }

    /// Sets the most bytes of text that a document may have. A larger one is not parsed,
    /// and is reported as one problem instead; see [`DEFAULT_LIMIT`].
    #[must_use]
    pub fn limit(mut self, bytes: usize) -> Self {
        self.limit = bytes;
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
            // The server asks the client nothing.
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
                answer(
                    self.view(&params.text_document.uri)
                        .map(|view| lsp::SemanticTokens {
                            result_id: None,
                            data: view.semantic_tokens(),
                        }),
                )
            }
            ask::Completion::METHOD => {
                let params: lsp::CompletionParams = parameters(params)?;
                let at = params.text_document_position;
                answer(self.view(&at.text_document.uri).map(|view| {
                    let offset = position::offset(view.file, at.position, view.encoding);
                    let mut items = view.completions(offset);
                    let extra = self.hooks.completions(view.file, view.parse, offset);
                    items.extend(view.words(offset, extra));
                    items
                }))
            }
            ask::GotoDefinition::METHOD => {
                let params: lsp::GotoDefinitionParams = parameters(params)?;
                let at = params.text_document_position_params;
                let uri = at.text_document.uri;
                answer(self.view(&uri).and_then(|view| {
                    let offset = position::offset(view.file, at.position, view.encoding);
                    let found = self.hooks.definition(view.file, view.parse, offset)?;
                    Some(lsp::Location {
                        uri: uri.clone(),
                        range: position::range(view.file, found, view.encoding),
                    })
                }))
            }
            _ => Err(Refusal(
                ErrorCode::MethodNotFound,
                format!("`{method}` is not supported"),
            )),
        }
    }

    fn view(&self, uri: &lsp::Uri) -> Option<View<'_>> {
        let document = self.documents.get(uri.as_str())?;
        Some(View {
            language: self.language,
            editor: self.editor,
            file: &document.file,
            parse: document.parse.as_ref()?,
            encoding: self.encoding,
        })
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
        let legend = lsp::SemanticTokensLegend {
            token_types: features::TOKEN_TYPES.to_vec(),
            token_modifiers: features::TOKEN_MODIFIERS.to_vec(),
        };
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
            definition_provider: self
                .hooks
                .provides_definitions()
                .then_some(lsp::OneOf::Left(true)),
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
        let bad = |problem: serde_json::Error| vec![log(format!("`{method}`: {problem}"))];
        match method {
            told::DidOpenTextDocument::METHOD => {
                match serde_json::from_value::<lsp::DidOpenTextDocumentParams>(message.params) {
                    Ok(params) => {
                        let opened = params.text_document;
                        self.update(opened.uri, opened.version, opened.text)
                    }
                    Err(problem) => bad(problem),
                }
            }
            told::DidChangeTextDocument::METHOD => {
                match serde_json::from_value::<lsp::DidChangeTextDocumentParams>(message.params) {
                    Ok(params) => self.change(params),
                    Err(problem) => bad(problem),
                }
            }
            told::DidCloseTextDocument::METHOD => {
                match serde_json::from_value::<lsp::DidCloseTextDocumentParams>(message.params) {
                    Ok(params) => {
                        let uri = params.text_document.uri;
                        self.documents.remove(uri.as_str());
                        let mut replies = Vec::new();
                        if catch_unwind(AssertUnwindSafe(|| self.hooks.closed(uri.as_str())))
                            .is_err()
                        {
                            replies.push(log("the language failed to close a document".to_owned()));
                        }
                        replies.push(publish(uri, None, Vec::new()));
                        replies
                    }
                    Err(problem) => bad(problem),
                }
            }
            _ => Vec::new(),
        }
    }

    fn change(&mut self, params: lsp::DidChangeTextDocumentParams) -> Vec<Message> {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let Some(document) = self.documents.get(uri.as_str()) else {
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
        let mut file = document.file.clone();
        for change in params.content_changes {
            let text = match change.range {
                // Without a range, the change is the whole new text.
                None => change.text,
                Some(range) => {
                    let start = position::offset(&file, range.start, self.encoding);
                    let end = position::offset(&file, range.end, self.encoding).max(start);
                    let mut text = file.text().to_owned();
                    // The offsets are on character boundaries, so this cannot panic.
                    if let (true, true) = (
                        text.is_char_boundary(usize::from(start)),
                        text.is_char_boundary(usize::from(end)),
                    ) {
                        text.replace_range(usize::from(start)..usize::from(end), &change.text);
                    }
                    text
                }
            };
            let size = text.len();
            file = match SourceFile::new(file.id(), file.name(), text) {
                Ok(file) => file,
                Err(_) => return self.discard(uri, version, size),
            };
        }
        let text = file.text().to_owned();
        self.update(uri, version, text)
    }

    /// Forgets a document whose text cannot be addressed, and says why.
    fn discard(&mut self, uri: lsp::Uri, version: i32, size: usize) -> Vec<Message> {
        self.documents.remove(uri.as_str());
        self.too_large(uri, version, size)
    }

    /// Tells the editor that the text is too large to serve, in the place of problems.
    fn too_large(&self, uri: lsp::Uri, version: i32, size: usize) -> Vec<Message> {
        let found = lsp::Diagnostic {
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
        };
        vec![publish(uri, Some(version), vec![found])]
    }

    /// Runs the checks of the language, and keeps a panic in them from being more than
    /// a message to the editor.
    fn checks(
        &self,
        file: &SourceFile,
        parse: &Parse,
        logged: &mut Vec<Message>,
    ) -> Vec<Diagnostic> {
        catch_unwind(AssertUnwindSafe(|| self.hooks.diagnostics(file, parse))).unwrap_or_else(
            |panic| {
                let text = panic_text(&*panic);
                logged.push(log(format!("the language's checks failed: {text}")));
                Vec::new()
            },
        )
    }

    /// Parses the new text of a document and publishes its problems.
    fn update(&mut self, uri: lsp::Uri, version: i32, text: String) -> Vec<Message> {
        let id = match self.documents.get(uri.as_str()) {
            Some(document) => document.file.id(),
            None => {
                self.opened = self.opened.saturating_add(1);
                FileId::new(self.opened)
            }
        };
        let size = text.len();
        let file = match SourceFile::new(id, uri.as_str(), text) {
            Ok(file) => file,
            Err(_) => return self.discard(uri, version, size),
        };
        if size > self.limit {
            let shown = self.too_large(uri.clone(), version, size);
            self.documents.insert(
                uri.as_str().to_owned(),
                Document {
                    uri,
                    version,
                    file,
                    parse: None,
                },
            );
            return shown;
        }
        let parse = langmeld_syntax::parse(self.language, &file);
        let mut replies = Vec::new();
        let mut found = parse.diagnostics.clone();
        for extra in self.checks(&file, &parse, &mut replies) {
            let known = found.iter().any(|d| {
                (d.code, d.range, &d.message) == (extra.code, extra.range, &extra.message)
            });
            if !known && extra.file == id {
                found.push(extra);
            }
        }
        found.sort_by_key(|d| (d.range.start(), d.range.end(), d.code));
        let source = self.language.name().to_lowercase();
        let shown = found
            .iter()
            .map(|d| diagnostic(&file, &uri, d, &source, self.encoding))
            .collect();
        replies.push(publish(uri.clone(), Some(version), shown));
        self.documents.insert(
            uri.as_str().to_owned(),
            Document {
                uri,
                version,
                file,
                parse: Some(parse),
            },
        );
        replies
    }

    /// The addresses of the documents that are open.
    pub fn documents(&self) -> impl Iterator<Item = &str> {
        self.documents
            .values()
            .map(|document| document.uri.as_str())
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

fn diagnostic(
    file: &SourceFile,
    uri: &lsp::Uri,
    found: &Diagnostic,
    source: &str,
    encoding: Encoding,
) -> lsp::Diagnostic {
    let mut message = found.message.clone();
    for note in &found.notes {
        message.push('\n');
        message.push_str(note);
    }
    let related: Vec<_> = found
        .labels
        .iter()
        .filter(|label| label.file == file.id())
        .map(|label| lsp::DiagnosticRelatedInformation {
            location: lsp::Location {
                uri: uri.clone(),
                range: position::range(file, label.range, encoding),
            },
            message: label.message.clone(),
        })
        .collect();
    lsp::Diagnostic {
        range: position::range(file, found.range, encoding),
        severity: Some(match found.severity {
            Severity::Warning => lsp::DiagnosticSeverity::WARNING,
            Severity::Info => lsp::DiagnosticSeverity::INFORMATION,
            _ => lsp::DiagnosticSeverity::ERROR,
        }),
        code: Some(lsp::NumberOrString::String(found.code.to_owned())),
        source: Some(source.to_owned()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        ..lsp::Diagnostic::default()
    }
}
