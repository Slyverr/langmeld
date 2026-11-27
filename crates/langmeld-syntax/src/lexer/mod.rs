//! The lexer: source text to tokens.

mod compile;
mod run;
mod scan;
mod spec;

use std::fmt;
use std::sync::Arc;

use regex_automata::Anchored;
use regex_automata::dfa::{Automaton, dense};
use text_size::TextSize;

use crate::lexeme::Vocabulary;
use crate::{Diagnostic, Lexeme, SourceFile, SyntaxKind};

pub use compile::LexerBuilder;
pub use spec::{
    FloatPreset, IdentStyle, IntPreset, Interpolation, LexLimits, Preset, StringParts,
    StringPreset, Terminators, TokenSpec,
};

pub(crate) use spec::ExternalFn;
use spec::Matcher;

/// A token matched by the pattern automaton.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Pattern {
    spec: usize,
    /// Fixed text is preferred to patterns of the same length.
    rank: u8,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Comment {
    spec: usize,
    open: String,
    /// `None` for a comment that runs to the end of the line.
    close: Option<String>,
    /// Punctuation that starts with `open` and is longer.
    longer: Vec<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Scanner {
    spec: usize,
    kind: ScannerKind,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
enum ScannerKind {
    XidIdent,
    Char,
    String(StringPreset),
    External(#[serde(with = "crate::embed::token_code")] Arc<ExternalFn>),
}

/// What is known about one kind of token.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct KindInfo {
    kind: SyntaxKind,
    /// The first declaration of the kind.
    spec: usize,
    keyword: bool,
    identifier: bool,
    doc_comment: bool,
}

/// Answers about kinds, worked out once so that a parser never searches the
/// declarations. Sorted by kind, so a lookup is a binary search without hashing.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Kinds {
    infos: Vec<KindInfo>,
}

impl Kinds {
    fn of(specs: &[TokenSpec]) -> Self {
        let mut infos: Vec<KindInfo> = Vec::new();
        for (index, spec) in specs.iter().enumerate() {
            let keyword = matches!(spec.matcher, Matcher::Keyword(_));
            match infos.iter_mut().find(|info| info.kind == spec.kind) {
                Some(info) => {
                    info.keyword |= keyword;
                    info.identifier |= spec.identifier;
                    info.doc_comment |= spec.doc;
                }
                None => infos.push(KindInfo {
                    kind: spec.kind,
                    spec: index,
                    keyword,
                    identifier: spec.identifier,
                    doc_comment: spec.doc,
                }),
            }
        }
        infos.sort_by_key(|info| info.kind);
        Self { infos }
    }

    fn get(&self, kind: SyntaxKind) -> Option<&KindInfo> {
        let at = self
            .infos
            .binary_search_by_key(&kind, |info| info.kind)
            .ok()?;
        self.infos.get(at)
    }
}

/// What an external token sees of the source.
///
/// ```
/// use langmeld_syntax::{LexCursor, SyntaxKind, TokenSpec};
///
/// // A token for `#` followed by digits.
/// let hash_number = TokenSpec::external(SyntaxKind(1), "hash number", |cursor: &mut LexCursor<'_>| {
///     let digits = cursor.rest().strip_prefix('#')?;
///     let count = digits.bytes().take_while(u8::is_ascii_digit).count();
///     (count > 0).then_some(1 + count)
/// });
/// # let _ = hash_number;
/// ```
pub struct LexCursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> LexCursor<'a> {
    fn new(source: &'a str, offset: usize) -> Self {
        Self { source, offset }
    }

    /// The text from here to the end of the file.
    pub fn rest(&self) -> &'a str {
        &self.source[self.offset..]
    }

    /// The whole file.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Where `rest` starts in the file.
    pub fn offset(&self) -> TextSize {
        text_size(self.offset)
    }
}

/// A byte offset as a [`TextSize`]. Offsets come from a [`SourceFile`], whose text
/// is checked to fit in a `u32`, so the fallback is never used.
fn text_size(offset: usize) -> TextSize {
    TextSize::new(u32::try_from(offset).unwrap_or(u32::MAX))
}

/// The result of lexing one file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lexed {
    /// Every byte of the file, in order, without gaps.
    pub tokens: Vec<Lexeme>,
    /// What is wrong with the text.
    pub diagnostics: Vec<Diagnostic>,
}

/// Splits source text into tokens, as declared with a [`LexerBuilder`].
///
/// At every position the longest declared token wins, so `=-` is `=` and `-` unless
/// `=-` is declared. Lexing never fails: text that matches nothing becomes
/// [`SyntaxKind::ERROR`] tokens with diagnostics.
///
/// ```
/// use langmeld_syntax::{
///     FileId, IdentStyle, Lexer, Preset, SourceFile, SyntaxKind, TokenSpec,
/// };
///
/// const NAME: SyntaxKind = SyntaxKind(1);
/// const LET: SyntaxKind = SyntaxKind(2);
/// const SPACE: SyntaxKind = SyntaxKind(3);
///
/// let lexer = Lexer::builder()
///     .token(TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Xid)))
///     .token(TokenSpec::keyword(LET, "let"))
///     .token(TokenSpec::regex(SPACE, "space", r"\s+").skip())
///     .build()
///     .expect("the declarations are valid");
/// let file = SourceFile::new(FileId::new(0), "example", "let x")?;
/// let kinds: Vec<_> = lexer.lex(&file).tokens.iter().map(|t| t.kind).collect();
/// assert_eq!(kinds, [LET, SPACE, NAME]);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Lexer {
    specs: Vec<TokenSpec>,
    #[serde(with = "crate::embed::automaton")]
    dfa: Option<dense::DFA<Vec<u32>>>,
    patterns: Vec<Pattern>,
    /// Longest opener first.
    comments: Vec<Comment>,
    scanners: Vec<Scanner>,
    /// Reserved words with their kinds, sorted by word.
    keywords: Vec<(String, SyntaxKind)>,
    kinds: Kinds,
    terminators: Option<Terminators>,
    limits: LexLimits,
}

impl Lexer {
    /// Starts declaring tokens.
    /// Gives the code of external tokens to a lexer read back from bytes.
    pub(crate) fn bind(&mut self, externals: &crate::Externals) -> Result<(), crate::EmbedError> {
        for (index, spec) in self.specs.iter_mut().enumerate() {
            if !matches!(spec.matcher, Matcher::External(_)) {
                continue;
            }
            let code = externals
                .token_code(&spec.name)
                .ok_or_else(|| crate::EmbedError::MissingToken(spec.name.clone()))?;
            for scanner in self.scanners.iter_mut().filter(|s| s.spec == index) {
                scanner.kind = ScannerKind::External(code.clone());
            }
            spec.matcher = Matcher::External(code);
        }
        Ok(())
    }

    /// Starts declaring a lexer.
    pub fn builder() -> LexerBuilder {
        LexerBuilder::default()
    }

    /// Splits a file into tokens that cover all of it.
    #[must_use]
    pub fn lex(&self, file: &SourceFile) -> Lexed {
        run::Run::new(self, file).finish()
    }

    /// The longest match of the pattern automaton at the start of `text`, preferring
    /// the earliest declaration among equally long ones.
    fn longest_pattern(&self, text: &str) -> Option<(usize, Pattern)> {
        let dfa = self.dfa.as_ref()?;
        // Anchors are refused, so one start state serves every position.
        let mut state = dfa.universal_start_state(Anchored::Yes)?;
        // The last matching state and where its match ends. Which pattern won is only
        // worked out once, for the longest.
        let mut longest = None;
        let mut dead = false;
        for (at, &byte) in text.as_bytes().iter().enumerate() {
            state = dfa.next_state(state, byte);
            if dfa.is_special_state(state) {
                if dfa.is_match_state(state) {
                    // Matches are reported one byte late.
                    longest = Some((state, at));
                } else if dfa.is_dead_state(state) || dfa.is_quit_state(state) {
                    dead = true;
                    break;
                }
            }
        }
        if !dead {
            state = dfa.next_eoi_state(state);
            if dfa.is_match_state(state) {
                longest = Some((state, text.len()));
            }
        }
        let (state, len) = longest?;
        if len == 0 || !text.is_char_boundary(len) {
            return None;
        }
        let pattern = |i| self.patterns.get(dfa.match_pattern(state, i).as_usize());
        let chosen = match dfa.match_len(state) {
            // Almost always one pattern matches.
            1 => pattern(0),
            many => (0..many)
                .filter_map(pattern)
                .min_by_key(|pattern| (pattern.rank, pattern.spec)),
        };
        chosen.map(|&pattern| (len, pattern))
    }

    fn reads_as_identifier(&self, word: &str) -> bool {
        let identifiers = self
            .longest_pattern(word)
            .map(|(len, pattern)| (len, pattern.spec))
            .into_iter()
            .chain(self.scanners.iter().filter_map(|scanner| {
                let len = match &scanner.kind {
                    ScannerKind::XidIdent => scan::xid_identifier(word).map(|s| s.len),
                    ScannerKind::External(lex) => lex(&mut LexCursor::new(word, 0)),
                    ScannerKind::Char | ScannerKind::String(_) => None,
                };
                len.map(|len| (len, scanner.spec))
            }));
        let mut identifiers = identifiers;
        identifiers.any(|(len, spec)| len == word.len() && self.specs[spec].identifier)
    }

    /// Whether tokens of `kind` are trivia, which parsers do not see.
    pub(crate) fn is_trivia(&self, kind: SyntaxKind) -> bool {
        self.spec(kind).is_some_and(|spec| spec.trivia)
    }

    fn spec(&self, kind: SyntaxKind) -> Option<&TokenSpec> {
        self.specs.get(self.kinds.get(kind)?.spec)
    }

    /// The kind of the reserved word `word`, if it is one.
    fn keyword(&self, word: &str) -> Option<SyntaxKind> {
        let at = self
            .keywords
            .binary_search_by(|(known, _)| known.as_str().cmp(word))
            .ok()?;
        self.keywords.get(at).map(|&(_, kind)| kind)
    }

    fn interpolation_of(&self, kind: SyntaxKind) -> Option<(&Interpolation, &'static str)> {
        self.specs.iter().find_map(|spec| {
            let Matcher::Preset(Preset::String(options)) = &spec.matcher else {
                return None;
            };
            let interpolation = options.interpolation.as_ref()?;
            let kinds = interpolation.kinds;
            let part = [
                (kinds.start, "string"),
                (kinds.text, "string text"),
                (kinds.end, "closing quote"),
                (kinds.hole_open, "open"),
                (kinds.hole_close, "close"),
            ]
            .into_iter()
            .find(|&(part, _)| part == kind)?;
            Some((interpolation, part.1))
        })
    }
}

impl fmt::Debug for Lexer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.specs.iter().map(|spec| spec.name.as_str()).collect();
        f.debug_struct("Lexer").field("tokens", &names).finish()
    }
}

impl Vocabulary for Lexer {
    fn describe(&self, kind: SyntaxKind) -> String {
        if kind == SyntaxKind::EOF {
            return "end of file".to_owned();
        }
        if kind == SyntaxKind::ERROR {
            return "invalid text".to_owned();
        }
        if self.terminators.as_ref().is_some_and(|t| t.newline == kind) {
            return "end of line".to_owned();
        }
        if let Some((interpolation, part)) = self.interpolation_of(kind) {
            return match part {
                "open" => format!("`{}`", interpolation.open),
                "close" => format!("`{}`", interpolation.close),
                part => part.to_owned(),
            };
        }
        match self.spec(kind) {
            Some(spec) => match spec.matcher {
                Matcher::Punct(_) | Matcher::Keyword(_) | Matcher::Contextual(_) => {
                    format!("`{}`", spec.name)
                }
                _ => spec.name.clone(),
            },
            None => "token".to_owned(),
        }
    }

    fn is_keyword(&self, kind: SyntaxKind) -> bool {
        self.kinds.get(kind).is_some_and(|info| info.keyword)
    }

    fn is_identifier(&self, kind: SyntaxKind) -> bool {
        self.kinds.get(kind).is_some_and(|info| info.identifier)
    }

    fn contextual_text(&self, kind: SyntaxKind) -> Option<&str> {
        match &self.spec(kind)?.matcher {
            Matcher::Contextual(word) => Some(word),
            _ => None,
        }
    }

    fn is_doc_comment(&self, kind: SyntaxKind) -> bool {
        self.kinds.get(kind).is_some_and(|info| info.doc_comment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: SyntaxKind = SyntaxKind(1);
    const LET: SyntaxKind = SyntaxKind(2);
    const FROM: SyntaxKind = SyntaxKind(3);
    const SEMI: SyntaxKind = SyntaxKind(4);
    const DOC: SyntaxKind = SyntaxKind(5);
    const NOTE: SyntaxKind = SyntaxKind(6);
    const NEWLINE: SyntaxKind = SyntaxKind(7);
    const BREAK: SyntaxKind = SyntaxKind(8);

    fn lexer() -> Lexer {
        Lexer::builder()
            .token(TokenSpec::preset(
                NAME,
                "identifier",
                Preset::Ident(IdentStyle::Xid),
            ))
            .token(TokenSpec::keyword(LET, "let"))
            .token(TokenSpec::contextual(FROM, "from"))
            .token(TokenSpec::punct(SEMI, ";"))
            .token(TokenSpec::line_comment(DOC, "///").doc())
            .token(TokenSpec::line_comment(NOTE, "//"))
            .terminators(Terminators::new(NEWLINE, BREAK))
            .build()
            .expect("the declarations are valid")
    }

    #[test]
    fn kinds_answer_what_a_parser_asks() {
        let lexer = lexer();
        assert!(lexer.is_keyword(LET));
        assert!(!lexer.is_keyword(FROM));
        assert!(!lexer.is_keyword(SyntaxKind(999)));
        assert!(lexer.is_identifier(NAME));
        assert!(!lexer.is_identifier(LET));
        assert_eq!(lexer.contextual_text(FROM), Some("from"));
        assert_eq!(lexer.contextual_text(LET), None);
        assert!(lexer.is_doc_comment(DOC));
        assert!(!lexer.is_doc_comment(NOTE));
    }

    #[test]
    fn kinds_have_names_for_messages() {
        let lexer = lexer();
        assert_eq!(lexer.describe(SEMI), "`;`");
        assert_eq!(lexer.describe(LET), "`let`");
        assert_eq!(lexer.describe(FROM), "`from`");
        assert_eq!(lexer.describe(NAME), "identifier");
        assert_eq!(lexer.describe(NEWLINE), "end of line");
        assert_eq!(lexer.describe(SyntaxKind::EOF), "end of file");
        assert_eq!(lexer.describe(SyntaxKind::ERROR), "invalid text");
        assert_eq!(lexer.describe(SyntaxKind(999)), "token");
    }

    #[test]
    fn a_kind_declared_twice_has_what_either_declaration_has() {
        const K: SyntaxKind = SyntaxKind(9);
        let specs = [
            TokenSpec::line_comment(K, "//"),
            TokenSpec::keyword(K, "kw"),
            TokenSpec::preset(K, "name", Preset::Ident(IdentStyle::Ascii)),
            TokenSpec::line_comment(K, "///").doc(),
        ];
        let kinds = Kinds::of(&specs);
        let info = kinds.get(K).expect("declared");
        assert!(info.keyword && info.identifier && info.doc_comment);
        assert_eq!(info.spec, 0);
    }

    #[test]
    fn the_parts_of_an_interpolated_string_have_names() {
        let parts = StringParts {
            start: SyntaxKind(20),
            text: SyntaxKind(21),
            end: SyntaxKind(22),
            hole_open: SyntaxKind(23),
            hole_close: SyntaxKind(24),
        };
        let preset = StringPreset::default()
            .double(true)
            .interpolation(Interpolation::new("${", "}", parts));
        let lexer = Lexer::builder()
            .token(TokenSpec::preset(
                NAME,
                "identifier",
                Preset::Ident(IdentStyle::Ascii),
            ))
            .token(TokenSpec::preset(
                SyntaxKind(20),
                "string",
                Preset::String(preset),
            ))
            .build()
            .expect("the declarations are valid");
        let described: Vec<_> = (20..=24).map(|n| lexer.describe(SyntaxKind(n))).collect();
        assert_eq!(
            described,
            ["string", "string text", "closing quote", "`${`", "`}`"]
        );
        assert_eq!(lexer.describe(NAME), "identifier");
    }

    #[test]
    fn a_lexer_prints_its_tokens() {
        assert!(format!("{:?}", lexer()).starts_with("Lexer { tokens: [\"identifier\", \"let\""));
    }

    #[test]
    fn keywords_are_found_among_many() {
        let words = ["zeta", "alpha", "mid", "beta", "omega"];
        let mut builder = Lexer::builder().token(TokenSpec::preset(
            NAME,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ));
        for (n, word) in (10..).zip(words) {
            builder = builder.token(TokenSpec::keyword(SyntaxKind(n), word));
        }
        let lexer = builder.build().expect("the declarations are valid");
        for (n, word) in (10..).zip(words) {
            assert_eq!(lexer.keyword(word), Some(SyntaxKind(n)));
        }
        assert_eq!(lexer.keyword("alph"), None);
        assert_eq!(lexer.keyword(""), None);
    }
}
