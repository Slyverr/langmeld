//! Reading the syntax tree of a `.langmeld` file.

use langmeld_syntax::{SyntaxNode, SyntaxToken, TextRange};

use crate::meta;

/// A node of a `.langmeld` file.
#[derive(Clone)]
pub(crate) struct Node(pub(crate) SyntaxNode);

impl Node {
    pub(crate) fn is(&self, name: &str) -> bool {
        self.name() == name
    }

    pub(crate) fn name(&self) -> &'static str {
        meta::language().kind_name(self.0.kind())
    }

    pub(crate) fn range(&self) -> TextRange {
        self.0.text_range()
    }

    pub(crate) fn children(&self) -> impl Iterator<Item = Node> {
        self.0.children().map(Node)
    }

    pub(crate) fn child(&self, name: &str) -> Option<Node> {
        self.children().find(|child| child.is(name))
    }

    /// The tokens directly in this node that rules see.
    pub(crate) fn tokens(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !meta::language().is_trivia(token.kind()))
    }

    /// The direct tokens of the kind called `kind`: `ident`, `string` and so on.
    pub(crate) fn all(&self, kind: &str) -> impl Iterator<Item = SyntaxToken> {
        let wanted = meta::language().kind(kind);
        self.tokens()
            .filter(move |token| Some(token.kind()) == wanted)
    }

    /// The first name written directly in this node.
    pub(crate) fn ident(&self) -> Option<SyntaxToken> {
        self.all("ident").next()
    }

    pub(crate) fn idents(&self) -> impl Iterator<Item = SyntaxToken> {
        self.all("ident")
    }

    /// Whether a token with this text is directly in the node.
    pub(crate) fn has(&self, literal: &str) -> bool {
        self.tokens().any(|token| token.text() == literal)
    }
}
