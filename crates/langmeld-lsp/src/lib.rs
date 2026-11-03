//! A small LSP transport and diagnostic server for declarative Langmeld languages.

use std::{
    collections::HashMap,
    io::{self, BufRead, Read, Write},
};

use langmeld_core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{
    AstNode, ConfiguredLanguageParser, ParseResult, PreparedLanguage, Program, WorkspaceAnalysis,
    analyze_workspace_with_resolutions,
};
use serde_json::{Value, json};

mod navigation;

const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMPLETION_ITEMS: usize = 200;
const MAX_DOCUMENT_SYMBOLS: usize = 10_000;

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
            "language declares host analysis behaviors; use a host-integrated language server"
                .to_owned(),
        );
    }
    PreparedLanguage::new(language)
        .map_err(|error| error.to_string())?
        .configure()
        .map_err(|error| error.to_string())
}

/// Serves document synchronization and diagnostics messages.
///
/// Returns true when the client sent shutdown before exit.
///
/// # Errors
/// Returns malformed framing, I/O, or JSON-RPC serialization errors.
pub fn serve<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    language: ValidatedLanguageSpec,
) -> Result<bool, String> {
    let parser = configure_language(language.clone())?;
    let mut server = Server {
        language,
        parser,
        documents: HashMap::new(),
        shutdown: false,
    };

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
                    initialize_response(id, self.supports_completion(), self.supports_navigation())
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
        let analysis = analyze_workspace_with_resolutions(&self.language, &files);
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

    fn supports_completion(&self) -> bool {
        self.parser.language().keywords().next().is_some()
    }

    fn supports_navigation(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| construct.symbol.is_some())
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
        keyword_completion_items(
            &document.source.text,
            offset,
            self.parser.language().keywords(),
        )
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
        let analysis = analyze_workspace_with_resolutions(&self.language, &files);
        Some(NavigationRequest {
            uris,
            files,
            source_index,
            occurrence,
            analysis,
        })
    }
}

fn initialize_response(id: Value, supports_completion: bool, supports_navigation: bool) -> Value {
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

fn keyword_completion_items<'a>(
    text: &str,
    offset: usize,
    keywords: impl Iterator<Item = &'a str>,
) -> Vec<Value> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Vec::new();
    }
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_prefix = &text[line_start..offset];
    let word_start = line_prefix
        .char_indices()
        .rev()
        .find(|(_, value)| !value.is_alphanumeric() && *value != '_' && *value != '-')
        .map_or(line_start, |(index, value)| {
            line_start + index + value.len_utf8()
        });
    let prefix = &text[word_start..offset];
    keywords
        .filter(|keyword| keyword.starts_with(prefix))
        .take(MAX_COMPLETION_ITEMS)
        .map(|keyword| {
            json!({
                "label": keyword,
                "kind": 14,
                "filterText": keyword,
                "textEdit": {
                    "range": {
                        "start": lsp_position(text, word_start),
                        "end": lsp_position(text, offset),
                    },
                    "newText": keyword,
                }
            })
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
    let mut line_start = 0;
    for line in 0..=target_line {
        if line == target_line {
            let line_end = source[line_start..]
                .find('\n')
                .map_or(source.len(), |offset| line_start + offset);
            let content_end = if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
                line_end - 1
            } else {
                line_end
            };
            let content = &source[line_start..content_end];
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
            return (character == target_character).then_some(content_end);
        }
        let newline = source[line_start..].find('\n')?;
        line_start = line_start.checked_add(newline + 1)?;
    }
    None
}

fn lsp_position(text: &str, offset: usize) -> Value {
    let mut boundary = offset.min(text.len());
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let prefix = &text[..boundary];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix
        .rfind('\n')
        .map_or(prefix, |index| &prefix[index + 1..]);
    json!({
        "line": line,
        "character": line_start.encode_utf16().count(),
    })
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
        let response = initialize_response(json!(1), false, false);
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
