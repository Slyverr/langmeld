/// The kind of a token or a node.
///
/// Each language gives the numbers their meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxKind(pub u16);

/// Connects [`SyntaxKind`] to `rowan` syntax trees.
///
/// It has no values; it only appears as a type argument, as in
/// `rowan::SyntaxNode<SyntaxLanguage>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxLanguage {}

impl rowan::Language for SyntaxLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.0)
    }
}
