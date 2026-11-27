//! The primitives every parser is written with.

mod builder;

use rowan::GreenNode;
use text_size::{TextRange, TextSize};

use crate::lexeme::Vocabulary;
use crate::{Diagnostic, FileId, Lexed, Lexeme, Lexer, SourceFile, SyntaxKind, SyntaxNode};

pub use builder::TreeBuilder;

/// How often a parser may look ahead without taking a token before it is considered
/// stuck. A loop that never takes a token gets here quickly; in a debug build that
/// is an assertion failure, and otherwise the parser stops with a diagnostic.
const FUEL: u32 = 1 << 20;

/// How deep a parser may nest. Going over a limit is reported as a diagnostic.
///
/// ```
/// use langmeld_syntax::ParseLimits;
///
/// let limits = ParseLimits::default().max_depth(64);
/// assert_eq!(limits.max_depth, 64);
/// assert_eq!(limits.max_tree_depth, ParseLimits::default().max_tree_depth);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseLimits {
    /// Most nodes that may be under construction at once. This bounds the recursion
    /// of a parser that starts a node at every level. Parsing stops here.
    pub max_depth: u32,
    /// Greatest height of the tree, which loops can grow without recursion. Nodes
    /// above this height are left out.
    pub max_tree_depth: u32,
}

impl ParseLimits {
    /// Sets the most nodes under construction at once.
    #[must_use]
    pub fn max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    /// Sets the greatest height of the tree.
    #[must_use]
    pub fn max_tree_depth(mut self, depth: u32) -> Self {
        self.max_tree_depth = depth;
        self
    }
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_tree_depth: 1024,
        }
    }
}

/// The result of parsing one file.
///
/// ```
/// use langmeld_syntax::{FileId, Lexer, Parser, SourceFile};
///
/// let lexer = Lexer::builder().build().expect("no declarations, no problems");
/// let file = SourceFile::new(FileId::new(0), "empty", "")?;
/// let parse = Parser::new(&file, lexer.lex(&file), &lexer).finish();
/// assert!(parse.diagnostics.is_empty());
/// assert_eq!(parse.syntax().text_range().len(), 0.into());
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parse {
    /// The tree. It holds every byte of the source, whatever is wrong with it.
    pub green: GreenNode,
    /// Problems from the lexer and the parser, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Parse {
    /// The root of the tree.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

enum Event {
    /// The start of a node. `kind` is `None` until the node is completed, and stays so
    /// if it is abandoned.
    Start {
        kind: Option<SyntaxKind>,
        /// A later start that must open before this one.
        wrapper: Option<usize>,
    },
    Token {
        kind: SyntaxKind,
        count: usize,
    },
    Finish,
}

/// A node under construction.
struct Open {
    event: usize,
    /// Height of the tallest child so far.
    height: u32,
}

/// The start of a node whose kind is not decided yet.
///
/// Finish it with [`complete`](Self::complete) or [`abandon`](Self::abandon). A marker
/// that is simply dropped counts as abandoned.
#[must_use = "complete or abandon the marker"]
pub struct Marker {
    event: usize,
}

impl Marker {
    /// Makes everything taken since [`Parser::start`] a node of `kind`. Nodes started
    /// after this one and still open are abandoned.
    pub fn complete(self, parser: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        parser.complete(self.event, kind)
    }

    /// Gives up on the node. What was taken since stays in the surrounding node.
    pub fn abandon(self, parser: &mut Parser<'_>) {
        parser.close(self.event);
    }
}

/// A finished node, which can still be wrapped in another.
#[derive(Clone, Copy, Debug)]
pub struct CompletedMarker {
    /// `None` if the marker had already been closed.
    event: Option<usize>,
    height: u32,
}

impl CompletedMarker {
    /// Starts a node that contains this one, as when `1` turns out to be the left side
    /// of `1 + 2`.
    ///
    /// ```
    /// use langmeld_syntax::{
    ///     FileId, IdentStyle, Lexer, Parser, Preset, SourceFile, SyntaxKind, TokenSpec,
    /// };
    ///
    /// const NAME: SyntaxKind = SyntaxKind(1);
    /// const PLUS: SyntaxKind = SyntaxKind(2);
    /// const REF: SyntaxKind = SyntaxKind(3);
    /// const SUM: SyntaxKind = SyntaxKind(4);
    ///
    /// let lexer = Lexer::builder()
    ///     .token(TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Ascii)))
    ///     .token(TokenSpec::punct(PLUS, "+"))
    ///     .build()
    ///     .expect("the declarations are valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "a+b")?;
    /// let mut p = Parser::new(&file, lexer.lex(&file), &lexer);
    ///
    /// let first = p.start();
    /// p.bump();
    /// let mut left = first.complete(&mut p, REF);
    /// if p.at(PLUS) {
    ///     // Only now is it known that `a` is the left side of a sum.
    ///     let sum = left.precede(&mut p);
    ///     p.bump();
    ///     let right = p.start();
    ///     p.expect(NAME);
    ///     right.complete(&mut p, REF);
    ///     left = sum.complete(&mut p, SUM);
    /// }
    /// # let _ = left;
    /// let tree = p.finish().syntax();
    /// assert_eq!(tree.kind(), SUM);
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn precede(self, parser: &mut Parser<'_>) -> Marker {
        let marker = parser.start();
        if let Some(event) = self.event {
            if let Some(Event::Start { wrapper, .. }) = parser.events.get_mut(event) {
                *wrapper = Some(marker.event);
            }
            if let Some(open) = parser.open.last_mut() {
                open.height = self.height;
            }
        }
        marker
    }
}

/// Reads tokens and records the shape of the tree.
///
/// A parser is a set of functions that look at the next tokens, take them and mark
/// where nodes start and end. Errors never stop it, and none of its methods panic.
/// After one error, further ones are held back until a token is taken as expected, so
/// that one mistake gives one diagnostic.
///
/// ```
/// use langmeld_syntax::{
///     FileId, IdentStyle, Lexer, Parser, Preset, SourceFile, SyntaxKind, TokenSpec,
/// };
///
/// const NAME: SyntaxKind = SyntaxKind(1);
/// const SEMI: SyntaxKind = SyntaxKind(2);
/// const LIST: SyntaxKind = SyntaxKind(3);
///
/// /// `list = (name ";")*`
/// fn list(p: &mut Parser<'_>) {
///     let node = p.start();
///     while !p.at_end() {
///         if p.eat(NAME) {
///             p.expect(SEMI);
///         } else {
///             p.recover("a name", &[NAME]);
///         }
///     }
///     node.complete(p, LIST);
/// }
///
/// let lexer = Lexer::builder()
///     .token(TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Ascii)))
///     .token(TokenSpec::punct(SEMI, ";"))
///     .build()
///     .expect("the declarations are valid");
/// let file = SourceFile::new(FileId::new(0), "example", "a;b")?;
/// let mut parser = Parser::new(&file, lexer.lex(&file), &lexer);
/// list(&mut parser);
/// let parse = parser.finish();
///
/// assert_eq!(parse.syntax().text(), "a;b");
/// assert_eq!(parse.diagnostics[0].message, "expected `;`, found end of file");
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub struct Parser<'a> {
    file: FileId,
    text: &'a str,
    lexer: &'a Lexer,
    tokens: Vec<Lexeme>,
    /// Indices into `tokens` of those the parser sees.
    visible: Vec<usize>,
    /// The next entry of `visible`.
    at: usize,
    events: Vec<Event>,
    open: Vec<Open>,
    diagnostics: Vec<Diagnostic>,
    /// Set after an error, until a token is taken as expected.
    recovering: bool,
    /// Kinds asked for at the current token.
    expected: Vec<SyntaxKind>,
    fuel: u32,
    /// Set when a limit was reached: the parser then only sees the end of the file.
    halted: bool,
    tree_too_deep: bool,
    limits: ParseLimits,
}

impl<'a> Parser<'a> {
    /// A parser for the tokens of a file. `lexer` is the one that produced them; it
    /// names kinds in messages and tells keywords from identifiers.
    pub fn new(file: &'a SourceFile, lexed: Lexed, lexer: &'a Lexer) -> Self {
        let visible = lexed
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| !builder::is_hidden(token))
            .map(|(index, _)| index)
            .collect();
        let mut parser = Self {
            file: file.id(),
            text: file.text(),
            lexer,
            tokens: lexed.tokens,
            visible,
            at: 0,
            events: Vec::new(),
            open: Vec::new(),
            diagnostics: lexed.diagnostics,
            recovering: false,
            expected: Vec::new(),
            fuel: FUEL,
            halted: false,
            tree_too_deep: false,
            limits: ParseLimits::default(),
        };
        parser.recovering = parser.follows_unknown_text();
        parser
    }

    /// Replaces the default limits.
    pub fn limits(mut self, limits: ParseLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Starts a node at the current token.
    pub fn start(&mut self) -> Marker {
        let event = self.events.len();
        self.events.push(Event::Start {
            kind: None,
            wrapper: None,
        });
        self.open.push(Open { event, height: 0 });
        if self.open.len() > self.limits.max_depth as usize {
            let limit = self.limits.max_depth;
            self.halt("LM-P003", || too_deep(limit));
        }
        Marker { event }
    }

    /// The kind of the current token, or [`SyntaxKind::EOF`]. This does not count as
    /// expecting anything.
    pub fn current(&mut self) -> SyntaxKind {
        self.nth(0)
    }

    /// The kind of the token `n` places after the current one.
    pub fn nth(&mut self, n: usize) -> SyntaxKind {
        match self.fuel.checked_sub(1) {
            Some(fuel) => self.fuel = fuel,
            None => {
                // Only the first time: a parser that was already halted may spin on.
                debug_assert!(
                    self.halted,
                    "the parser looked ahead {FUEL} times without taking a token; \
                     a loop is not making progress"
                );
                self.halt("LM-P004", || {
                    "the parser stopped making progress here".to_owned()
                });
            }
        }
        if self.halted {
            return SyntaxKind::EOF;
        }
        self.token_at(n).map_or(SyntaxKind::EOF, |token| token.kind)
    }

    /// Whether the current token is of `kind`. If it is not, a later error can say
    /// that `kind` was expected.
    pub fn at(&mut self, kind: SyntaxKind) -> bool {
        let found = self.current() == kind;
        if !found && !self.expected.contains(&kind) {
            self.expected.push(kind);
        }
        found
    }

    /// Whether there is no token left.
    pub fn at_end(&mut self) -> bool {
        self.current() == SyntaxKind::EOF
    }

    /// The text of the current token, or nothing at the end.
    pub fn current_text(&mut self) -> &'a str {
        if self.at_end() {
            return "";
        }
        let range = self.current_range();
        self.slice(range)
    }

    /// Where the current token is. At the end, the empty range after the text.
    pub(crate) fn current_range(&self) -> TextRange {
        match self.token_at(0) {
            Some(token) => token.range,
            None => TextRange::empty(TextSize::of(self.text)),
        }
    }

    /// Whether the current token is the contextual keyword `kind`.
    pub fn at_contextual(&mut self, kind: SyntaxKind) -> bool {
        let Some(word) = self.lexer.contextual_text(kind) else {
            return false;
        };
        let current = self.current();
        let found = self.lexer.is_identifier(current) && self.current_text() == word;
        if !found && !self.expected.contains(&kind) {
            self.expected.push(kind);
        }
        found
    }

    /// Whether the next tokens are of these kinds with nothing between them, as `>`
    /// `>` must be to mean `>>`.
    pub fn at_glued(&mut self, kinds: &[SyntaxKind]) -> bool {
        kinds.iter().enumerate().all(|(n, &kind)| {
            self.nth(n) == kind
                && (n + 1 == kinds.len() || self.token_at(n).is_some_and(|token| token.joint))
        })
    }

    /// Takes the current token. Does nothing at the end.
    pub fn bump(&mut self) {
        let kind = self.current();
        self.take(kind, 1);
    }

    /// Takes the current token as one of `kind`.
    pub fn bump_as(&mut self, kind: SyntaxKind) {
        if !self.at_end() {
            self.take(kind, 1);
        }
    }

    /// Takes the next `count` tokens as one token of `kind`. Does nothing unless they
    /// follow each other with nothing between them.
    pub fn bump_glued(&mut self, kind: SyntaxKind, count: usize) {
        let joined = count > 0
            && !self.at_end()
            && (0..count - 1).all(|n| self.token_at(n).is_some_and(|token| token.joint))
            && self.token_at(count - 1).is_some();
        if joined {
            self.take(kind, count);
        }
    }

    /// Takes the current token if it is of `kind`.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        let found = self.at(kind);
        if found {
            self.bump();
        }
        found
    }

    /// Takes the current token if it is the contextual keyword `kind`.
    pub fn eat_contextual(&mut self, kind: SyntaxKind) -> bool {
        let found = self.at_contextual(kind);
        if found {
            self.bump_as(kind);
        }
        found
    }

    /// Takes a token of `kind`, or reports that it is missing and takes nothing.
    ///
    /// A reserved word found where an identifier is expected, on the same line as what
    /// precedes it, is reported as such and taken, since it was meant as the name.
    pub fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        let current = self.current();
        if self.lexer.is_identifier(kind) && self.lexer.is_keyword(current) && !self.on_new_line() {
            if !self.recovering {
                let message = format!(
                    "`{}` is a keyword and can't be used as a name",
                    self.current_text()
                );
                let range = self.current_range();
                self.report("LM-P002", range, message);
            }
            let wrong = self.start();
            self.take_unexpected();
            wrong.complete(self, SyntaxKind::ERROR);
        } else {
            self.error_expected();
        }
        false
    }

    /// Reports that something else was expected, listing every kind asked for at the
    /// current token.
    pub fn error_expected(&mut self) {
        if self.recovering {
            return;
        }
        let names: Vec<_> = self
            .expected
            .iter()
            .map(|&kind| self.lexer.describe(kind))
            .collect();
        let list = match names.split_last() {
            None => return self.expected("something else"),
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        };
        self.expected(&list);
    }

    /// Reports "expected `what`, found …" for the current token.
    pub fn expected(&mut self, what: &str) {
        // While an earlier error is being held against this one, build no message.
        if self.recovering {
            return;
        }
        let message = format!("expected {what}, found {}", self.found());
        // Something missing at the end of a line is reported there, not on the next.
        let range = match self.previous_end() {
            Some(end) if self.at_end() || self.on_new_line() => TextRange::empty(end),
            _ => self.current_range(),
        };
        self.report("LM-P001", range, message);
    }

    /// Reports a problem of the language's own at the current token.
    pub fn error(&mut self, code: &'static str, message: impl Into<String>) {
        if self.recovering {
            return;
        }
        let range = self.current_range();
        self.report(code, range, message.into());
    }

    /// Reports that `what` was expected, then puts tokens into an
    /// [`SyntaxKind::ERROR`] node until one of the `recovery` kinds or the end.
    ///
    /// If the current token is already one of them, nothing is taken, so a loop must
    /// handle those kinds itself.
    pub fn recover(&mut self, what: &str, recovery: &[SyntaxKind]) {
        if !self.recovering {
            let message = format!("expected {what}, found {}", self.found());
            let range = self.current_range();
            self.report("LM-P001", range, message);
        }
        if self.at_end() || recovery.contains(&self.current()) {
            return;
        }
        let skipped = self.start();
        while !self.at_end() && !recovery.contains(&self.current()) {
            self.take_unexpected();
        }
        skipped.complete(self, SyntaxKind::ERROR);
    }

    /// Whether a line ends between the previous token and the current one.
    pub(crate) fn on_new_line(&self) -> bool {
        self.previous_end().is_some_and(|end| {
            let between = TextRange::new(end, self.current_range().start().max(end));
            self.slice(between).contains(['\n', '\r'])
        })
    }

    /// Builds the tree. Tokens that were not taken are reported and kept in it.
    pub fn finish(mut self) -> Parse {
        if !self.halted && !self.recovering && self.token_at(0).is_some() {
            let message = format!("expected end of file, found {}", self.found());
            let range = self.current_range();
            self.report("LM-P001", range, message);
        }
        let green = self.build();
        let mut diagnostics = self.diagnostics;
        diagnostics.sort_by_key(|d| (d.range.start(), d.range.end(), d.code));
        Parse { green, diagnostics }
    }

    fn build(&mut self) -> GreenNode {
        let mut tree = TreeBuilder::new(self.text, &self.tokens).lexer(self.lexer);
        let mut events = std::mem::take(&mut self.events);
        let mut kinds = Vec::new();
        for index in 0..events.len() {
            // Taking the event leaves an abandoned start in its place, so a wrapper
            // that was opened early is skipped when its turn comes.
            let taken = std::mem::replace(
                &mut events[index],
                Event::Start {
                    kind: None,
                    wrapper: None,
                },
            );
            match taken {
                Event::Start { kind, mut wrapper } => {
                    kinds.push(kind);
                    while let Some(next) = wrapper.take() {
                        let Some(event) = events.get_mut(next) else {
                            break;
                        };
                        if let Event::Start {
                            kind,
                            wrapper: further,
                        } = event
                        {
                            kinds.push(kind.take());
                            wrapper = further.take();
                        }
                    }
                    for kind in kinds.drain(..).rev().flatten() {
                        tree.start_node(kind);
                    }
                }
                Event::Token { kind, count } => tree.token(kind, count),
                Event::Finish => tree.finish_node(),
            }
        }
        tree.finish()
    }

    fn complete(&mut self, event: usize, kind: SyntaxKind) -> CompletedMarker {
        let Some(height) = self.close(event) else {
            return CompletedMarker {
                event: None,
                height: 0,
            };
        };
        let mut node_height = height + 1;
        if node_height > self.limits.max_tree_depth {
            if !self.tree_too_deep {
                self.tree_too_deep = true;
                let range = self.current_range();
                let message = too_deep(self.limits.max_tree_depth);
                // Reported even while recovering: the tree is incomplete.
                self.push_diagnostic("LM-P003", range, message);
            }
            // The node is left out; its children join the one around it.
            node_height = height;
        } else {
            if let Some(Event::Start { kind: slot, .. }) = self.events.get_mut(event) {
                *slot = Some(kind);
            }
            self.events.push(Event::Finish);
        }
        if let Some(parent) = self.open.last_mut() {
            parent.height = parent.height.max(node_height);
        }
        CompletedMarker {
            event: Some(event),
            height: node_height,
        }
    }

    /// Removes an open node, and those opened inside it, returning its height.
    fn close(&mut self, event: usize) -> Option<u32> {
        let position = self.open.iter().rposition(|open| open.event == event)?;
        let height = self.open[position..]
            .iter()
            .map(|open| open.height)
            .max()
            .unwrap_or(0);
        self.open.truncate(position);
        Some(height)
    }

    /// Takes tokens as part of the expected shape.
    fn take(&mut self, kind: SyntaxKind, count: usize) {
        if self.halted || self.token_at(0).is_none() {
            return;
        }
        self.advance(kind, count);
        self.recovering = self.follows_unknown_text();
    }

    /// Takes a token that has no place here; errors stay held back.
    fn take_unexpected(&mut self) {
        if self.halted {
            return;
        }
        if let Some(kind) = self.token_at(0).map(|token| token.kind) {
            self.advance(kind, 1);
        }
    }

    fn advance(&mut self, kind: SyntaxKind, count: usize) {
        self.events.push(Event::Token { kind, count });
        self.at += count;
        self.expected.clear();
        self.fuel = FUEL;
    }

    fn token_at(&self, n: usize) -> Option<&Lexeme> {
        let index = *self.visible.get(self.at.checked_add(n)?)?;
        self.tokens.get(index)
    }

    /// Whether the lexer rejected text just before the current token. That was already
    /// reported, and what the parser misses here is most likely its result.
    fn follows_unknown_text(&self) -> bool {
        let end = self
            .visible
            .get(self.at)
            .copied()
            .unwrap_or(self.tokens.len());
        let start = match self.at.checked_sub(1) {
            Some(previous) => self.visible.get(previous).map_or(0, |&index| index + 1),
            None => 0,
        };
        self.tokens
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .any(|token| token.kind == SyntaxKind::ERROR)
    }

    fn previous_end(&self) -> Option<TextSize> {
        let index = *self.visible.get(self.at.checked_sub(1)?)?;
        self.tokens.get(index).map(|token| token.range.end())
    }

    fn slice(&self, range: TextRange) -> &'a str {
        self.text
            .get(std::ops::Range::<usize>::from(range))
            .unwrap_or("")
    }

    /// How a message names the current token.
    fn found(&mut self) -> String {
        let kind = self.current();
        if kind == SyntaxKind::EOF {
            return self.lexer.describe(kind);
        }
        let text = self.current_text();
        // Line endings and long tokens read better by name.
        if text.chars().count() > 30 || text.contains(['\n', '\r']) {
            self.lexer.describe(kind)
        } else {
            format!("`{text}`")
        }
    }

    fn report(&mut self, code: &'static str, range: TextRange, message: String) {
        if !self.recovering {
            self.recovering = true;
            self.push_diagnostic(code, range, message);
        }
    }

    fn push_diagnostic(&mut self, code: &'static str, range: TextRange, message: String) {
        self.diagnostics
            .push(Diagnostic::error(code, self.file, range, message));
    }

    /// Stops parsing because a limit was reached.
    fn halt(&mut self, code: &'static str, message: impl FnOnce() -> String) {
        if !self.halted {
            self.halted = true;
            self.recovering = true;
            let range = self
                .token_at(0)
                .map_or_else(|| self.current_range(), |token| token.range);
            self.push_diagnostic(code, range, message());
        }
    }
}

fn too_deep(limit: u32) -> String {
    format!("this is nested more than {limit} levels deep")
}
