//! The shapes a grammar rule is made of.
//!
//! ```
//! use langmeld_syntax::pattern::{field, lit, opt, r};
//! use langmeld_syntax::seq;
//!
//! // let name = value ;
//! let statement = seq![
//!     lit("let"),
//!     field("name", r("Name")),
//!     opt(seq![lit("="), field("value", r("Expr"))]),
//!     lit(";"),
//! ];
//! ```

use crate::{FileId, TextRange};

/// A part of a rule. Build it with the functions of [`pattern`](self).
#[derive(Clone, Debug)]
pub struct Pattern(pub(crate) Shape, pub(crate) Option<(FileId, TextRange)>);

impl Pattern {
    /// Records where this part was written, so that a problem with a name it uses points
    /// there.
    #[must_use]
    pub fn located(mut self, file: FileId, range: TextRange) -> Self {
        self.1 = Some((file, range));
        self
    }
}

fn new(shape: Shape) -> Pattern {
    Pattern(shape, None)
}

#[derive(Clone, Debug)]
pub(crate) enum Shape {
    Literal(String),
    Reference(String),
    Sequence(Vec<Pattern>),
    Choice(Vec<Pattern>),
    Optional(Box<Pattern>),
    Many(Box<Pattern>),
    Many1(Box<Pattern>),
    Separated {
        item: Box<Pattern>,
        separator: String,
        strict: bool,
    },
    Terminated(Box<Pattern>),
    Field(String, Box<Pattern>),
    Look(Box<Pattern>),
    Not(Box<Pattern>),
    Attempt(Box<Pattern>),
}

/// Exactly this text, such as `;` or `let`. A word is reserved unless it is declared
/// contextual.
///
/// ```
/// use langmeld_syntax::pattern::{lit, many};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many(lit("go")))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("go go"), 0);
/// ```
pub fn lit(text: impl Into<String>) -> Pattern {
    new(Shape::Literal(text.into()))
}

/// The rule or token called `name`.
///
/// ```
/// use langmeld_syntax::pattern::{many, r};
/// use langmeld_syntax::{FileId, IdentStyle, IntPreset, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .token(Token::preset("int", Preset::Int(IntPreset::default())))
///     .rule("File", many(r("int")))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("1 2 3"), 0);
/// assert_eq!(problems("1 x"), 1);
/// ```
pub fn r(name: impl Into<String>) -> Pattern {
    new(Shape::Reference(name.into()))
}

/// One part after another. [`seq!`](crate::seq) is shorter to write.
///
/// ```
/// use langmeld_syntax::pattern::lit;
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", seq![lit("a"), lit("b")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a b"), 0);
/// assert_eq!(problems("a c"), 1);
/// ```
pub fn seq(parts: impl IntoIterator<Item = Pattern>) -> Pattern {
    new(Shape::Sequence(parts.into_iter().collect()))
}

/// One of several parts, told apart by the tokens that come next.
/// [`choice!`](crate::choice) is shorter to write.
///
/// ```
/// use langmeld_syntax::pattern::lit;
/// use langmeld_syntax::{choice, FileId, IdentStyle, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", choice![lit("a"), lit("b")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("b"), 0);
/// assert_eq!(problems("c"), 1);
/// ```
pub fn choice(alternatives: impl IntoIterator<Item = Pattern>) -> Pattern {
    new(Shape::Choice(alternatives.into_iter().collect()))
}

/// The part, or nothing.
///
/// ```
/// use langmeld_syntax::pattern::{lit, opt};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", seq![lit("a"), opt(lit("b"))])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a"), 0);
/// assert_eq!(problems("a c"), 1);
/// ```
pub fn opt(part: Pattern) -> Pattern {
    new(Shape::Optional(Box::new(part)))
}

/// The part any number of times.
///
/// ```
/// use langmeld_syntax::pattern::{lit, many};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many(lit("a")))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a a a"), 0);
/// assert_eq!(problems("b"), 1);
/// ```
pub fn many(part: Pattern) -> Pattern {
    new(Shape::Many(Box::new(part)))
}

/// The part once or more.
///
/// ```
/// use langmeld_syntax::pattern::{lit, many1};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many1(lit("a")))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a a"), 0);
/// ```
pub fn many1(part: Pattern) -> Pattern {
    new(Shape::Many1(Box::new(part)))
}

/// Any number of items with `separator` between them, and possibly after the last.
///
/// ```
/// use langmeld_syntax::pattern::{lit, r, sep};
/// use langmeld_syntax::{FileId, IdentStyle, IntPreset, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .token(Token::preset("int", Preset::Int(IntPreset::default())))
///     .rule("File", seq![lit("["), sep(r("int"), ","), lit("]")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("[1, 2,]"), 0);
/// assert_eq!(problems("[1 2]"), 1);
/// ```
pub fn sep(item: Pattern, separator: impl Into<String>) -> Pattern {
    new(Shape::Separated {
        item: Box::new(item),
        separator: separator.into(),
        strict: false,
    })
}

/// Any number of items with `separator` between them, and not after the last.
///
/// ```
/// use langmeld_syntax::pattern::{lit, r, sep_strict};
/// use langmeld_syntax::{FileId, IdentStyle, IntPreset, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .token(Token::preset("int", Preset::Int(IntPreset::default())))
///     .rule("File", seq![lit("["), sep_strict(r("int"), ","), lit("]")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("[1, 2]"), 0);
/// assert_eq!(problems("[1, 2,]"), 1);
/// ```
pub fn sep_strict(item: Pattern, separator: impl Into<String>) -> Pattern {
    new(Shape::Separated {
        item: Box::new(item),
        separator: separator.into(),
        strict: true,
    })
}

/// The part followed by the end of a line or another terminator, which may be left
/// out before `}` and at the end of the file. See
/// [`LanguageBuilder::terminators`](crate::LanguageBuilder::terminators).
///
/// ```
/// use langmeld_syntax::pattern::{lit, many, terminated};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"[ \t\r\n]+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .terminators([";"])
///     .rule("File", many(terminated(lit("go"))))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("go\ngo; go"), 0);
/// assert_eq!(problems("go go"), 1);
/// ```
pub fn terminated(part: Pattern) -> Pattern {
    new(Shape::Terminated(Box::new(part)))
}

/// Names a part, so that the tree's typed view can offer it.
///
/// ```
/// use langmeld_syntax::pattern::{field, lit, r};
/// use langmeld_syntax::{FileId, IdentStyle, IntPreset, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .token(Token::preset("int", Preset::Int(IntPreset::default())))
///     .rule("File", seq![lit("let"), field("value", r("int"))])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("let 1"), 0);
/// assert_eq!(problems("let"), 1);
/// ```
pub fn field(name: impl Into<String>, part: Pattern) -> Pattern {
    new(Shape::Field(name.into(), Box::new(part)))
}

/// Requires that the part comes next, without taking it.
///
/// ```
/// use langmeld_syntax::pattern::{lit, look};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", seq![lit("a"), look(lit("b")), lit("b")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a b"), 0);
/// assert_eq!(problems("a a"), 1);
/// ```
pub fn look(part: Pattern) -> Pattern {
    new(Shape::Look(Box::new(part)))
}

/// Requires that the part does not come next.
///
/// ```
/// use langmeld_syntax::pattern::{lit, not};
/// use langmeld_syntax::{FileId, IdentStyle, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", seq![not(lit("a")), lit("b")])
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("b"), 0);
/// ```
pub fn not(part: Pattern) -> Pattern {
    new(Shape::Not(Box::new(part)))
}

/// Parses the part if that works without errors, and otherwise goes on as if it had
/// not been tried. In a choice, attempted alternatives come first, in order.
///
/// ```
/// use langmeld_syntax::pattern::{attempt, lit, many};
/// use langmeld_syntax::{choice, FileId, IdentStyle, Language, parse, Preset, seq, SourceFile, Token};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many(choice![attempt(seq![lit("a"), lit("b")]), seq![lit("a"), lit("c")]]))
///     .build()
///     .expect("the grammar is valid");
/// let problems = |text: &str| {
///     let file = SourceFile::new(FileId::new(0), "example", text).expect("small");
///     parse(&language, &file).diagnostics.len()
/// };
/// assert_eq!(problems("a b a c"), 0);
/// assert_eq!(problems("a d"), 1);
/// ```
pub fn attempt(part: Pattern) -> Pattern {
    new(Shape::Attempt(Box::new(part)))
}
