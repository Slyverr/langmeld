//! One pass over one file.

use text_size::TextRange;

use crate::{Diagnostic, FileId, Lexeme, SourceFile, SyntaxKind};

use super::scan::{self, Problem, Scanned};
use super::spec::{Interpolation, Preset};
use super::{LexCursor, Lexed, Lexer, Matcher, ScannerKind, text_size};

/// Where in an interpolated string the lexer is.
enum Mode<'l> {
    /// Between the quotes, outside holes.
    Text {
        quote: char,
        start: usize,
        interpolation: &'l Interpolation,
    },
    /// Inside a hole. `depth` counts open pairs of the closing delimiter's kind.
    Hole {
        interpolation: &'l Interpolation,
        depth: usize,
    },
}

/// The match chosen at one position.
struct Found {
    len: usize,
    spec: usize,
    rank: u8,
    problem: Option<Problem>,
    /// Set for the opening quote of an interpolated string.
    opens_string: Option<char>,
}

pub(super) struct Run<'l, 't> {
    lexer: &'l Lexer,
    file: FileId,
    text: &'t str,
    at: usize,
    tokens: Vec<Lexeme>,
    diagnostics: Vec<Diagnostic>,
    modes: Vec<Mode<'l>>,
    /// Start of the current run of unknown characters.
    unknown_from: Option<usize>,
    /// Whether a newline here would end a statement.
    can_terminate: bool,
    /// Open brackets: the closing kind, and whether newlines are ignored inside.
    brackets: Vec<(SyntaxKind, bool)>,
}

/// `start..end` as a range. Callers pass offsets into a source file's text, which is
/// checked to fit in a `u32`.
fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(text_size(start), text_size(end))
}

impl<'l, 't> Run<'l, 't> {
    pub(super) fn new(lexer: &'l Lexer, file: &'t SourceFile) -> Self {
        Self {
            lexer,
            file: file.id(),
            text: file.text(),
            at: 0,
            // Most text averages a few bytes per token.
            tokens: Vec::with_capacity(
                (file.text().len() / 4).min(lexer.limits.max_tokens as usize),
            ),
            diagnostics: Vec::new(),
            modes: Vec::new(),
            unknown_from: None,
            can_terminate: false,
            brackets: Vec::new(),
        }
    }

    pub(super) fn finish(mut self) -> Lexed {
        let limit = self.lexer.limits.max_len;
        if self.text.len() > limit as usize {
            let message = format!(
                "this file is {} bytes long, and the limit is {limit}",
                self.text.len()
            );
            self.give_up("LM-L007", message);
        } else {
            self.read_all();
        }
        let mut next_is_code = false;
        for token in self.tokens.iter_mut().rev() {
            token.joint = next_is_code && !token.trivia;
            next_is_code = !token.trivia;
        }
        Lexed {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn read_all(&mut self) {
        if self.text.starts_with('\u{FEFF}') {
            self.push(SyntaxKind::BOM, '\u{FEFF}'.len_utf8(), true);
        }
        while self.at < self.text.len() {
            if self.tokens.len() >= self.lexer.limits.max_tokens as usize {
                self.end_unknown();
                let limit = self.lexer.limits.max_tokens;
                let message = format!("this file has too many tokens; the limit is {limit}");
                self.modes.clear();
                self.give_up("LM-L008", message);
                return;
            }
            match self.modes.last() {
                Some(&Mode::Text {
                    quote,
                    interpolation,
                    ..
                }) => self.string_text(quote, interpolation),
                Some(&Mode::Hole {
                    interpolation,
                    depth,
                }) => self.in_hole(interpolation, depth),
                None => {
                    if !self.line_end() {
                        self.token();
                    }
                }
            }
        }
        self.end_unknown();
        self.abandon_strings();
    }

    /// Covers the rest of the file with one error token.
    fn give_up(&mut self, code: &'static str, message: String) {
        let rest = range(self.at, self.text.len());
        if !rest.is_empty() {
            self.push(SyntaxKind::ERROR, self.text.len() - self.at, false);
        }
        self.error(code, rest, message);
    }

    fn error(&mut self, code: &'static str, range: TextRange, message: String) {
        self.diagnostics
            .push(Diagnostic::error(code, self.file, range, message));
    }

    fn push(&mut self, kind: SyntaxKind, len: usize, trivia: bool) {
        let end = self.at + len;
        self.tokens.push(Lexeme {
            kind,
            range: range(self.at, end),
            trivia,
            joint: false,
        });
        self.at = end;
        if !trivia {
            self.significant(kind);
        }
    }

    /// Tracks what a newline after this token would mean.
    fn significant(&mut self, kind: SyntaxKind) {
        let Some(terminators) = &self.lexer.terminators else {
            return;
        };
        if kind == terminators.newline {
            self.can_terminate = false;
            return;
        }
        self.can_terminate = !terminators.continue_after.contains(&kind);
        let opened = |pairs: &[(SyntaxKind, SyntaxKind)]| {
            pairs
                .iter()
                .find(|(open, _)| *open == kind)
                .map(|&(_, close)| close)
        };
        if let Some(close) = opened(&terminators.brackets) {
            self.brackets.push((close, true));
        } else if let Some(close) = opened(&terminators.blocks) {
            self.brackets.push((close, false));
        } else if let Some(open) = self.brackets.iter().rposition(|&(close, _)| close == kind) {
            self.brackets.truncate(open);
        }
    }

    /// With terminators, reads a line ending as a token of its own.
    fn line_end(&mut self) -> bool {
        let Some(terminators) = &self.lexer.terminators else {
            return false;
        };
        let rest = &self.text[self.at..];
        let len = if rest.starts_with("\r\n") {
            2
        } else if rest.starts_with(scan::is_line_end) {
            1
        } else {
            return false;
        };
        self.end_unknown();
        let ignored = self.brackets.last().is_some_and(|&(_, ignored)| ignored);
        if self.can_terminate && !ignored {
            self.push(terminators.newline, len, false);
        } else {
            self.push(terminators.line_break, len, true);
        }
        true
    }

    /// Reads one ordinary token, or one unknown character.
    fn token(&mut self) {
        let Some(found) = self.longest_match() else {
            let len = self.text[self.at..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            self.unknown_from.get_or_insert(self.at);
            self.at += len;
            return;
        };
        self.end_unknown();
        let lexer = self.lexer;
        let spec = &lexer.specs[found.spec];
        let start = self.at;
        let text = &self.text[start..start + found.len];

        if let Some(quote) = found.opens_string {
            if let Matcher::Preset(Preset::String(options)) = &spec.matcher
                && let Some(interpolation) = &options.interpolation
            {
                self.push(interpolation.kinds.start, found.len, false);
                self.modes.push(Mode::Text {
                    quote,
                    start,
                    interpolation,
                });
            }
            return;
        }

        if matches!(spec.matcher, Matcher::Preset(Preset::Int(_)))
            && matches!(text, "0x" | "0X" | "0b" | "0B" | "0o" | "0O")
        {
            let message = format!("`{text}` needs at least one digit after it");
            self.error("LM-L006", range(start, start + found.len), message);
        }
        if let Some(problem) = found.problem {
            self.report(problem, start, found.len);
        }
        // Words are looked up as slices of the source, so no string is built.
        let kind = if spec.identifier {
            lexer.keyword(text).unwrap_or(spec.kind)
        } else {
            spec.kind
        };
        self.push(kind, found.len, spec.trivia);
    }

    fn report(&mut self, problem: Problem, start: usize, len: usize) {
        let whole = range(start, start + len);
        match problem {
            Problem::UnterminatedString(quote) => self.unterminated_string(whole, quote),
            Problem::UnterminatedComment { opener_len } => self.error(
                "LM-L003",
                range(start, start + opener_len),
                self.closer_message(start),
            ),
            Problem::UnterminatedChar => self.error(
                "LM-L004",
                whole,
                "unterminated character literal; add a closing `'`".to_owned(),
            ),
            Problem::NotOneChar => self.error(
                "LM-L005",
                whole,
                "a character literal holds exactly one character".to_owned(),
            ),
        }
    }

    fn closer_message(&self, start: usize) -> String {
        let close = self
            .lexer
            .comments
            .iter()
            .find(|comment| self.text[start..].starts_with(&comment.open))
            .and_then(|comment| comment.close.as_deref())
            .unwrap_or_default();
        format!("unterminated block comment; add `{close}`")
    }

    fn unterminated_string(&mut self, range: TextRange, quote: char) {
        let message =
            format!("unterminated string; add a closing `{quote}` before the end of the line");
        self.error("LM-L002", range, message);
    }

    /// Reports the outermost open string once, and returns to ordinary code.
    fn abandon_strings(&mut self) {
        if let Some(&Mode::Text { quote, start, .. }) = self.modes.first() {
            self.unterminated_string(range(start, self.at), quote);
        }
        self.modes.clear();
    }

    fn end_unknown(&mut self) {
        let Some(from) = self.unknown_from.take() else {
            return;
        };
        let text = &self.text[from..self.at];
        let shown: String = text.chars().map(printable).collect();
        let noun = if text.chars().nth(1).is_some() {
            "characters"
        } else {
            "character"
        };
        let found = range(from, self.at);
        self.error("LM-L001", found, format!("unexpected {noun} `{shown}`"));
        let end = self.at;
        self.at = from;
        self.push(SyntaxKind::ERROR, end - from, false);
    }

    /// The longest token starting here. Between equal lengths, fixed text wins, then
    /// the declaration order.
    fn longest_match(&self) -> Option<Found> {
        let rest = &self.text[self.at..];
        if let Some(comment) = self.comment(rest) {
            return Some(comment);
        }
        let mut best: Option<Found> = None;
        let mut consider = |found: Found| {
            let better = best.as_ref().is_none_or(|best| {
                found.len > best.len
                    || (found.len == best.len && (found.rank, found.spec) < (best.rank, best.spec))
            });
            if better {
                best = Some(found);
            }
        };
        if let Some((len, pattern)) = self.lexer.longest_pattern(rest) {
            consider(Found {
                len: self.stop_trivia_at_line_end(rest, len, pattern.spec),
                spec: pattern.spec,
                rank: pattern.rank,
                problem: None,
                opens_string: None,
            });
        }
        for scanner in &self.lexer.scanners {
            let mut opens_string = None;
            let scanned = match &scanner.kind {
                ScannerKind::XidIdent => scan::xid_identifier(rest),
                ScannerKind::Char => scan::character(rest),
                ScannerKind::String(options) => {
                    scan::string_start(options, rest).map(|(quote, raw)| {
                        if options.interpolation.is_some() && !raw {
                            opens_string = Some(quote);
                            Scanned {
                                len: 1,
                                problem: None,
                            }
                        } else {
                            scan::string(rest, quote, raw)
                        }
                    })
                }
                ScannerKind::External(lex) => {
                    let mut cursor = LexCursor::new(self.text, self.at);
                    lex(&mut cursor)
                        .filter(|&len| len > 0 && rest.is_char_boundary(len))
                        .map(|len| Scanned { len, problem: None })
                }
            };
            if let Some(scanned) = scanned {
                consider(Found {
                    len: scanned.len,
                    spec: scanner.spec,
                    rank: 1,
                    problem: scanned.problem,
                    opens_string,
                });
            }
        }
        best
    }

    fn comment(&self, rest: &str) -> Option<Found> {
        let comment = self
            .lexer
            .comments
            .iter()
            .find(|comment| rest.starts_with(&comment.open))?;
        if comment.longer.iter().any(|text| rest.starts_with(text)) {
            return None;
        }
        let scanned = match &comment.close {
            None => Scanned {
                len: scan::line_len(rest),
                problem: None,
            },
            Some(close) => {
                let nested = self.lexer.specs[comment.spec].nested;
                scan::block_comment(rest, &comment.open, close, nested)
            }
        };
        Some(Found {
            len: scanned.len,
            spec: comment.spec,
            rank: 0,
            problem: scanned.problem,
            opens_string: None,
        })
    }

    /// Where line endings are tokens of their own, whitespace stops before them.
    fn stop_trivia_at_line_end(&self, rest: &str, len: usize, spec: usize) -> usize {
        let lines_matter = self.lexer.terminators.is_some() || !self.modes.is_empty();
        if !lines_matter || !self.lexer.specs[spec].trivia {
            return len;
        }
        match rest.get(..len).map(scan::line_len) {
            Some(before) if before > 0 => before,
            _ => len,
        }
    }

    fn string_text(&mut self, quote: char, interpolation: &'l Interpolation) {
        let rest = &self.text[self.at..];
        let mut chars = rest.char_indices().peekable();
        let mut end = rest.len();
        while let Some(&(offset, c)) = chars.peek() {
            if c == quote || scan::is_line_end(c) || rest[offset..].starts_with(&interpolation.open)
            {
                end = offset;
                break;
            }
            chars.next();
            if c == '\\' {
                chars.next_if(|&(_, next)| !scan::is_line_end(next));
            }
        }
        if end > 0 {
            self.push(interpolation.kinds.text, end, false);
        }
        let rest = &self.text[self.at..];
        if rest.starts_with(quote) {
            self.push(interpolation.kinds.end, quote.len_utf8(), false);
            self.modes.pop();
        } else if rest.starts_with(&interpolation.open) {
            self.push(
                interpolation.kinds.hole_open,
                interpolation.open.len(),
                false,
            );
            self.modes.push(Mode::Hole {
                interpolation,
                depth: 0,
            });
        } else {
            self.abandon_strings();
        }
    }

    fn in_hole(&mut self, interpolation: &'l Interpolation, depth: usize) {
        let rest = &self.text[self.at..];
        if depth == 0 && rest.starts_with(&interpolation.close) {
            self.end_unknown();
            self.push(
                interpolation.kinds.hole_close,
                interpolation.close.len(),
                false,
            );
            self.modes.pop();
            return;
        }
        if rest.starts_with(scan::is_line_end) {
            self.end_unknown();
            self.abandon_strings();
            return;
        }
        let before = self.tokens.len();
        let start = self.at;
        let open_modes = self.modes.len();
        self.token();
        // A string that starts here adds a mode, and its quote does not nest.
        if self.tokens.len() == before || self.modes.len() != open_modes {
            return;
        }
        let text = &self.text[start..self.at];
        let nests = match interpolation.close.as_str() {
            "}" => Some('{'),
            ")" => Some('('),
            "]" => Some('['),
            _ => None,
        };
        let opened = nests.is_some_and(|open| text.ends_with(open));
        let closed = nests.is_some() && text == interpolation.close;
        if let Some(Mode::Hole { depth, .. }) = self.modes.last_mut() {
            if opened {
                *depth += 1;
            } else if closed {
                *depth = depth.saturating_sub(1);
            }
        }
    }
}

/// A character as a message shows it: itself, or an escape if it is invisible.
fn printable(c: char) -> String {
    match c {
        '\'' | '"' | '\\' => c.to_string(),
        c => c.escape_debug().to_string(),
    }
}
