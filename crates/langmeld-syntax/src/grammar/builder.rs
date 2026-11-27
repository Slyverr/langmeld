//! Declaring a language.

use std::sync::Arc;

use crate::{Diagnostic, LexCursor, LexLimits, ParseLimits, Parser, Preset};

use super::{Language, Pattern};

pub(crate) type LexFn = dyn Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync;
pub(crate) type ParseFn = dyn Fn(&mut Parser<'_>) + Send + Sync;

#[derive(Clone)]
pub(crate) enum TokenShape {
    Regex(String),
    Preset(Preset),
    External(Arc<LexFn>),
}

/// A token of a language, referred to by name in its rules.
///
/// Literals such as `;` need no declaration: rules declare them by using them.
///
/// ```
/// use langmeld_syntax::pattern::{lit, many, r};
/// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
///
/// let language = Language::builder("Tags")
///     // Text that rules never see.
///     .token(Token::skip("space", r"\s+"))
///     // A ready-made kind of token.
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     // Text matching a regular expression.
///     .token(Token::regex("colour", r"#[0-9a-f]{6}"))
///     .rule("File", many(seq![lit("tag"), r("word"), r("colour")]))
///     .build()
///     .expect("the grammar is valid");
/// let file = SourceFile::new(FileId::new(0), "example", "tag sky #00aaff")?;
/// assert!(parse(&language, &file).diagnostics.is_empty());
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone)]
pub struct Token {
    pub(crate) name: String,
    pub(crate) shape: TokenShape,
    pub(crate) trivia: bool,
    pub(crate) identifier: bool,
}

impl Token {
    fn new(name: impl Into<String>, shape: TokenShape) -> Self {
        let identifier = matches!(shape, TokenShape::Preset(Preset::Ident(_)));
        Self {
            name: name.into(),
            shape,
            trivia: false,
            identifier,
        }
    }

    /// Text matching a regular expression.
    ///
    /// ```
    /// use langmeld_syntax::Token;
    ///
    /// let _hex = Token::regex("hex", "0x[0-9a-f]+");
    /// ```
    pub fn regex(name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self::new(name, TokenShape::Regex(pattern.into()))
    }

    /// Text that rules never see, such as whitespace.
    ///
    /// ```
    /// use langmeld_syntax::Token;
    ///
    /// let _space = Token::skip("space", r"\s+");
    /// ```
    pub fn skip(name: impl Into<String>, pattern: impl Into<String>) -> Self {
        let mut token = Self::regex(name, pattern);
        token.trivia = true;
        token
    }

    /// A ready-made kind of token, such as numbers or identifiers.
    ///
    /// ```
    /// use langmeld_syntax::{IdentStyle, Preset, Token};
    ///
    /// let _name = Token::preset("name", Preset::Ident(IdentStyle::Xid));
    /// ```
    pub fn preset(name: impl Into<String>, preset: Preset) -> Self {
        Self::new(name, TokenShape::Preset(preset))
    }

    /// A token recognised by Rust code; see
    /// [`TokenSpec::external`](crate::TokenSpec::external).
    ///
    /// ```
    /// use langmeld_syntax::pattern::{many, r};
    /// use langmeld_syntax::{FileId, Language, SourceFile, Token, parse};
    ///
    /// // `#` followed by digits, found by counting them.
    /// let language = Language::builder("Refs")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::external("ref", |cursor| {
    ///         let digits = cursor.rest().strip_prefix('#')?;
    ///         let count = digits.bytes().take_while(u8::is_ascii_digit).count();
    ///         (count > 0).then_some(1 + count)
    ///     }))
    ///     .rule("File", many(r("ref")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "#1 #22")?;
    /// assert!(parse(&language, &file).diagnostics.is_empty());
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn external(
        name: impl Into<String>,
        lex: impl Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync + 'static,
    ) -> Self {
        Self::new(name, TokenShape::External(Arc::new(lex)))
    }

    /// Marks the token as an identifier, so that reserved words are recognised among
    /// its matches. The identifier preset already is one.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, r};
    /// use langmeld_syntax::{FileId, Language, SourceFile, Token, parse, seq};
    ///
    /// // A name with dashes, in which `let` is still recognised as a reserved word.
    /// let language = Language::builder("Dashes")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::regex("name", r"[a-z]+(-[a-z]+)*").identifier())
    ///     .rule("File", seq![lit("let"), r("name")])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "let my-name")?;
    /// assert!(parse(&language, &file).diagnostics.is_empty());
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn identifier(mut self) -> Self {
        self.identifier = true;
        self
    }
}

/// How operators of the same precedence group.
///
/// ```
/// use langmeld_syntax::Assoc;
///
/// // `a - b - c` is `(a - b) - c`, but `a = b = c` is `a = (b = c)`.
/// assert_ne!(Assoc::Left, Assoc::Right);
/// // `a < b < c` is an error.
/// assert_ne!(Assoc::None, Assoc::Left);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    /// `a - b - c` is `(a - b) - c`.
    Left,
    /// `a = b = c` is `a = (b = c)`.
    Right,
    /// `a < b < c` is an error.
    None,
}

#[derive(Clone)]
pub(crate) enum Operator {
    Prefix {
        name: String,
        symbols: Vec<String>,
        precedence: u16,
    },
    Infix {
        name: String,
        symbols: Vec<String>,
        precedence: u16,
        assoc: Assoc,
    },
    Postfix {
        name: String,
        body: Pattern,
        precedence: u16,
    },
}

/// Expressions: atoms joined by operators with precedence.
///
/// Each atom and operator becomes a node of its name. A larger precedence binds
/// tighter; precedences start at 1.
///
/// ```
/// use langmeld_syntax::pattern::{lit, r};
/// use langmeld_syntax::{Assoc, ExprSpec, seq};
///
/// let expressions = ExprSpec::new("Expr")
///     .atom("Literal", r("int"))
///     .atom("Paren", seq![lit("("), r("Expr"), lit(")")])
///     .prefix("Neg", ["-"], 9)
///     .infix("Mul", ["*", "/"], 6, Assoc::Left)
///     .infix("Add", ["+", "-"], 5, Assoc::Left);
/// ```
#[derive(Clone)]
pub struct ExprSpec {
    pub(crate) name: String,
    pub(crate) atoms: Vec<(String, Pattern)>,
    pub(crate) operators: Vec<Operator>,
}

impl ExprSpec {
    /// An expression rule called `name`.
    ///
    /// ```
    /// use langmeld_syntax::ExprSpec;
    ///
    /// let _expressions = ExprSpec::new("Expr");
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            atoms: Vec::new(),
            operators: Vec::new(),
        }
    }

    /// An operand. It becomes a node called `name`.
    ///
    /// ```
    /// use langmeld_syntax::ExprSpec;
    /// use langmeld_syntax::pattern::{lit, r};
    /// use langmeld_syntax::seq;
    ///
    /// let _expressions = ExprSpec::new("Expr")
    ///     .atom("Number", r("int"))
    ///     .atom("Group", seq![lit("("), r("Expr"), lit(")")]);
    /// ```
    pub fn atom(mut self, name: impl Into<String>, body: Pattern) -> Self {
        self.atoms.push((name.into(), body));
        self
    }

    /// Operators written before their operand, such as `-`.
    ///
    /// ```
    /// use langmeld_syntax::ExprSpec;
    /// use langmeld_syntax::pattern::r;
    ///
    /// let _expressions = ExprSpec::new("Expr")
    ///     .atom("Number", r("int"))
    ///     .prefix("Negate", ["-", "!"], 9);
    /// ```
    pub fn prefix<S: Into<String>>(
        mut self,
        name: impl Into<String>,
        symbols: impl IntoIterator<Item = S>,
        precedence: u16,
    ) -> Self {
        self.operators.push(Operator::Prefix {
            name: name.into(),
            symbols: symbols.into_iter().map(Into::into).collect(),
            precedence,
        });
        self
    }

    /// Operators written between two operands, which group as `assoc` says.
    ///
    /// ```
    /// use langmeld_syntax::pattern::r;
    /// use langmeld_syntax::{Assoc, ExprSpec};
    ///
    /// let _expressions = ExprSpec::new("Expr")
    ///     .atom("Number", r("int"))
    ///     .infix("Power", ["**"], 8, Assoc::Right)
    ///     .infix("Sum", ["+", "-"], 5, Assoc::Left)
    ///     .infix("Compare", ["<", "=="], 4, Assoc::None);
    /// ```
    pub fn infix<S: Into<String>>(
        mut self,
        name: impl Into<String>,
        symbols: impl IntoIterator<Item = S>,
        precedence: u16,
        assoc: Assoc,
    ) -> Self {
        self.operators.push(Operator::Infix {
            name: name.into(),
            symbols: symbols.into_iter().map(Into::into).collect(),
            precedence,
            assoc,
        });
        self
    }

    /// An operator written after its operand, such as a call. `body` must start with
    /// a literal.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{field, lit, r, sep};
    /// use langmeld_syntax::{ExprSpec, seq};
    ///
    /// let _expressions = ExprSpec::new("Expr")
    ///     .atom("Name", r("ident"))
    ///     .postfix(
    ///         "Call",
    ///         seq![lit("("), field("args", sep(r("Expr"), ",")), lit(")")],
    ///         10,
    ///     );
    /// ```
    pub fn postfix(mut self, name: impl Into<String>, body: Pattern, precedence: u16) -> Self {
        self.operators.push(Operator::Postfix {
            name: name.into(),
            body,
            precedence,
        });
        self
    }
}

/// A rule parsed by Rust code, for what a grammar cannot express.
///
/// The function uses the [`Parser`] primitives, and can call declared rules with
/// [`Parser::rule`] and look up kinds with [`Parser::kind_of`].
#[derive(Clone)]
/// ```
/// use langmeld_syntax::pattern::{many, r};
/// use langmeld_syntax::{ExternRule, FileId, IdentStyle, Language, Preset, SourceFile, Token, parse};
///
/// // A rule that makes one node from two words, and calls `Name` for each.
/// let language = Language::builder("Pairs")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many(r("Pair")))
///     .rule("Name", r("word"))
///     .extern_rule(
///         ExternRule::new("Pair", |p| {
///             let pair = p.start();
///             p.rule("Name");
///             p.rule("Name");
///             let kind = p.kind_of("Couple");
///             pair.complete(p, kind);
///         })
///         .first(["word"])
///         .produces(["Couple"]),
///     )
///     .build()
///     .expect("the grammar is valid");
/// let file = SourceFile::new(FileId::new(0), "example", "a b c d")?;
/// let parsed = parse(&language, &file);
/// assert!(parsed.diagnostics.is_empty());
/// assert_eq!(parsed.syntax().children().count(), 2);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub struct ExternRule {
    pub(crate) name: String,
    pub(crate) parse: Arc<ParseFn>,
    pub(crate) first: Vec<String>,
    pub(crate) uses: Vec<String>,
    pub(crate) produces: Vec<String>,
}

impl ExternRule {
    /// A rule called `name`, parsed by `parse`.
    ///
    /// ```
    /// use langmeld_syntax::ExternRule;
    ///
    /// // Takes whatever token is next.
    /// let _skip = ExternRule::new("Skip", |p| p.bump());
    /// ```
    pub fn new(
        name: impl Into<String>,
        parse: impl Fn(&mut Parser<'_>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            parse: Arc::new(parse),
            first: Vec::new(),
            uses: Vec::new(),
            produces: Vec::new(),
        }
    }

    /// The literals or tokens the rule can start with. Without them it may start with
    /// anything, which limits what the grammar can check.
    ///
    /// ```
    /// use langmeld_syntax::ExternRule;
    ///
    /// let _rule = ExternRule::new("Group", |p| p.bump()).first(["("]);
    /// ```
    pub fn first<S: Into<String>>(mut self, symbols: impl IntoIterator<Item = S>) -> Self {
        self.first.extend(symbols.into_iter().map(Into::into));
        self
    }

    /// Further literals the function reads, so that the lexer knows them.
    ///
    /// ```
    /// use langmeld_syntax::ExternRule;
    ///
    /// // The function reads `)` and `=>` itself, so the lexer must know them.
    /// let _rule = ExternRule::new("Arrow", |p| p.bump()).first(["("]).uses([")", "=>"]);
    /// ```
    pub fn uses<S: Into<String>>(mut self, literals: impl IntoIterator<Item = S>) -> Self {
        self.uses.extend(literals.into_iter().map(Into::into));
        self
    }

    /// The names of the nodes the function makes.
    ///
    /// ```
    /// use langmeld_syntax::ExternRule;
    ///
    /// let _rule = ExternRule::new("Arrow", |p| p.bump()).produces(["Lambda", "Group"]);
    /// ```
    pub fn produces<S: Into<String>>(mut self, nodes: impl IntoIterator<Item = S>) -> Self {
        self.produces.extend(nodes.into_iter().map(Into::into));
        self
    }
}

#[derive(Clone)]
pub(crate) enum Comment {
    Line(String),
    Doc(String),
    Block {
        open: String,
        close: String,
        nested: bool,
    },
}

#[derive(Clone)]
pub(crate) enum Definition {
    Rule(String, Pattern),
    Expr(ExprSpec),
    Extern(ExternRule),
}

impl Definition {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Rule(name, _) => name,
            Self::Expr(expr) => &expr.name,
            Self::Extern(rule) => &rule.name,
        }
    }
}

/// Collects the declarations of a [`Language`].
#[derive(Clone)]
/// ```
/// use langmeld_syntax::pattern::{lit, many, r};
/// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
///
/// let language = Language::builder("Doc")
///     .token(Token::skip("space", r"[ \t\r\n]+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
///     .rule("File", many(seq![lit("let"), r("word")]))
///     .build()
///     .expect("the grammar is valid");
/// let file = SourceFile::new(FileId::new(0), "example", "let a let b")?;
/// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub struct LanguageBuilder {
    pub(crate) name: String,
    pub(crate) tokens: Vec<Token>,
    pub(crate) keywords: Vec<String>,
    pub(crate) contextual: Vec<String>,
    pub(crate) comments: Vec<Comment>,
    /// Extra terminators, if newlines end statements.
    pub(crate) terminators: Option<Vec<String>>,
    /// Pairs inside which lines continue, if not the usual ones.
    pub(crate) continue_inside: Option<Vec<(String, String)>>,
    /// Further places to resume for a rule.
    pub(crate) recover: Vec<(String, Vec<String>)>,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) root: Option<String>,
    pub(crate) descriptions: Vec<(String, String)>,
    pub(crate) lex_limits: LexLimits,
    pub(crate) parse_limits: ParseLimits,
}

impl LanguageBuilder {
    pub(super) fn new(name: String) -> Self {
        Self {
            name,
            tokens: Vec::new(),
            keywords: Vec::new(),
            contextual: Vec::new(),
            comments: Vec::new(),
            terminators: None,
            continue_inside: None,
            recover: Vec::new(),
            definitions: Vec::new(),
            root: None,
            descriptions: Vec::new(),
            lex_limits: LexLimits::default(),
            parse_limits: ParseLimits::default(),
        }
    }

    /// Declares a token.
    ///
    /// ```
    /// use langmeld_syntax::{Language, Token};
    ///
    /// let _builder = Language::builder("Doc").token(Token::skip("space", r"\s+"));
    /// ```
    pub fn token(mut self, token: Token) -> Self {
        self.tokens.push(token);
        self
    }

    /// Reserves words that no rule mentions. Words that rules use are reserved
    /// already.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .keywords(["spare"])
    ///     .rule("File", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a b")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn keywords<S: Into<String>>(mut self, words: impl IntoIterator<Item = S>) -> Self {
        self.keywords.extend(words.into_iter().map(Into::into));
        self
    }

    /// Words that mean something only where a rule asks for them, and are ordinary
    /// identifiers elsewhere.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .contextual(["from"])
    ///     .rule("File", seq![lit("move"), r("word"), lit("from"), r("word")])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "move from from from")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn contextual<S: Into<String>>(mut self, words: impl IntoIterator<Item = S>) -> Self {
        self.contextual.extend(words.into_iter().map(Into::into));
        self
    }

    /// Comments from `open` to the end of the line.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .line_comment("//")
    ///     .rule("File", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a // and more\nb")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn line_comment(mut self, open: impl Into<String>) -> Self {
        self.comments.push(Comment::Line(open.into()));
        self
    }

    /// Documentation comments, which belong to the node after them.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .doc_comment("///")
    ///     .rule("File", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "/// about a\na")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn doc_comment(mut self, open: impl Into<String>) -> Self {
        self.comments.push(Comment::Doc(open.into()));
        self
    }

    /// Comments from `open` to the first `close`.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .block_comment("/*", "*/")
    ///     .rule("File", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a /* b */ c")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn block_comment(mut self, open: impl Into<String>, close: impl Into<String>) -> Self {
        self.comments.push(Comment::Block {
            open: open.into(),
            close: close.into(),
            nested: false,
        });
        self
    }

    /// Block comments that can contain block comments.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .nested_block_comment("/*", "*/")
    ///     .rule("File", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a /* b /* c */ */ d")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn nested_block_comment(
        mut self,
        open: impl Into<String>,
        close: impl Into<String>,
    ) -> Self {
        self.comments.push(Comment::Block {
            open: open.into(),
            close: close.into(),
            nested: true,
        });
        self
    }

    /// Lets the end of a line terminate statements, as may any of the `extra`
    /// literals. Used with [`terminated`](crate::pattern::terminated).
    ///
    /// Lines continue inside `(` `)` and `[` `]`, and after a binary operator or a
    /// comma.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{many, r, terminated};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .terminators([";"])
    ///     .rule("File", many(terminated(r("word"))))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a\nb; c")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn terminators<S: Into<String>>(mut self, extra: impl IntoIterator<Item = S>) -> Self {
        self.terminators
            .get_or_insert_default()
            .extend(extra.into_iter().map(Into::into));
        self
    }

    /// The pairs of literals inside which the end of a line terminates nothing,
    /// instead of `(` `)` and `[` `]`.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r, terminated};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .terminators([";"])
    ///     .continue_inside([("<", ">")])
    ///     .rule("File", many(terminated(seq![lit("<"), many(r("word")), lit(">")])))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "<a\nb>\n<c>")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn continue_inside<S: Into<String>>(
        mut self,
        pairs: impl IntoIterator<Item = (S, S)>,
    ) -> Self {
        self.continue_inside = Some(
            pairs
                .into_iter()
                .map(|(open, close)| (open.into(), close.into()))
                .collect(),
        );
        self
    }

    /// Lets parsing resume at these literals after unexpected text inside `rule`, in
    /// addition to the places worked out from the grammar.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .recover("Item", ["!"])
    ///     .rule("File", many(r("Item")))
    ///     .rule("Item", seq![lit("{"), many(r("word")), lit("}")])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "{ a b } { c }")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn recover<S: Into<String>>(
        mut self,
        rule: impl Into<String>,
        literals: impl IntoIterator<Item = S>,
    ) -> Self {
        self.recover
            .push((rule.into(), literals.into_iter().map(Into::into).collect()));
        self
    }

    /// Declares a rule. A rule that is a choice of other rules adds no node of its
    /// own; every other rule becomes a node of its name.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .rule("File", r("Greeting"))
    ///     .rule("Greeting", seq![lit("hello"), r("word")])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "hello world")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn rule(mut self, name: impl Into<String>, body: Pattern) -> Self {
        self.definitions.push(Definition::Rule(name.into(), body));
        self
    }

    /// Declares expressions.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, r};
    /// use langmeld_syntax::{Assoc, ExprSpec, FileId, IntPreset, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Sums")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::preset("int", Preset::Int(IntPreset::default())))
    ///     .rule("File", r("Expr"))
    ///     .expr(
    ///         ExprSpec::new("Expr")
    ///             .atom("Number", r("int"))
    ///             .atom("Group", seq![lit("("), r("Expr"), lit(")")])
    ///             .infix("Sum", ["+"], 5, Assoc::Left),
    ///     )
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "1 + (2 + 3)")?;
    /// assert!(parse(&language, &file).diagnostics.is_empty());
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn expr(mut self, expr: ExprSpec) -> Self {
        self.definitions.push(Definition::Expr(expr));
        self
    }

    /// Declares a rule parsed by Rust code.
    ///
    /// ```
    /// use langmeld_syntax::pattern::r;
    /// use langmeld_syntax::{ExternRule, FileId, IdentStyle, Language, Preset, SourceFile, Token, parse};
    ///
    /// let language = Language::builder("Any")
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .rule("File", r("Word"))
    ///     .extern_rule(ExternRule::new("Word", |p| p.bump()).first(["word"]))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "hello")?;
    /// assert!(parse(&language, &file).diagnostics.is_empty());
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn extern_rule(mut self, rule: ExternRule) -> Self {
        self.definitions.push(Definition::Extern(rule));
        self
    }

    /// The rule a whole file is parsed with. By default, the first rule declared.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"[ \t\r\n]+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .root("Start")
    ///     .rule("Other", lit("!"))
    ///     .rule("Start", many(r("word")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a b")?;
    /// assert_eq!(parse(&language, &file).diagnostics.len(), 0);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn root(mut self, rule: impl Into<String>) -> Self {
        self.root = Some(rule.into());
        self
    }

    /// How errors name a rule or a token, such as `a statement`. By default a rule is
    /// named in words and a token by its name.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq};
    ///
    /// let language = Language::builder("Doc")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .rule("File", many(r("Greeting")))
    ///     .rule("Greeting", seq![lit("hello"), r("word")])
    ///     .describe("Greeting", "a greeting")
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "bye")?;
    /// let parsed = parse(&language, &file);
    /// assert_eq!(parsed.diagnostics[0].message, "expected a greeting, found `bye`");
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn describe(mut self, rule: impl Into<String>, text: impl Into<String>) -> Self {
        self.descriptions.push((rule.into(), text.into()));
        self
    }

    /// Replaces the default limits of the lexer.
    ///
    /// ```
    /// use langmeld_syntax::{Language, LexLimits};
    ///
    /// let _builder = Language::builder("Doc").lex_limits(LexLimits::default().max_len(1 << 20));
    /// ```
    pub fn lex_limits(mut self, limits: LexLimits) -> Self {
        self.lex_limits = limits;
        self
    }

    /// Replaces the default limits of the parser.
    ///
    /// ```
    /// use langmeld_syntax::{Language, ParseLimits};
    ///
    /// let _builder = Language::builder("Doc").parse_limits(ParseLimits::default().max_depth(64));
    /// ```
    pub fn parse_limits(mut self, limits: ParseLimits) -> Self {
        self.parse_limits = limits;
        self
    }

    /// Checks the grammar and prepares it for parsing. Reports every problem found,
    /// each with a note that says how to fix it. They are about the declarations, not
    /// about a source file, so their file is `FileId::new(0)` and their range is empty.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{many, r};
    /// use langmeld_syntax::{IdentStyle, Language, Preset, Token};
    ///
    /// let problems = Language::builder("Names")
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Xid)))
    ///     .rule("File", many(r("Wrod")))
    ///     .build()
    ///     .expect_err("`Wrod` is not declared");
    /// assert_eq!(problems[0].code, "LM-G001");
    /// assert_eq!(
    /// problems[0].message,
    /// "`File` uses `Wrod`, which is not a rule or a token",
    /// );
    /// ```
    pub fn build(self) -> Result<Language, Vec<Diagnostic>> {
        super::compile::compile(self)
    }
}
