use std::collections::HashMap;

use langmeld_core::{PositionEncoding, SourceFile, Span};
use lsp_types::{SemanticToken as LspSemanticToken, SemanticTokens};

/// Names advertised to an editor for host-provided semantic tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticTokenLegend {
    /// Token classifications, such as `function`, `variable`, or a custom name.
    pub token_types: Vec<String>,
    /// Modifiers, such as `declaration`, `readonly`, or a custom name.
    pub token_modifiers: Vec<String>,
}

impl SemanticTokenLegend {
    /// Creates a legend from token type and modifier names.
    #[must_use]
    pub fn new(
        token_types: impl IntoIterator<Item = impl Into<String>>,
        token_modifiers: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            token_types: token_types.into_iter().map(Into::into).collect(),
            token_modifiers: token_modifiers.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_names("token type", &self.token_types)?;
        validate_names("token modifier", &self.token_modifiers)?;
        if self.token_types.is_empty() {
            return Err("semantic token legends must declare at least one token type".into());
        }
        if self.token_modifiers.len() > u32::BITS as usize {
            return Err("semantic token legends cannot declare more than 32 modifiers".into());
        }
        Ok(())
    }
}

fn validate_names(kind: &str, names: &[String]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::with_capacity(names.len());
    for name in names {
        if name.trim().is_empty() {
            return Err(format!("semantic token {kind} names cannot be empty"));
        }
        if !seen.insert(name) {
            return Err(format!(
                "semantic token {kind} `{name}` is declared more than once"
            ));
        }
    }
    Ok(())
}

/// A host-classified source span before LSP position encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticToken {
    /// UTF-8 byte range in the analyzed source snapshot.
    pub span: Span,
    /// Token type name declared in the backend legend.
    pub token_type: String,
    /// Modifier names declared in the backend legend.
    pub modifiers: Vec<String>,
}

impl SemanticToken {
    /// Creates a semantic token with no modifiers.
    #[must_use]
    pub fn new(span: Span, token_type: impl Into<String>) -> Self {
        Self {
            span,
            token_type: token_type.into(),
            modifiers: Vec::new(),
        }
    }

    /// Adds one legend modifier.
    #[must_use]
    pub fn with_modifier(mut self, modifier: impl Into<String>) -> Self {
        self.modifiers.push(modifier.into());
        self
    }
}

pub(crate) fn encode(
    source: &SourceFile,
    legend: &SemanticTokenLegend,
    tokens: impl IntoIterator<Item = SemanticToken>,
    encoding: PositionEncoding,
) -> SemanticTokens {
    let token_types = legend
        .token_types
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index as u32))
        .collect::<HashMap<_, _>>();
    let modifiers = legend
        .token_modifiers
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let line_index = source.line_index();
    let mut encoded_tokens = Vec::new();
    for token in tokens {
        let Some(&token_type) = token_types.get(token.token_type.as_str()) else {
            continue;
        };
        if token.span.is_empty()
            || token.span.end > source.text.len()
            || !source.text.is_char_boundary(token.span.start)
            || !source.text.is_char_boundary(token.span.end)
        {
            continue;
        }
        let mut modifier_bits = 0_u32;
        let mut valid_modifiers = true;
        for modifier in &token.modifiers {
            let Some(&bit) = modifiers.get(modifier.as_str()) else {
                valid_modifiers = false;
                break;
            };
            modifier_bits |= 1_u32 << bit;
        }
        if !valid_modifiers {
            continue;
        }
        for span in split_lines(&source.text, token.span) {
            let Some(start) = line_index.position(span.start, encoding) else {
                continue;
            };
            let Some(end) = line_index.position(span.end, encoding) else {
                continue;
            };
            if start.line != end.line || start.character >= end.character {
                continue;
            }
            let Ok(length) = u32::try_from(end.character - start.character) else {
                continue;
            };
            encoded_tokens.push((span, start, length, token_type, modifier_bits));
        }
    }
    encoded_tokens.sort_by_key(|(span, _, _, _, _)| (span.start, span.end));

    let mut encoded = Vec::with_capacity(encoded_tokens.len());
    let mut previous_end = 0;
    let mut previous_line = 0;
    let mut previous_start = 0;
    for (span, start, length, token_type, token_modifiers_bitset) in encoded_tokens {
        if span.start < previous_end {
            continue;
        }
        let delta_line = start.line.saturating_sub(previous_line);
        let delta_start = if delta_line == 0 {
            start.character.saturating_sub(previous_start)
        } else {
            start.character
        };
        let (Ok(delta_line), Ok(delta_start)) =
            (u32::try_from(delta_line), u32::try_from(delta_start))
        else {
            continue;
        };
        encoded.push(LspSemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset,
        });
        previous_end = span.end;
        previous_line = start.line;
        previous_start = start.character;
    }
    SemanticTokens {
        result_id: None,
        data: encoded,
    }
}

fn split_lines(text: &str, span: Span) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = span.start;
    for (relative, _) in text[span.start..span.end].match_indices('\n') {
        let newline = span.start + relative;
        let end = if newline > start && text.as_bytes()[newline - 1] == b'\r' {
            newline - 1
        } else {
            newline
        };
        if start < end {
            spans.push(Span::new(start, end));
        }
        start = newline + 1;
    }
    if start < span.end {
        spans.push(Span::new(start, span.end));
    }
    spans
}
