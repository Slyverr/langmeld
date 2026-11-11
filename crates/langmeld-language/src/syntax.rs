//! Lossless syntax structure for the declarative parser.

use langmeld_core::{SourceFile, Span};

use crate::{CapturedField, CapturedValue, LexedToken, LexicalRules, ParsedConstruct, TokenKind};

/// Source structure returned alongside a declarative parse.
///
/// The tree keeps construct and captured-field boundaries while its token table
/// covers every byte of the source, including whitespace, comments, and text
/// the lexer could not classify. Token and node spans refer to the source
/// snapshot passed to the parser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntaxTree {
    /// Root node containing all parsed constructs and source tokens.
    pub root: SyntaxNode,
    /// Lossless token and trivia table, in source order.
    pub tokens: Vec<SyntaxToken>,
}

/// One structural node in a [`SyntaxTree`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntaxNode {
    /// Role represented by this node.
    pub kind: SyntaxNodeKind,
    /// Construct or captured-field name, when applicable.
    pub name: Option<String>,
    /// Selected syntax variant for construct nodes.
    pub variant: Option<usize>,
    /// UTF-8 byte range in the parsed source snapshot.
    pub span: Span,
    /// Nested structural nodes and indices into [`SyntaxTree::tokens`].
    pub children: Vec<SyntaxElement>,
}

/// Structural role of a syntax node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyntaxNodeKind {
    /// Root containing the complete source file.
    #[default]
    SourceFile,
    /// A construct recognized by the language grammar.
    Construct,
    /// A captured field on a construct.
    Field,
    /// An expression source range captured for host parsing.
    Expression,
}

/// A nested syntax node or a token-table reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxElement {
    /// A nested construct or field node.
    Node(Box<SyntaxNode>),
    /// Index into [`SyntaxTree::tokens`].
    Token(usize),
}

/// One source segment in the lossless token table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    /// Lexical or trivia category for this source segment.
    pub kind: SyntaxTokenKind,
    /// UTF-8 byte range in the parsed source snapshot.
    pub span: Span,
}

/// Token and trivia categories preserved by the declarative parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    /// Whitespace excluding line breaks.
    Whitespace,
    /// A line break, preserving LF, CR, or CRLF.
    Newline,
    /// Identifier token.
    Identifier,
    /// Decimal integer token.
    Integer,
    /// Quoted or host-defined string token.
    String,
    /// Operator or punctuation token.
    Punctuation,
    /// Comment recognized from the language's lexical rules.
    Comment,
    /// Source text not classified by the lexer or grammar.
    Unknown,
}

impl SyntaxToken {
    /// Returns the exact source text for this token when `source` contains its
    /// span.
    #[must_use]
    pub fn text<'a>(&self, source: &'a SourceFile) -> Option<&'a str> {
        source.text.get(self.span.start..self.span.end)
    }
}

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
        Span::new(cursor, source.text.len()),
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
            span: Span::new(0, source.text.len()),
            children: with_tokens(children, Span::new(0, source.text.len()), &syntax_tokens),
        },
        tokens: syntax_tokens,
    }
}

pub(crate) fn unparsed_syntax_tree(source: &SourceFile) -> SyntaxTree {
    let span = Span::new(0, source.text.len());
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
    let Some(gap) = source.text.get(span.start..span.end) else {
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
