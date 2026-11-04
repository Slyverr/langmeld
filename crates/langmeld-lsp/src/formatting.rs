//! Document formatting and safe source/code-action edits.

use langmeld_core::{Severity, SourceFile, Span};
use langmeld_language::{FormatOptions, SyntaxTokenKind, format_parsed_source};
use serde_json::{Value, json};

use super::{MAX_DOCUMENT_BYTES, OpenDocument, lsp_diagnostic, lsp_range, offset_at_position};

const MAX_FORMAT_TAB_SIZE: usize = 32;

impl super::Server {
    pub(super) fn supports_builtin_formatting(&self) -> bool {
        !self.parser.has_custom_lexer()
            && self
                .language
                .spec()
                .constructs
                .iter()
                .all(|construct| construct.custom_parser.is_none())
    }

    pub(super) fn supports_formatting(&self) -> bool {
        self.formatting_provider.is_some() || self.supports_builtin_formatting()
    }

    pub(super) fn supports_missing_closer_quickfix(&self) -> bool {
        self.supports_builtin_formatting()
    }

    pub(super) fn format_document(
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

    pub(super) fn validate_formatted_document(
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

    pub(super) fn formatting_edits(&self, params: &Value) -> Vec<Value> {
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

    pub(super) fn code_actions(&self, params: &Value) -> Vec<Value> {
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

    pub(super) fn missing_closer_action(
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
                    SyntaxTokenKind::MissingRightBrace
                        | SyntaxTokenKind::MissingBlockCloser
                        | SyntaxTokenKind::MissingRightBracket
                )
            })
            .collect();
        let [token] = missing.as_slice() else {
            return None;
        };
        let (code, closer) = match token.kind {
            SyntaxTokenKind::MissingRightBrace => ("LM1204", "}"),
            SyntaxTokenKind::MissingBlockCloser => {
                let diagnostic = document
                    .parsed
                    .diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.code == "LM1204")?;
                ("LM1204", missing_block_closer(&diagnostic.message)?)
            }
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
        let insertion = if matches!(
            token.kind,
            SyntaxTokenKind::MissingRightBrace | SyntaxTokenKind::MissingBlockCloser
        ) && !document.source.text.ends_with('\n')
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
}

fn missing_block_closer(message: &str) -> Option<&str> {
    let value = message.strip_prefix("expected block closer `")?;
    let (closer, _) = value.split_once("` before the end of the file")?;
    (!closer.is_empty()).then_some(closer)
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
