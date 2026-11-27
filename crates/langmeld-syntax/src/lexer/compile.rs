//! Turns token declarations into a [`Lexer`], or explains why it cannot.

use std::collections::{HashMap, HashSet};
use std::error::Error;

use regex_automata::dfa::{StartKind, dense};
use regex_automata::nfa::thompson;
use regex_automata::{MatchKind, util::syntax};

use text_size::{TextRange, TextSize};

use crate::{Diagnostic, FileId};

use crate::SyntaxKind;

use super::spec::{FloatPreset, IdentStyle, IntPreset, LexLimits, Matcher, Preset, Terminators};
use super::{Comment, Kinds, Lexer, Pattern, Scanner, ScannerKind, TokenSpec};

/// Memory limits that keep a careless pattern from taking unbounded time to compile.
const NFA_LIMIT: usize = 4 << 20;
const DFA_LIMIT: usize = 16 << 20;

/// A problem with the declarations. It has no place in any source file, so it points
/// at the start of file 0. The note says how to fix it.
fn definition_error(code: &'static str, message: String, fix: &str) -> Diagnostic {
    Diagnostic::error(
        code,
        FileId::new(0),
        TextRange::empty(TextSize::new(0)),
        message,
    )
    .note(format!("To fix this, {fix}."))
}

fn matches_empty(token: &str) -> Diagnostic {
    definition_error(
        "LM-G007",
        format!("token `{token}` can match empty text"),
        "require at least one character, for example with `+` instead of `*`",
    )
}

fn never_matches(token: &str) -> Diagnostic {
    definition_error(
        "LM-G009",
        format!(
            "token `{token}` can never match, because another token is declared with the same text"
        ),
        "remove one of the two",
    )
}

/// Collects token declarations for a [`Lexer`].
#[derive(Clone, Default)]
pub struct LexerBuilder {
    specs: Vec<TokenSpec>,
    terminators: Option<Terminators>,
    limits: LexLimits,
}

impl LexerBuilder {
    /// Declares a token. When two tokens match the same longest text, fixed text wins,
    /// and otherwise the one declared first.
    pub fn token(mut self, spec: TokenSpec) -> Self {
        self.specs.push(spec);
        self
    }

    /// Declares several tokens.
    pub fn tokens(mut self, specs: impl IntoIterator<Item = TokenSpec>) -> Self {
        self.specs.extend(specs);
        self
    }

    /// Lets newlines end statements.
    pub fn terminators(mut self, terminators: Terminators) -> Self {
        self.terminators = Some(terminators);
        self
    }

    /// Replaces the default limits.
    pub fn limits(mut self, limits: LexLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Checks the declarations and compiles them. Reports every problem found, each
    /// with a stable code and a note that says how to fix it. These problems are in
    /// the declarations, not in a source file, so their file is `FileId::new(0)` and
    /// their range is empty.
    pub fn build(self) -> Result<Lexer, Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut texts = HashSet::new();
        let mut patterns = Vec::new();
        let mut regexes = Vec::new();
        let mut comments = Vec::new();
        let mut scanners = Vec::new();
        let mut keywords: Vec<(String, SyntaxKind)> = Vec::new();
        // The token that reads each quote of strings.
        let mut quoted: HashMap<char, &str> = HashMap::new();
        let mut words = Vec::new();

        for (index, spec) in self.specs.iter().enumerate() {
            let name = spec.name.as_str();
            match &spec.matcher {
                Matcher::Punct(text)
                | Matcher::Keyword(text)
                | Matcher::Contextual(text)
                | Matcher::LineComment(text)
                | Matcher::BlockComment { open: text, .. } => {
                    let close_is_empty = matches!(
                        &spec.matcher,
                        Matcher::BlockComment { close, .. } if close.is_empty()
                    );
                    if text.is_empty() || close_is_empty {
                        errors.push(matches_empty(name));
                        continue;
                    }
                    if !texts.insert(text.as_str()) {
                        errors.push(never_matches(name));
                        continue;
                    }
                }
                Matcher::Regex(_) | Matcher::Preset(_) | Matcher::External(_) => {}
            }

            match &spec.matcher {
                Matcher::Punct(text) => {
                    patterns.push(Pattern {
                        spec: index,
                        rank: 0,
                    });
                    regexes.push(literal(text));
                }
                Matcher::Keyword(word) => {
                    keywords.push((word.clone(), spec.kind));
                    words.push(word.clone());
                }
                Matcher::Contextual(word) => words.push(word.clone()),
                Matcher::Regex(pattern) => match syntax::parse(pattern) {
                    Err(error) => errors.push(definition_error(
                        "LM-L101",
                        format!(
                            "token `{name}` has an invalid pattern: {}",
                            last_line(&error.to_string())
                        ),
                        "check the regular expression",
                    )),
                    Ok(parsed) if parsed.properties().minimum_len() == Some(0) => {
                        errors.push(matches_empty(name));
                    }
                    Ok(_) => {
                        patterns.push(Pattern {
                            spec: index,
                            rank: 1,
                        });
                        regexes.push(pattern.clone());
                    }
                },
                Matcher::Preset(preset) => {
                    let regex = match preset {
                        Preset::Int(options) => Some(int_pattern(*options)),
                        Preset::Float(options) => Some(float_pattern(*options)),
                        Preset::Ident(IdentStyle::Ascii) => {
                            Some("[A-Za-z_][A-Za-z0-9_]*".to_owned())
                        }
                        Preset::Ident(IdentStyle::Xid) => {
                            scanners.push(Scanner {
                                spec: index,
                                kind: ScannerKind::XidIdent,
                            });
                            None
                        }
                        Preset::Char => {
                            scanners.push(Scanner {
                                spec: index,
                                kind: ScannerKind::Char,
                            });
                            None
                        }
                        Preset::String(options) => {
                            if !options.double && !options.single {
                                errors.push(definition_error(
                                    "LM-L104",
                                    format!("string token `{name}` enables no kind of quote"),
                                    "enable `double` or `single`",
                                ));
                            }
                            let quotes = [('"', options.double), ('\'', options.single)];
                            for (quote, _) in quotes.into_iter().filter(|(_, on)| *on) {
                                if let Some(first) = quoted.insert(quote, name) {
                                    errors.push(definition_error(
                                        "LM-G009",
                                        format!(
                                            "token `{name}` can never match, because `{first}` \
                                             reads the same quotes"
                                        ),
                                        "let one token enable that quote",
                                    ));
                                }
                            }
                            let empty_delimiter = options
                                .interpolation
                                .as_ref()
                                .is_some_and(|i| i.open.is_empty() || i.close.is_empty());
                            if empty_delimiter {
                                errors.push(matches_empty(name));
                            }
                            scanners.push(Scanner {
                                spec: index,
                                kind: ScannerKind::String(options.clone()),
                            });
                            None
                        }
                    };
                    if let Some(regex) = regex {
                        patterns.push(Pattern {
                            spec: index,
                            rank: 1,
                        });
                        regexes.push(regex);
                    }
                }
                Matcher::External(lex) => scanners.push(Scanner {
                    spec: index,
                    kind: ScannerKind::External(lex.clone()),
                }),
                Matcher::LineComment(open) => comments.push(Comment {
                    spec: index,
                    open: open.clone(),
                    close: None,
                    longer: Vec::new(),
                }),
                Matcher::BlockComment { open, close } => comments.push(Comment {
                    spec: index,
                    open: open.clone(),
                    close: Some(close.clone()),
                    longer: Vec::new(),
                }),
            }
        }

        // Punctuation that starts like a comment but is longer wins over it.
        for comment in &mut comments {
            comment.longer = self
                .specs
                .iter()
                .filter_map(|spec| match &spec.matcher {
                    Matcher::Punct(text)
                        if text.len() > comment.open.len() && text.starts_with(&comment.open) =>
                    {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .collect();
        }
        // So that `///` is tried before `//`.
        comments.sort_by_key(|comment| std::cmp::Reverse(comment.open.len()));

        let dfa = match compile(&regexes) {
            Ok(dfa) => dfa,
            Err(error) => {
                errors.push(*error);
                None
            }
        };

        keywords.sort();
        let lexer = Lexer {
            kinds: Kinds::of(&self.specs),
            specs: self.specs,
            dfa,
            patterns,
            comments,
            scanners,
            keywords,
            terminators: self.terminators,
            limits: self.limits,
        };
        check_words(&lexer, &words, &mut errors);

        if errors.is_empty() {
            Ok(lexer)
        } else {
            Err(errors)
        }
    }
}

/// Every keyword must be something the lexer first reads as an identifier.
fn check_words(lexer: &Lexer, words: &[String], errors: &mut Vec<Diagnostic>) {
    let has_identifier = lexer.specs.iter().any(|spec| spec.identifier);
    for word in words {
        if !has_identifier {
            errors.push(definition_error(
                "LM-L102",
                format!("keyword `{word}` needs an identifier token"),
                "declare one, for example with the identifier preset",
            ));
        } else if !lexer.reads_as_identifier(word) {
            errors.push(definition_error(
                "LM-G008",
                format!("keyword `{word}` is not matched as an identifier"),
                "declare it as punctuation, or widen the identifier token so that it \
                 matches the whole word",
            ));
        }
    }
}

fn compile(regexes: &[String]) -> Result<Option<dense::DFA<Vec<u32>>>, Box<Diagnostic>> {
    if regexes.is_empty() {
        return Ok(None);
    }
    let nfa = thompson::Compiler::new()
        .configure(
            thompson::Config::new()
                .nfa_size_limit(Some(NFA_LIMIT))
                .which_captures(thompson::WhichCaptures::None),
        )
        .build_many(regexes)
        .map_err(|error| {
            if error.size_limit().is_some() {
                too_complex()
            } else {
                cannot_compile(&error)
            }
        })?;
    let config = dense::Config::new()
        .match_kind(MatchKind::All)
        .start_kind(StartKind::Anchored)
        .dfa_size_limit(Some(DFA_LIMIT))
        .determinize_size_limit(Some(DFA_LIMIT));
    dense::Builder::new()
        .configure(config)
        .build_from_nfa(&nfa)
        .map(Some)
        .map_err(|error| {
            if error.is_size_limit_exceeded() {
                too_complex()
            } else {
                cannot_compile(&error)
            }
        })
}

fn too_complex() -> Box<Diagnostic> {
    Box::new(definition_error(
        "LM-L103",
        "the token patterns are too complex to compile".to_owned(),
        "simplify large repetitions such as `{1000}`",
    ))
}

fn cannot_compile(error: &dyn Error) -> Box<Diagnostic> {
    Box::new(definition_error(
        "LM-L101",
        format!("a token pattern cannot be compiled: {error}"),
        "remove anchors and word boundaries from the patterns",
    ))
}

/// A pattern matching exactly `text`.
fn literal(text: &str) -> String {
    text.chars()
        .map(|c| format!("\\x{{{:x}}}", u32::from(c)))
        .collect()
}

fn digits(class: &str, underscores: bool) -> String {
    if underscores {
        format!("{class}(?:_*{class})*")
    } else {
        format!("{class}+")
    }
}

fn int_pattern(options: IntPreset) -> String {
    let bases = [
        (options.hex, "0[xX]", "[0-9a-fA-F]"),
        (options.binary, "0[bB]", "[01]"),
        (options.octal, "0[oO]", "[0-7]"),
    ];
    let mut alternatives: Vec<String> = bases
        .into_iter()
        .filter(|(enabled, ..)| *enabled)
        // The digits are optional here so that a bare prefix is reported as such.
        .map(|(_, prefix, class)| format!("{prefix}(?:{})?", digits(class, options.underscores)))
        .collect();
    alternatives.push(digits("[0-9]", options.underscores));
    format!("(?:{})", alternatives.join("|"))
}

fn float_pattern(options: FloatPreset) -> String {
    let number = digits("[0-9]", options.underscores);
    if options.exponent {
        let exponent = format!("[eE][+-]?{number}");
        format!("(?:{number}\\.{number}(?:{exponent})?|{number}{exponent})")
    } else {
        format!("{number}\\.{number}")
    }
}

/// The part of a multi-line pattern error that says what is wrong.
fn last_line(message: &str) -> &str {
    let line = message.lines().last().unwrap_or(message);
    line.strip_prefix("error: ").unwrap_or(line)
}
