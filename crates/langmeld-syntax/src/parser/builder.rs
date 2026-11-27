//! Builds a lossless tree from tokens and node boundaries.

use rowan::{GreenNode, GreenNodeBuilder, Language};
use text_size::TextRange;

use crate::lexeme::Vocabulary;
use crate::{Lexeme, Lexer, SyntaxKind, SyntaxLanguage};

/// Whether parsers do not see the token.
pub(super) fn is_hidden(token: &Lexeme) -> bool {
    token.trivia || token.kind == SyntaxKind::ERROR
}

/// Builds a syntax tree that holds every byte of the source.
///
/// Say where nodes start and finish, and how the significant tokens are used; the
/// builder places whitespace, comments and unknown text. They go before the node that
/// follows them, except documentation comments, which go inside it.
///
/// No sequence of calls can fail, and no byte is lost: a token whose range does not
/// fit the text (which a lexer of this crate never gives) is added as an
/// [`SyntaxKind::ERROR`] token holding all the text not yet in the tree, and a debug
/// build fails an assertion.
///
/// ```
/// use langmeld_syntax::{
///     FileId, IdentStyle, Lexer, Preset, SourceFile, SyntaxKind, SyntaxNode, TokenSpec,
///     TreeBuilder,
/// };
///
/// const NAME: SyntaxKind = SyntaxKind(1);
/// const SPACE: SyntaxKind = SyntaxKind(2);
/// const ROOT: SyntaxKind = SyntaxKind(3);
///
/// let lexer = Lexer::builder()
///     .token(TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Ascii)))
///     .token(TokenSpec::regex(SPACE, "space", r"[ ]+").skip())
///     .build()
///     .expect("the declarations are valid");
/// let file = SourceFile::new(FileId::new(0), "example", "a b")?;
/// let lexed = lexer.lex(&file);
///
/// let mut builder = TreeBuilder::new(file.text(), &lexed.tokens);
/// builder.start_node(ROOT);
/// builder.token(NAME, 1);
/// builder.token(NAME, 1);
/// builder.finish_node();
/// let root = SyntaxNode::new_root(builder.finish());
/// assert_eq!(root.text(), "a b");
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
///
/// The first node started is the root: it stays open
/// until [`finish`](Self::finish), so whatever comes later ends up inside it.
pub struct TreeBuilder<'a> {
    text: &'a str,
    tokens: &'a [Lexeme],
    /// The first token not yet in the tree.
    next: usize,
    /// Open nodes, the root included.
    depth: usize,
    /// How many bytes of the text are in the tree.
    written: usize,
    lexer: Option<&'a Lexer>,
    inner: GreenNodeBuilder<'static>,
}

impl<'a> TreeBuilder<'a> {
    /// A builder for the tokens of `text`.
    pub fn new(text: &'a str, tokens: &'a [Lexeme]) -> Self {
        Self {
            text,
            tokens,
            next: 0,
            depth: 0,
            written: 0,
            lexer: None,
            inner: GreenNodeBuilder::new(),
        }
    }

    /// Uses `lexer` to recognise documentation comments.
    #[must_use]
    pub fn lexer(mut self, lexer: &'a Lexer) -> Self {
        self.lexer = Some(lexer);
        self
    }

    /// Starts a node. Tokens added until the matching
    /// [`finish_node`](Self::finish_node) are its children.
    pub fn start_node(&mut self, kind: SyntaxKind) {
        if self.depth > 0 {
            self.add_hidden(|builder, token| !builder.is_doc(token));
        }
        self.inner.start_node(SyntaxLanguage::kind_to_raw(kind));
        self.depth += 1;
        if self.depth == 1 {
            // The root holds everything before the first node.
            self.add_hidden(|builder, token| !builder.is_doc(token));
        } else if self.peek().is_some_and(|token| self.is_doc(token)) {
            // A documentation comment, and what follows it, belongs to this node.
            self.add_hidden(|_, _| true);
        }
    }

    /// Adds the next `count` significant tokens as one token of `kind`.
    pub fn token(&mut self, kind: SyntaxKind, count: usize) {
        if self.depth == 0 {
            self.start_node(SyntaxKind::ERROR);
        }
        self.add_hidden(|_, _| true);
        let mut covered: Option<TextRange> = None;
        let mut taken = 0;
        while taken < count {
            let Some(token) = self.peek() else { break };
            covered = Some(covered.map_or(token.range, |range| range.cover(token.range)));
            taken += usize::from(!is_hidden(token));
            self.next += 1;
        }
        if let Some(range) = covered {
            self.add(kind, range);
        }
    }

    /// Finishes the node started last. Does nothing for the root.
    pub fn finish_node(&mut self) {
        if self.depth > 1 {
            self.inner.finish_node();
            self.depth -= 1;
        }
    }

    /// Finishes every node and returns the root. Tokens that were not added are put in
    /// an [`SyntaxKind::ERROR`] node at the end of it.
    pub fn finish(mut self) -> GreenNode {
        if self.depth == 0 {
            self.start_node(SyntaxKind::ERROR);
        }
        while self.depth > 1 {
            self.finish_node();
        }
        self.add_hidden(|_, _| true);
        let rest = self.tokens.get(self.next..).unwrap_or_default();
        if let Some(last) = rest.iter().rposition(|token| !is_hidden(token)) {
            let end = self.next + last + 1;
            self.inner
                .start_node(SyntaxLanguage::kind_to_raw(SyntaxKind::ERROR));
            while self.next < end {
                self.add_next();
            }
            self.inner.finish_node();
        }
        while self.next < self.tokens.len() {
            self.add_next();
        }
        if self.written < self.text.len() {
            // The tokens did not cover the text.
            self.add_unplaced();
        }
        self.inner.finish_node();
        self.inner.finish()
    }

    fn peek(&self) -> Option<&'a Lexeme> {
        self.tokens.get(self.next)
    }

    fn is_doc(&self, token: &Lexeme) -> bool {
        self.lexer
            .is_some_and(|lexer| lexer.is_doc_comment(token.kind))
    }

    /// Adds the hidden tokens that come next, as long as `wanted` accepts them.
    fn add_hidden(&mut self, wanted: impl Fn(&Self, &Lexeme) -> bool) {
        while self
            .peek()
            .is_some_and(|token| is_hidden(token) && wanted(self, token))
        {
            self.add_next();
        }
    }

    fn add_next(&mut self) {
        if let Some(token) = self.peek() {
            self.next += 1;
            self.add(token.kind, token.range);
        }
    }

    fn add(&mut self, kind: SyntaxKind, range: TextRange) {
        let fits = (usize::from(range.start()) == self.written)
            .then(|| self.text.get(std::ops::Range::<usize>::from(range)))
            .flatten();
        debug_assert!(
            fits.is_some(),
            "token range {range:?} does not follow the text already in the tree"
        );
        match fits {
            Some(text) => {
                self.inner.token(SyntaxLanguage::kind_to_raw(kind), text);
                self.written += text.len();
            }
            None => self.add_unplaced(),
        }
    }

    /// Adds all the text not yet in the tree as one error token, so that nothing is lost.
    fn add_unplaced(&mut self) {
        let rest = self.text.get(self.written..).unwrap_or_default();
        if !rest.is_empty() {
            let error = SyntaxLanguage::kind_to_raw(SyntaxKind::ERROR);
            self.inner.token(error, rest);
            self.written = self.text.len();
        }
    }
}
