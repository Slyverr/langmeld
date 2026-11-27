//! What a language tells editors about its tokens.

/// What sort of thing a token is, for colouring and the like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenClass {
    /// A reserved word.
    Keyword,
    /// A word that is a keyword only where a rule asks for it.
    ContextualKeyword,
    /// A name.
    Identifier,
    /// A number.
    Number,
    /// Quoted text or a character, or a part of it.
    String,
    /// Fixed text other than words, such as operators and brackets.
    Punctuation,
    /// A comment.
    Comment,
    /// A documentation comment.
    DocComment,
    /// Space between tokens, including line breaks.
    Whitespace,
    /// Anything else, such as tokens declared by pattern.
    Other,
}
