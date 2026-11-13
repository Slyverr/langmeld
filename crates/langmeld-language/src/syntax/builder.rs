//! Build lossless syntax trees from parser output.

use langmeld_core::{SourceFile, Span};

use crate::parser::model::{CapturedField, CapturedValue, ParsedConstruct};
use crate::{LexedToken, LexicalRules, TokenKind};

use super::model::{
    SyntaxElement, SyntaxNode, SyntaxNodeKind, SyntaxToken, SyntaxTokenKind, SyntaxTree,
};

pub(crate) fn build_syntax_tree(
    source: &SourceFile,
    constructs: &[ParsedConstruct],
    tokens: &[LexedToken],
    lexical_rules: &LexicalRules,
) -> SyntaxTree {
    let mut syntax_tokens = Vec::new();
    let mut cursor = 0;

    for token in tokens {
        append_gap(
            source,
            Span::new(cursor, token.span.start),
            lexical_rules,
            &mut syntax_tokens,
        );
        syntax_tokens.push(SyntaxToken {
            kind: token_kind(token.kind),
            span: token.span,
        });
        cursor = token.span.end;
    }

    append_gap(
        source,
        Span::new(cursor, source.text().len()),
        lexical_rules,
        &mut syntax_tokens,
    );

    let children = constructs
        .iter()
        .map(|construct| SyntaxElement::Node(Box::new(construct_node(construct, &syntax_tokens))))
        .collect();

    SyntaxTree {
        root: SyntaxNode {
            kind: SyntaxNodeKind::SourceFile,
            name: None,
            variant: None,
            span: Span::new(0, source.text().len()),
            children: with_tokens(children, Span::new(0, source.text().len()), &syntax_tokens),
        },
        tokens: syntax_tokens,
    }
}

pub(crate) fn unparsed_syntax_tree(source: &SourceFile) -> SyntaxTree {
    let span = Span::new(0, source.text().len());
    let tokens = if span.is_empty() {
        Vec::new()
    } else {
        vec![SyntaxToken {
            kind: SyntaxTokenKind::Unknown,
            span,
        }]
    };

    SyntaxTree {
        root: SyntaxNode {
            kind: SyntaxNodeKind::SourceFile,
            name: None,
            variant: None,
            span,
            children: tokens
                .iter()
                .enumerate()
                .map(|(index, _)| SyntaxElement::Token(index))
                .collect(),
        },
        tokens,
    }
}

fn construct_node(construct: &ParsedConstruct, tokens: &[SyntaxToken]) -> SyntaxNode {
    let mut children = Vec::with_capacity(construct.fields.len() + construct.children.len());
    children.extend(
        construct
            .fields
            .iter()
            .map(|field| SyntaxElement::Node(Box::new(field_node(field, tokens)))),
    );
    children.extend(
        construct
            .children
            .iter()
            .map(|child| SyntaxElement::Node(Box::new(construct_node(child, tokens)))),
    );

    SyntaxNode {
        kind: SyntaxNodeKind::Construct,
        name: Some(construct.name.clone()),
        variant: Some(construct.variant),
        span: construct.span,
        children: with_tokens(children, construct.span, tokens),
    }
}

fn field_node(field: &CapturedField, tokens: &[SyntaxToken]) -> SyntaxNode {
    let kind = if matches!(field.value, CapturedValue::Expression) {
        SyntaxNodeKind::Expression
    } else {
        SyntaxNodeKind::Field
    };

    SyntaxNode {
        kind,
        name: Some(field.name.clone()),
        variant: None,
        span: field.span,
        children: with_tokens(Vec::new(), field.span, tokens),
    }
}

fn with_tokens(
    nodes: Vec<SyntaxElement>,
    span: Span,
    tokens: &[SyntaxToken],
) -> Vec<SyntaxElement> {
    // The parser emits fields and child constructs in source order.
    let mut token_index = tokens.partition_point(|token| token.span.end <= span.start);
    let token_end = tokens.partition_point(|token| token.span.start < span.end);
    let mut children = Vec::with_capacity(nodes.len() + token_end.saturating_sub(token_index));

    for node in nodes {
        let SyntaxElement::Node(node) = node else {
            continue;
        };
        if node.span.start < span.start || node.span.end > span.end {
            continue;
        }

        while let Some(token) = tokens.get(token_index)
            && token.span.end <= node.span.start
            && token.span.start >= span.start
        {
            children.push(SyntaxElement::Token(token_index));
            token_index += 1;
        }

        if node.span.start < node.span.end {
            while let Some(token) = tokens.get(token_index)
                && token.span.start < node.span.end
            {
                token_index += 1;
            }
        }
        children.push(SyntaxElement::Node(node));
    }

    while let Some(token) = tokens.get(token_index)
        && token.span.start < span.end
    {
        if token.span.start >= span.start {
            children.push(SyntaxElement::Token(token_index));
        }
        token_index += 1;
    }

    children
}

fn append_gap(
    source: &SourceFile,
    span: Span,
    rules: &LexicalRules,
    output: &mut Vec<SyntaxToken>,
) {
    let Some(gap) = source.slice(span) else {
        return;
    };
    let mut offset = 0;

    while offset < gap.len() {
        let rest = &gap[offset..];
        let absolute_start = span.start + offset;

        if let Some(end) = comment_end(rest, rules) {
            output.push(SyntaxToken {
                kind: SyntaxTokenKind::Comment,
                span: Span::new(absolute_start, absolute_start + end),
            });
            offset += end;
            continue;
        }

        let character = rest.chars().next().unwrap_or_default();
        let kind = match character {
            '\n' | '\r' => SyntaxTokenKind::Newline,
            character if character.is_whitespace() => SyntaxTokenKind::Whitespace,
            _ => SyntaxTokenKind::Unknown,
        };
        let mut end = character.len_utf8();

        if character == '\r' && rest[end..].starts_with('\n') {
            end += 1;
        } else if matches!(kind, SyntaxTokenKind::Whitespace | SyntaxTokenKind::Unknown) {
            while end < rest.len() {
                let tail = &rest[end..];
                if comment_end(tail, rules).is_some() {
                    break;
                }
                let next = tail.chars().next().unwrap_or_default();
                let next_kind = match next {
                    '\n' | '\r' => SyntaxTokenKind::Newline,
                    character if character.is_whitespace() => SyntaxTokenKind::Whitespace,
                    _ => SyntaxTokenKind::Unknown,
                };
                if next_kind != kind {
                    break;
                }
                end += next.len_utf8();
            }
        }

        output.push(SyntaxToken {
            kind,
            span: Span::new(absolute_start, absolute_start + end),
        });
        offset += end;
    }
}

fn comment_end(text: &str, rules: &LexicalRules) -> Option<usize> {
    if let Some(marker) = rules
        .line_comments()
        .iter()
        .filter(|marker| text.starts_with(marker.as_str()))
        .max_by_key(|marker| marker.len())
    {
        let end = text.find(['\r', '\n']).unwrap_or(text.len());
        return Some(end.max(marker.len()));
    }

    rules
        .block_comments()
        .iter()
        .filter(|(opening, _)| text.starts_with(opening.as_str()))
        .max_by_key(|(opening, _)| opening.len())
        .map(|(opening, closing)| {
            text[opening.len()..]
                .find(closing)
                .map_or(text.len(), |offset| opening.len() + offset + closing.len())
        })
}

fn token_kind(kind: TokenKind) -> SyntaxTokenKind {
    match kind {
        TokenKind::Identifier => SyntaxTokenKind::Identifier,
        TokenKind::Integer => SyntaxTokenKind::Integer,
        TokenKind::String => SyntaxTokenKind::String,
        TokenKind::Punctuation => SyntaxTokenKind::Punctuation,
        TokenKind::Invalid => SyntaxTokenKind::Unknown,
    }
}
