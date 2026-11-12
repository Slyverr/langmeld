//! Public data types for lossless declarative syntax trees.

use langmeld_core::{SourceFile, Span};

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
        source.text().get(self.span.start..self.span.end)
    }
}
