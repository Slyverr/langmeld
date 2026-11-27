/// The kind of a token or a node.
///
/// Each language gives the numbers their meaning, apart from the highest ones, which
/// are reserved for the constants of this type.
///
/// ```
/// use langmeld_syntax::SyntaxKind;
///
/// const NAME: SyntaxKind = SyntaxKind(1);
/// assert_ne!(NAME, SyntaxKind::ERROR);
/// ```
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct SyntaxKind(pub u16);

impl SyntaxKind {
    /// Text that is not valid: unknown characters, or tokens in a wrong place.
    pub const ERROR: Self = Self(u16::MAX);
    /// What a parser sees after the last token. It never appears in a tree.
    pub const EOF: Self = Self(u16::MAX - 1);
    /// A byte order mark at the start of a file.
    pub const BOM: Self = Self(u16::MAX - 2);
}

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

/// A node of a syntax tree.
pub type SyntaxNode = rowan::SyntaxNode<SyntaxLanguage>;
/// A token of a syntax tree.
pub type SyntaxToken = rowan::SyntaxToken<SyntaxLanguage>;
/// A node or a token of a syntax tree.
pub type SyntaxElement = rowan::SyntaxElement<SyntaxLanguage>;
