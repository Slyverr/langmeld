use std::io::{self, BufRead, Read, Write};

use langmeld_core::{Diagnostic, SourceFile, Span};
use serde_json::{Value, json};

use super::MAX_DOCUMENT_BYTES;

const MAX_HEADER_BYTES: usize = 8 * 1024;
pub(super) const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn lsp_range(source: &str, span: Span) -> Value {
    json!({
        "start": lsp_position(source, span.start),
        "end": lsp_position(source, span.end)
    })
}

pub(super) fn lsp_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) -> Value {
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

pub(super) fn apply_content_changes(current: &str, changes: &[Value]) -> Option<String> {
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

pub(super) fn offset_at_position(
    source: &str,
    target_line: u32,
    target_character: u32,
) -> Option<usize> {
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

pub(super) fn lsp_position(text: &str, offset: usize) -> Value {
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

pub(super) fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
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

pub(super) fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
