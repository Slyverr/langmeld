//! Tokens that are read by code instead of by the pattern automaton.

use super::spec::StringPreset;

/// A recognised piece of text.
pub(crate) struct Scanned {
    pub len: usize,
    pub problem: Option<Problem>,
}

/// What is wrong with a token that was still recognised.
#[derive(Clone, Copy)]
pub(crate) enum Problem {
    UnterminatedString(char),
    UnterminatedComment { opener_len: usize },
    UnterminatedChar,
    NotOneChar,
}

fn complete(len: usize) -> Scanned {
    Scanned { len, problem: None }
}

pub(crate) fn is_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r')
}

pub(crate) fn line_len(text: &str) -> usize {
    text.find(is_line_end).unwrap_or(text.len())
}

pub(crate) fn xid_identifier(text: &str) -> Option<Scanned> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if first != '_' && !unicode_ident::is_xid_start(first) {
        return None;
    }
    let len = chars
        .find(|&(_, c)| !unicode_ident::is_xid_continue(c))
        .map_or(text.len(), |(offset, _)| offset);
    Some(complete(len))
}

/// The quote that starts a string here, and the length of what precedes the body.
pub(crate) fn string_start(options: &StringPreset, text: &str) -> Option<(char, bool)> {
    let enabled = |quote: char| match quote {
        '"' => options.double,
        '\'' => options.single,
        _ => false,
    };
    let mut chars = text.chars();
    match chars.next()? {
        'r' if options.raw => chars.next().filter(|&q| enabled(q)).map(|q| (q, true)),
        quote => enabled(quote).then_some((quote, false)),
    }
}

/// A string without holes. `start` is the answer of [`string_start`].
pub(crate) fn string(text: &str, quote: char, raw: bool) -> Scanned {
    // The quote, and the `r` of a raw string.
    let opening = if raw { 2 } else { 1 };
    let mut chars = text.char_indices().skip(opening).peekable();
    while let Some((offset, c)) = chars.next() {
        if c == quote {
            return complete(offset + 1);
        }
        if is_line_end(c) {
            return unterminated_string(offset, quote);
        }
        if c == '\\' && !raw {
            // An escape cannot carry a string over a line end.
            chars.next_if(|&(_, next)| !is_line_end(next));
        }
    }
    unterminated_string(text.len(), quote)
}

fn unterminated_string(len: usize, quote: char) -> Scanned {
    Scanned {
        len,
        problem: Some(Problem::UnterminatedString(quote)),
    }
}

pub(crate) fn character(text: &str) -> Option<Scanned> {
    let mut chars = text.strip_prefix('\'')?.char_indices().peekable();
    let mut count = 0;
    while let Some((offset, c)) = chars.next() {
        match c {
            '\'' => {
                return Some(Scanned {
                    // Both quotes.
                    len: offset + 2,
                    problem: (count != 1).then_some(Problem::NotOneChar),
                });
            }
            c if is_line_end(c) => {
                return Some(Scanned {
                    len: offset + 1,
                    problem: Some(Problem::UnterminatedChar),
                });
            }
            '\\' => {
                let escaped = chars.next_if(|&(_, next)| !is_line_end(next));
                // `\u{...}` is one character.
                if escaped.is_some_and(|(_, e)| e == 'u')
                    && chars.next_if(|&(_, next)| next == '{').is_some()
                {
                    while chars
                        .next_if(|&(_, next)| next.is_ascii_hexdigit() || next == '_')
                        .is_some()
                    {}
                    chars.next_if(|&(_, next)| next == '}');
                }
            }
            _ => {}
        }
        count += 1;
    }
    Some(Scanned {
        len: text.len(),
        problem: Some(Problem::UnterminatedChar),
    })
}

/// A block comment, given that `text` starts with `open`.
pub(crate) fn block_comment(text: &str, open: &str, close: &str, nested: bool) -> Scanned {
    let bytes = text.as_bytes();
    let mut depth = 1_usize;
    let mut at = open.len();
    while let Some(rest) = bytes.get(at..).filter(|rest| !rest.is_empty()) {
        if rest.starts_with(close.as_bytes()) {
            at += close.len();
            depth -= 1;
            if depth == 0 {
                return complete(at);
            }
        } else if nested && rest.starts_with(open.as_bytes()) {
            at += open.len();
            depth += 1;
        } else {
            at += 1;
        }
    }
    Scanned {
        len: text.len(),
        problem: Some(Problem::UnterminatedComment {
            opener_len: open.len(),
        }),
    }
}
