//! How the tokens of a language are declared.

use std::sync::Arc;

use crate::SyntaxKind;

use super::LexCursor;

pub(crate) type ExternalFn = dyn Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum Matcher {
    Punct(String),
    Keyword(String),
    Contextual(String),
    Regex(String),
    Preset(Preset),
    External(#[serde(with = "crate::embed::token_code")] Arc<ExternalFn>),
    LineComment(String),
    BlockComment { open: String, close: String },
}

/// The declaration of one kind of token.
///
/// ```
/// use langmeld_syntax::{IdentStyle, Preset, SyntaxKind, TokenSpec};
///
/// const NAME: SyntaxKind = SyntaxKind(1);
/// const LET: SyntaxKind = SyntaxKind(2);
/// const ARROW: SyntaxKind = SyntaxKind(3);
/// const SPACE: SyntaxKind = SyntaxKind(4);
///
/// let tokens = [
///     TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Xid)),
///     TokenSpec::keyword(LET, "let"),
///     TokenSpec::punct(ARROW, "->"),
///     TokenSpec::regex(SPACE, "space", r"[ \t]+").skip(),
/// ];
/// assert_eq!(tokens.len(), 4);
/// ```
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenSpec {
    pub(crate) kind: SyntaxKind,
    /// How errors name the token. For fixed text, the text itself.
    pub(crate) name: String,
    pub(crate) matcher: Matcher,
    pub(crate) trivia: bool,
    pub(crate) doc: bool,
    pub(crate) identifier: bool,
    pub(crate) nested: bool,
}

impl TokenSpec {
    fn new(kind: SyntaxKind, name: impl Into<String>, matcher: Matcher) -> Self {
        Self {
            kind,
            name: name.into(),
            matcher,
            trivia: false,
            doc: false,
            identifier: false,
            nested: false,
        }
    }

    /// Fixed text such as `;` or `=>`. Only declared text is ever joined into one
    /// token.
    pub fn punct(kind: SyntaxKind, text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(kind, text.clone(), Matcher::Punct(text))
    }

    /// A reserved word. An identifier token with exactly this text becomes `kind`.
    pub fn keyword(kind: SyntaxKind, word: impl Into<String>) -> Self {
        let word = word.into();
        Self::new(kind, word.clone(), Matcher::Keyword(word))
    }

    /// A word with a special meaning in some places only. The lexer leaves it an
    /// identifier; a parser asks for it where it matters.
    pub fn contextual(kind: SyntaxKind, word: impl Into<String>) -> Self {
        let word = word.into();
        Self::new(kind, word.clone(), Matcher::Contextual(word))
    }

    /// Text matching a regular expression, in the syntax of the `regex` crate.
    /// Anchors and word boundaries are not supported. `name` is used in errors.
    pub fn regex(kind: SyntaxKind, name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self::new(kind, name, Matcher::Regex(pattern.into()))
    }

    /// A ready-made kind of token, such as numbers or strings.
    pub fn preset(kind: SyntaxKind, name: impl Into<String>, preset: Preset) -> Self {
        let identifier = matches!(preset, Preset::Ident(_));
        let mut spec = Self::new(kind, name, Matcher::Preset(preset));
        spec.identifier = identifier;
        spec
    }

    /// A token recognised by Rust code.
    ///
    /// `lex` is called with the text still to be read and returns how many bytes of it
    /// form the token. An answer that is zero, too large or inside a character counts
    /// as `None`.
    pub fn external(
        kind: SyntaxKind,
        name: impl Into<String>,
        lex: impl Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync + 'static,
    ) -> Self {
        Self::new(kind, name, Matcher::External(Arc::new(lex)))
    }

    /// A comment from `open` to the end of the line. Comments are trivia.
    pub fn line_comment(kind: SyntaxKind, open: impl Into<String>) -> Self {
        let open = open.into();
        Self::new(kind, open.clone(), Matcher::LineComment(open)).skip()
    }

    /// A comment from `open` to `close`. Comments are trivia.
    pub fn block_comment(
        kind: SyntaxKind,
        open: impl Into<String>,
        close: impl Into<String>,
    ) -> Self {
        let (open, close) = (open.into(), close.into());
        Self::new(kind, open.clone(), Matcher::BlockComment { open, close }).skip()
    }

    /// Lets a block comment contain further block comments.
    pub fn nested(mut self) -> Self {
        self.nested = true;
        self
    }

    /// Makes the token trivia, which parsers do not see.
    pub fn skip(mut self) -> Self {
        self.trivia = true;
        self
    }

    /// Makes the token a documentation comment: trivia that belongs to the node after
    /// it.
    pub fn doc(mut self) -> Self {
        self.doc = true;
        self.skip()
    }

    /// Marks a token declared with a pattern or by Rust as an identifier, so that
    /// keywords are recognised among its matches. The identifier preset already is
    /// one.
    pub fn identifier(mut self) -> Self {
        self.identifier = true;
        self
    }
}

/// Ready-made kinds of token.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Preset {
    /// Whole numbers.
    Int(IntPreset),
    /// Numbers with a fractional part or an exponent.
    Float(FloatPreset),
    /// Quoted text that ends on the line where it starts.
    String(StringPreset),
    /// One character in single quotes, possibly written with an escape.
    Char,
    /// Names.
    Ident(IdentStyle),
}

/// Which whole numbers to accept. Decimal numbers always are.
///
/// ```
/// use langmeld_syntax::IntPreset;
///
/// let numbers = IntPreset::default().hex(true).underscores(true);
/// assert!(numbers.hex && numbers.underscores && !numbers.binary);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct IntPreset {
    /// `0x1f`.
    pub hex: bool,
    /// `0b101`.
    pub binary: bool,
    /// `0o17`.
    pub octal: bool,
    /// `1_000_000`.
    pub underscores: bool,
}

impl IntPreset {
    /// Turns `0x1f` on or off.
    #[must_use]
    pub fn hex(mut self, on: bool) -> Self {
        self.hex = on;
        self
    }

    /// Turns `0b101` on or off.
    #[must_use]
    pub fn binary(mut self, on: bool) -> Self {
        self.binary = on;
        self
    }

    /// Turns `0o17` on or off.
    #[must_use]
    pub fn octal(mut self, on: bool) -> Self {
        self.octal = on;
        self
    }

    /// Turns `1_000_000` on or off.
    #[must_use]
    pub fn underscores(mut self, on: bool) -> Self {
        self.underscores = on;
        self
    }
}

/// Which fractional numbers to accept. A digit is required after the point.
///
/// ```
/// use langmeld_syntax::FloatPreset;
///
/// let numbers = FloatPreset::default().exponent(true);
/// assert!(numbers.exponent && !numbers.underscores);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FloatPreset {
    /// `1.5e-3` and `3e7`.
    pub exponent: bool,
    /// `1_000.5`.
    pub underscores: bool,
}

impl FloatPreset {
    /// Turns `1.5e-3` and `3e7` on or off.
    #[must_use]
    pub fn exponent(mut self, on: bool) -> Self {
        self.exponent = on;
        self
    }

    /// Turns `1_000.5` on or off.
    #[must_use]
    pub fn underscores(mut self, on: bool) -> Self {
        self.underscores = on;
        self
    }
}

/// Which strings to accept. At least one of `double` and `single` must be on.
///
/// ```
/// use langmeld_syntax::StringPreset;
///
/// let strings = StringPreset::default().double(true).raw(true);
/// assert!(strings.double && strings.raw && !strings.single);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct StringPreset {
    /// `"text"`.
    pub double: bool,
    /// `'text'`.
    pub single: bool,
    /// `r"text"`, in which a backslash is an ordinary character.
    pub raw: bool,
    /// Holes holding code, as in `"a {b} c"`. Raw strings have none.
    pub interpolation: Option<Interpolation>,
}

impl StringPreset {
    /// Turns `"text"` on or off.
    #[must_use]
    pub fn double(mut self, on: bool) -> Self {
        self.double = on;
        self
    }

    /// Turns `'text'` on or off.
    #[must_use]
    pub fn single(mut self, on: bool) -> Self {
        self.single = on;
        self
    }

    /// Turns `r"text"` on or off.
    #[must_use]
    pub fn raw(mut self, on: bool) -> Self {
        self.raw = on;
        self
    }

    /// Lets strings hold code in holes.
    #[must_use]
    pub fn interpolation(mut self, interpolation: Interpolation) -> Self {
        self.interpolation = Some(interpolation);
        self
    }
}

/// How holes in strings are written.
///
/// An interpolated string becomes several tokens: its start, the text between holes,
/// each hole's delimiters around ordinary tokens, and its end.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Interpolation {
    /// What opens a hole, such as `{` or `${`.
    pub open: String,
    /// What closes it. If this is `}`, `)` or `]`, matching pairs may nest inside.
    pub close: String,
    /// The kinds of the tokens that make up the string.
    pub kinds: StringParts,
}

impl Interpolation {
    /// Holes between `open` and `close`.
    pub fn new(open: impl Into<String>, close: impl Into<String>, kinds: StringParts) -> Self {
        Self {
            open: open.into(),
            close: close.into(),
            kinds,
        }
    }
}

/// The token kinds of an interpolated string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StringParts {
    /// The opening quote.
    pub start: SyntaxKind,
    /// Text between quotes and holes.
    pub text: SyntaxKind,
    /// The closing quote.
    pub end: SyntaxKind,
    /// What opens a hole.
    pub hole_open: SyntaxKind,
    /// What closes a hole.
    pub hole_close: SyntaxKind,
}

/// Which characters identifiers are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum IdentStyle {
    /// Letters and digits of every script (Unicode `XID`), and `_`.
    Xid,
    /// `A` to `Z`, `a` to `z`, digits and `_`.
    Ascii,
}

/// Newlines that end statements.
///
/// With this configured, a run of line endings, blank lines and comments after a token
/// becomes one `newline` token. Other line endings are trivia.
///
/// ```
/// use langmeld_syntax::{SyntaxKind, Terminators};
///
/// const NEWLINE: SyntaxKind = SyntaxKind(1);
/// const LINE_BREAK: SyntaxKind = SyntaxKind(2);
/// const L_PAREN: SyntaxKind = SyntaxKind(3);
/// const R_PAREN: SyntaxKind = SyntaxKind(4);
///
/// let terminators = Terminators::new(NEWLINE, LINE_BREAK).brackets([(L_PAREN, R_PAREN)]);
/// assert_eq!(terminators.brackets, [(L_PAREN, R_PAREN)]);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Terminators {
    /// The kind of a line ending that ends a statement.
    pub newline: SyntaxKind,
    /// The kind of every other line ending.
    pub line_break: SyntaxKind,
    /// Pairs of opening and closing kinds between which newlines never terminate,
    /// such as parentheses.
    pub brackets: Vec<(SyntaxKind, SyntaxKind)>,
    /// Pairs between which newlines terminate again, even inside `brackets`, such as
    /// braces.
    pub blocks: Vec<(SyntaxKind, SyntaxKind)>,
    /// A line ending after one of these continues on the next line, as after a binary
    /// operator or a comma.
    pub continue_after: Vec<SyntaxKind>,
}

impl Terminators {
    /// Newline termination with no brackets and no continuation.
    pub fn new(newline: SyntaxKind, line_break: SyntaxKind) -> Self {
        Self {
            newline,
            line_break,
            brackets: Vec::new(),
            blocks: Vec::new(),
            continue_after: Vec::new(),
        }
    }

    /// Sets the pairs between which newlines never terminate.
    #[must_use]
    pub fn brackets(mut self, pairs: impl IntoIterator<Item = (SyntaxKind, SyntaxKind)>) -> Self {
        self.brackets = pairs.into_iter().collect();
        self
    }

    /// Sets the pairs between which newlines terminate again.
    #[must_use]
    pub fn blocks(mut self, pairs: impl IntoIterator<Item = (SyntaxKind, SyntaxKind)>) -> Self {
        self.blocks = pairs.into_iter().collect();
        self
    }

    /// Sets the kinds after which a line ending continues on the next line.
    #[must_use]
    pub fn continue_after(mut self, kinds: impl IntoIterator<Item = SyntaxKind>) -> Self {
        self.continue_after = kinds.into_iter().collect();
        self
    }
}

/// How much text a lexer accepts. Going over a limit is reported as a diagnostic.
///
/// ```
/// use langmeld_syntax::LexLimits;
///
/// let limits = LexLimits::default().max_len(1 << 20);
/// assert_eq!(limits.max_len, 1 << 20);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct LexLimits {
    /// Largest file, in bytes.
    pub max_len: u32,
    /// Most tokens in one file.
    pub max_tokens: u32,
}

impl LexLimits {
    /// Sets the largest file, in bytes.
    #[must_use]
    pub fn max_len(mut self, bytes: u32) -> Self {
        self.max_len = bytes;
        self
    }

    /// Sets the most tokens in one file.
    #[must_use]
    pub fn max_tokens(mut self, count: u32) -> Self {
        self.max_tokens = count;
        self
    }
}

impl Default for LexLimits {
    fn default() -> Self {
        Self {
            max_len: 64 << 20,
            max_tokens: 8 << 20,
        }
    }
}
