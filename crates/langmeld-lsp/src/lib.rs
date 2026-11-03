//! A small LSP transport and diagnostic server for declarative Langmeld languages.

use std::{
    collections::HashSet,
    io::{self, BufRead, Read, Write},
};

use langmeld_core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{ConfiguredLanguageParser, PreparedLanguage, analyze_workspace};
use serde_json::{Value, json};

const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

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

/// Serves initialize, shutdown, textDocument/didOpen, and diagnostics messages.
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
        documents: HashSet::new(),
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
    documents: HashSet<String>,
    shutdown: bool,
}

impl Server {
    fn handle(&mut self, message: &Value) -> Vec<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Vec::new();
        };
        let id = message.get("id").cloned();
        match method {
            "initialize" => id.into_iter().map(initialize_response).collect(),
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
            "textDocument/didClose" => {
                if let Some(uri) = message
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                {
                    self.documents.remove(uri);
                    vec![json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": [] }
                    })]
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
        let (Some(uri), Some(text)) = (
            document.get("uri").and_then(Value::as_str),
            document.get("text").and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
        let source = SourceFile::new(uri, text);
        let parsed = self.parser.parse(&source);
        let mut diagnostics = parsed.diagnostics.clone();
        for item in analyze_workspace(&self.language, &[(&source, &parsed.program)]) {
            diagnostics.push(item.diagnostic);
        }
        self.documents.insert(uri.to_owned());
        vec![json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "version": version,
                "diagnostics": diagnostics.iter().map(|item| lsp_diagnostic(&source, item)).collect::<Vec<_>>()
            }
        })]
    }
}

fn initialize_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "capabilities": {
                "textDocumentSync": { "openClose": true, "change": 0 }
            },
            "serverInfo": {
                "name": "langmeld-lsp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
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
