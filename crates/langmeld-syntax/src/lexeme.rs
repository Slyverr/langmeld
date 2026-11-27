use text_size::TextRange;

use crate::SyntaxKind;

/// One lexed token: its kind and where it is. The text stays in the source.
///
/// ```
/// use langmeld_syntax::{FileId, Lexer, SourceFile, SyntaxKind, TokenSpec};
///
/// const GT: SyntaxKind = SyntaxKind(1);
/// const SPACE: SyntaxKind = SyntaxKind(2);
///
/// let lexer = Lexer::builder()
///     .token(TokenSpec::punct(GT, ">"))
///     .token(TokenSpec::regex(SPACE, "space", " +").skip())
///     .build()
///     .expect("the declarations are valid");
///
/// // `>>` is not declared, so it is two tokens; they are joint only without a space.
/// let file = SourceFile::new(FileId::new(0), "example", ">> >")?;
/// let joint: Vec<_> = lexer.lex(&file).tokens.iter().map(|t| t.joint).collect();
/// assert_eq!(joint, [true, false, false, false]);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lexeme {
    /// What the token is.
    pub kind: SyntaxKind,
    /// The bytes it covers.
    pub range: TextRange,
    /// Whether parsers skip it, like whitespace and comments.
    pub trivia: bool,
    /// Whether this token and the next are both not trivia, so nothing separates
    /// them. A parser can then read `>` `>` as one `>>`.
    pub joint: bool,
}

/// What a parser needs to know about token kinds, to report errors and to place
/// comments.
///
/// A [`Lexer`](crate::Lexer) provides this.
pub(crate) trait Vocabulary {
    /// How a message names the kind, such as `` `;` `` or `identifier`.
    fn describe(&self, kind: SyntaxKind) -> String;

    /// Whether the kind is a reserved word, which cannot be used as a name.
    fn is_keyword(&self, kind: SyntaxKind) -> bool;

    /// Whether the kind is an identifier.
    fn is_identifier(&self, kind: SyntaxKind) -> bool;

    /// The word a contextual keyword stands for, if `kind` is one.
    fn contextual_text(&self, kind: SyntaxKind) -> Option<&str>;

    /// Whether the kind is a documentation comment, which belongs to the node after it.
    fn is_doc_comment(&self, kind: SyntaxKind) -> bool;
}
