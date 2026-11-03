//! LSP semantic-token encoding for shared syntax classifications.

use langmeld_language::Classification;
pub(super) const TOKEN_TYPES: [&str; 8] = [
    "keyword",
    "operator",
    "string",
    "number",
    "comment",
    "variable",
    "enumMember",
    "property",
];

pub(super) const TOKEN_MODIFIERS: [&str; 2] = ["declaration", "reference"];

pub(super) fn encode(source: &str, classifications: &[Classification]) -> Vec<u32> {
    let mut pieces = Vec::new();
    for classification in classifications {
        let Some(token_type) = token_type(classification.kind) else {
            continue;
        };
        let Some(span) = source.get(classification.span.start..classification.span.end) else {
            continue;
        };
        let modifier = match classification.kind {
            "declaration" => 1,
            "reference" => 2,
            _ => 0,
        };
        let mut start = classification.span.start;
        for (relative, character) in span.char_indices() {
            if matches!(character, '\r' | '\n') {
                push_piece(
                    source,
                    start,
                    classification.span.start + relative,
                    token_type,
                    modifier,
                    &mut pieces,
                );
                start = classification.span.start + relative + character.len_utf8();
            }
        }
        push_piece(
            source,
            start,
            classification.span.end,
            token_type,
            modifier,
            &mut pieces,
        );
    }
    pieces.sort_by_key(|piece| (piece.0, piece.1));
    let mut data = Vec::with_capacity(pieces.len() * 5);
    let (mut previous_line, mut previous_start) = (0_u32, 0_u32);
    let mut previous_end = 0_u32;
    for (line, start, length, kind, modifier) in pieces {
        if line == previous_line && !data.is_empty() && start < previous_end {
            continue;
        }
        let delta_line = line - previous_line;
        let delta_start = if delta_line == 0 {
            start - previous_start
        } else {
            start
        };
        data.extend([delta_line, delta_start, length, kind, modifier]);
        previous_line = line;
        previous_start = start;
        previous_end = start + length;
    }
    data
}

fn token_type(kind: &str) -> Option<u32> {
    Some(match kind {
        "keyword" => 0,
        "operator" => 1,
        "string" => 2,
        "number" => 3,
        "comment" => 4,
        "identifier" | "declaration" | "reference" => 5,
        "enumMember" => 6,
        "property" => 7,
        _ => return None,
    })
}

fn push_piece(
    source: &str,
    start: usize,
    end: usize,
    token_type: u32,
    modifier: u32,
    output: &mut Vec<(u32, u32, u32, u32, u32)>,
) {
    if start >= end {
        return;
    }
    let start_position = super::lsp_position(source, start);
    let end_position = super::lsp_position(source, end);
    let (Some(line), Some(character), Some(end_line), Some(end_character)) = (
        start_position["line"].as_u64(),
        start_position["character"].as_u64(),
        end_position["line"].as_u64(),
        end_position["character"].as_u64(),
    ) else {
        return;
    };
    if line != end_line || character >= end_character {
        return;
    }
    let (Ok(line), Ok(character), Ok(length)) = (
        u32::try_from(line),
        u32::try_from(character),
        u32::try_from(end_character - character),
    ) else {
        return;
    };
    output.push((line, character, length, token_type, modifier));
}

#[cfg(test)]
mod tests {
    use super::*;
    use langmeld_core::Span;

    #[test]
    fn encodes_utf16_deltas_and_declaration_reference_modifiers() {
        let source = "😀x\nref";
        let encoded = encode(
            source,
            &[
                Classification {
                    span: Span::new(4, 5),
                    kind: "declaration",
                },
                Classification {
                    span: Span::new(6, 9),
                    kind: "reference",
                },
            ],
        );
        assert_eq!(encoded, [0, 2, 1, 5, 1, 1, 0, 3, 5, 2]);
    }

    #[test]
    fn splits_multiline_tokens_across_lf_crlf_and_cr() {
        let source = "a\nb\r\nc\rd";
        let encoded = encode(
            source,
            &[Classification {
                span: Span::new(0, source.len()),
                kind: "comment",
            }],
        );
        assert_eq!(
            encoded,
            [0, 0, 1, 4, 0, 1, 0, 1, 4, 0, 1, 0, 1, 4, 0, 1, 0, 1, 4, 0]
        );
    }
}
