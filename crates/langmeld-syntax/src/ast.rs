//! Typed views of syntax trees. Code generated from a grammar is built on this, and
//! views written by hand can be too.

use crate::{SyntaxElement, SyntaxKind, SyntaxNode, TextRange};

/// A node seen as one kind of thing, such as a statement.
pub trait AstNode: Sized {
    /// Whether nodes of this kind can be seen this way.
    fn can_cast(kind: SyntaxKind) -> bool;

    /// The view of `node`, if it is of such a kind.
    fn cast(node: SyntaxNode) -> Option<Self>;

    /// The node viewed.
    fn syntax(&self) -> &SyntaxNode;

    /// Where the node is in the source.
    fn range(&self) -> TextRange {
        self.syntax().text_range()
    }
}

/// The children of `node`, from after the first `after` token to before the first
/// `before` token that follows. Without `after` they start at the first child, and
/// without `before` they end at the last one. If there is no `after` token, there are
/// none.
pub fn between(
    node: &SyntaxNode,
    after: Option<SyntaxKind>,
    before: Option<SyntaxKind>,
) -> impl Iterator<Item = SyntaxElement> + use<> {
    let is = |element: &SyntaxElement, kind: SyntaxKind| {
        element.as_token().is_some() && element.kind() == kind
    };
    let mut children = node.children_with_tokens();
    if let Some(after) = after {
        children.find(|child| is(child, after));
    }
    children.take_while(move |child| before.is_none_or(|before| !is(child, before)))
}

/// The children of `node` after the first token of kind `after`. Generated types use it
/// to find a part that follows a token.
pub fn after(node: &SyntaxNode, after: u16) -> impl Iterator<Item = SyntaxElement> + use<> {
    between(node, Some(SyntaxKind(after)), None)
}

/// The children of `node` before the first token of kind `before`.
pub fn before(node: &SyntaxNode, before: u16) -> impl Iterator<Item = SyntaxElement> + use<> {
    between(node, None, Some(SyntaxKind(before)))
}

/// The children of `node` after the first token of kind `after` and before the first
/// token of kind `before` that follows it.
pub fn within(
    node: &SyntaxNode,
    after: u16,
    before: u16,
) -> impl Iterator<Item = SyntaxElement> + use<> {
    between(node, Some(SyntaxKind(after)), Some(SyntaxKind(before)))
}
