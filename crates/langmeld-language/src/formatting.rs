//! Source-preserving indentation formatting for valid Langmeld programs.

use langmeld_core::{Diagnostic, Severity, SourceFile, ValidatedLanguageSpec};

use crate::{ParseResult, PreparedLanguage, SyntaxTokenKind, analyze};

/// Whitespace policy used by the structural indentation formatter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatOptions {
    /// Number of spaces used for each nesting level when `insert_spaces` is true.
    pub tab_size: usize,
    /// Whether to indent with spaces or tab characters.
    pub insert_spaces: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            tab_size: 2,
            insert_spaces: true,
        }
    }
}

/// Normalizes block indentation while preserving source text, comments, and strings.
///
/// Line endings are normalized to LF. The formatter uses Langmeld's built-in
/// lexer, so languages with host-specific lexical rules should provide a host
/// formatter instead.
///
/// # Errors
/// Returns parser and generic semantic diagnostics instead of formatting invalid input.
pub fn format_source(
    spec: &ValidatedLanguageSpec,
    source: &SourceFile,
    options: FormatOptions,
) -> Result<String, Vec<Diagnostic>> {
    let prepared = PreparedLanguage::new(spec.clone()).map_err(|error| {
        vec![Diagnostic::spec_error(
            "LMFMT002",
            format!("language parser could not be prepared: {error}"),
        )]
    })?;
    let parser = prepared.with_custom_parsers([]).map_err(|error| {
        vec![Diagnostic::spec_error(
            "LMFMT002",
            format!("language parser could not be configured: {error}"),
        )]
    })?;
    let parsed = parser.parse(source);
    format_parsed_source(spec, source, &parsed, options)
}

/// Formats source from an existing parse result without parsing it again.
///
/// # Errors
/// Returns parser and generic semantic diagnostics instead of formatting invalid input.
pub fn format_parsed_source(
    spec: &ValidatedLanguageSpec,
    source: &SourceFile,
    parsed: &ParseResult,
    options: FormatOptions,
) -> Result<String, Vec<Diagnostic>> {
    if source.text.is_empty() {
        return Ok(String::new());
    }

    let mut diagnostics = parsed.diagnostics.clone();
    diagnostics.extend(analyze(spec, &parsed.program, source));
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(diagnostics);
    }
    if options.tab_size == 0 {
        return Err(vec![Diagnostic::spec_error(
            "LMFMT001",
            "indentation size must be at least one",
        )]);
    }

    let indentation = if options.insert_spaces {
        " ".repeat(options.tab_size)
    } else {
        "\t".to_owned()
    };
    let trailing_newline = source.text.ends_with('\n');
    let lines: Vec<_> = source.text.split('\n').collect();
    let line_count = lines.len() - usize::from(trailing_newline);
    let braces = parsed_brace_events(source, parsed);
    let mut brace_index = 0;
    let mut line_start = 0;
    let mut output = String::with_capacity(source.text.len());
    let mut depth = 0_usize;

    for (index, original) in lines.into_iter().take(line_count).enumerate() {
        let line = original.strip_suffix('\r').unwrap_or(original);
        let content = line.trim_start();
        let content_start = line_start + line.len() - content.len();
        let line_end = line_start + line.len();
        let first_event = brace_index;
        while brace_index < braces.len() && braces[brace_index].offset < line_end {
            brace_index += 1;
        }
        let line_events = &braces[first_event..brace_index];
        if content.is_empty() {
            output.push('\n');
        } else {
            let leading_closes = leading_closing_braces(content, content_start, line_events);
            depth = depth.saturating_sub(leading_closes);
            for _ in 0..depth {
                output.push_str(&indentation);
            }
            output.push_str(content);
            if index + 1 < line_count || trailing_newline {
                output.push('\n');
            }
            let opens = line_events.iter().filter(|event| event.opens).count();
            let closes = line_events.len() - opens;
            depth = depth
                .saturating_add(opens)
                .saturating_sub(closes.saturating_sub(leading_closes));
        }
        line_start += original.len() + 1;
    }
    Ok(output)
}

#[derive(Clone, Copy)]
struct BraceEvent {
    offset: usize,
    opens: bool,
}

fn parsed_brace_events(source: &SourceFile, parsed: &ParseResult) -> Vec<BraceEvent> {
    parsed
        .syntax_tokens
        .iter()
        .filter_map(|token| {
            if token.kind != SyntaxTokenKind::Punctuation {
                return None;
            }
            let start = token.span.start;
            let end = token.span.end;
            match source.text.get(start..end)? {
                "{" => Some(BraceEvent {
                    offset: start,
                    opens: true,
                }),
                "}" => Some(BraceEvent {
                    offset: start,
                    opens: false,
                }),
                _ => None,
            }
        })
        .collect()
}

fn leading_closing_braces(content: &str, content_start: usize, events: &[BraceEvent]) -> usize {
    let mut event_index = 0;
    let mut offset = content_start;
    let mut count = 0;
    for character in content.chars() {
        if character != '}' {
            break;
        }
        while events
            .get(event_index)
            .is_some_and(|event| event.offset < offset)
        {
            event_index += 1;
        }
        if !events
            .get(event_index)
            .is_some_and(|event| event.offset == offset && !event.opens)
        {
            break;
        }
        event_index += 1;
        count += 1;
        offset += character.len_utf8();
    }
    count
}
