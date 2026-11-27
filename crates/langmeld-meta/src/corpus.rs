//! Corpus files: cases of a source text and the tree and diagnostics a language gives
//! for it.
//!
//! ```text
//! === a case
//! source text
//! ---
//! the tree, then the diagnostics
//! ```
//!
//! A tree is written one node or token per line, without trivia; a node holding a
//! single token takes one line. Each diagnostic follows as `! CODE start..end message`.

use std::fmt::{self, Write};

use langmeld_syntax::{FileId, Language, Parse, SourceFile, SyntaxKind, SyntaxNode, parse};

const HEADER: &str = "=== ";
const SEPARATOR: &str = "---";
const EXPECTED_FAILURE: &str = "[expected-failure]";

/// One case of a corpus file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The name, unique within its file.
    pub name: String,
    /// One-based line of the header.
    pub line: usize,
    /// The input, with its line endings untouched.
    pub source: String,
    /// The expected output, with `\n` line endings.
    pub expected: String,
    /// Whether the case is known not to pass yet.
    pub expected_failure: bool,
}

/// Why a corpus file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError {
    /// One-based line of the problem.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for FormatError {}

fn without_line_ending(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// Splits a header into the case name and its marker.
fn read_title(title: &str, line: usize) -> Result<(String, bool), FormatError> {
    let title = title.trim();
    let marker = title
        .ends_with(']')
        .then(|| title.rfind('['))
        .flatten()
        .map(|start| title.split_at(start));
    let (name, expected_failure) = match marker {
        Some((name, EXPECTED_FAILURE)) => (name.trim_end(), true),
        Some((_, unknown)) => {
            let message =
                format!("unknown marker `{unknown}`; the only marker is `{EXPECTED_FAILURE}`");
            return Err(FormatError { line, message });
        }
        None => (title, false),
    };
    if name.is_empty() {
        let message = "the case has no name".to_owned();
        return Err(FormatError { line, message });
    }
    Ok((name.to_owned(), expected_failure))
}

/// Splits the text of a corpus file into its cases.
pub fn parse_cases(text: &str) -> Result<Vec<Case>, FormatError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut lines = text
        .split_inclusive('\n')
        .zip(1..)
        .map(|(raw, number)| (number, raw, without_line_ending(raw)))
        .peekable();
    let mut cases: Vec<Case> = Vec::new();
    while let Some((line, _, content)) = lines.next() {
        if content.is_empty() {
            continue;
        }
        let Some(title) = content.strip_prefix(HEADER) else {
            let message = format!("expected a case header starting with `{HEADER}`");
            return Err(FormatError { line, message });
        };
        let (name, expected_failure) = read_title(title, line)?;
        if cases.iter().any(|case| case.name == name) {
            let message = format!("there is already a case named `{name}`");
            return Err(FormatError { line, message });
        }
        let mut source = String::new();
        loop {
            let Some((_, raw, content)) = lines.next() else {
                let message = format!("case `{name}` has no `{SEPARATOR}` line after its source");
                return Err(FormatError { line, message });
            };
            if content == SEPARATOR {
                break;
            }
            source.push_str(raw);
        }
        // The line ending before the separator belongs to the format.
        source.truncate(without_line_ending(&source).len());
        let mut expected = Vec::new();
        while let Some((_, _, content)) =
            lines.next_if(|(_, _, content)| !content.starts_with(HEADER))
        {
            expected.push(content);
        }
        let expected = expected.join("\n").trim_end_matches('\n').to_owned();
        cases.push(Case {
            name,
            line,
            source,
            expected,
            expected_failure,
        });
    }
    Ok(cases)
}

fn node(language: &Language, current: &SyntaxNode, depth: usize, out: &mut String) {
    let children: Vec<_> = current
        .children_with_tokens()
        .filter(|child| !language.is_trivia(child.kind()))
        .collect();
    let indent = "  ".repeat(depth);
    let called = if current.kind() == SyntaxKind::ERROR {
        "Error"
    } else {
        language.kind_name(current.kind())
    };
    let only_tokens = children.iter().all(|child| child.as_token().is_some());
    let one_line = depth > 0
        && only_tokens
        && !children.is_empty()
        && (children.len() == 1 || called == "Error");
    write!(out, "{indent}{called}").expect("writes to a string");
    if one_line {
        for token in children.iter().filter_map(|child| child.as_token()) {
            write!(out, " {:?}", token.text()).expect("writes to a string");
        }
        out.push('\n');
        return;
    }
    out.push('\n');
    for child in children {
        if let Some(token) = child.as_token() {
            writeln!(out, "{indent}  {:?}", token.text()).expect("writes to a string");
        } else if let Some(child) = child.as_node() {
            node(language, child, depth + 1, out);
        }
    }
}

/// A parse as corpus files write it: the tree, then the diagnostics.
pub fn render_parse(language: &Language, parsed: &Parse) -> String {
    let mut out = String::new();
    node(language, &parsed.syntax(), 0, &mut out);
    for d in &parsed.diagnostics {
        let (start, end) = (u32::from(d.range.start()), u32::from(d.range.end()));
        writeln!(out, "! {} {start}..{end} {}", d.code, d.message).expect("writes to a string");
    }
    out.pop();
    out
}

/// What the language gives for a case's source, as corpus files write it.
pub fn actual(language: &Language, source: &str) -> String {
    match SourceFile::new(FileId::new(0), "case", source) {
        Ok(file) => render_parse(language, &parse(language, &file)),
        Err(error) => format!("! the source cannot be read: {error}"),
    }
}

/// How a case came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// The output is what the case expects.
    Passed,
    /// The output differs.
    Failed,
    /// The output differs, as the case says it does.
    ExpectedFailure,
    /// The case is marked `[expected-failure]`, and passes.
    UnexpectedPass,
}

/// A case and what the language gave for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseResult {
    /// The case.
    pub case: Case,
    /// What the language gave.
    pub actual: String,
    /// How it came out.
    pub outcome: Outcome,
}

/// Runs the cases against a language.
pub fn run(language: &Language, cases: Vec<Case>) -> Vec<CaseResult> {
    cases
        .into_iter()
        .map(|case| {
            let actual = actual(language, &case.source);
            let same = actual == case.expected;
            let outcome = match (same, case.expected_failure) {
                (true, false) => Outcome::Passed,
                (false, false) => Outcome::Failed,
                (false, true) => Outcome::ExpectedFailure,
                (true, true) => Outcome::UnexpectedPass,
            };
            CaseResult {
                case,
                actual,
                outcome,
            }
        })
        .collect()
}

/// The text of a corpus file whose cases expect what the language gave, except cases
/// that are marked as expected failures.
pub fn rewrite(results: &[CaseResult]) -> String {
    let mut out = String::new();
    for result in results {
        let case = &result.case;
        let marker = if case.expected_failure {
            format!(" {EXPECTED_FAILURE}")
        } else {
            String::new()
        };
        let expected = if case.expected_failure {
            &case.expected
        } else {
            &result.actual
        };
        write!(
            out,
            "{HEADER}{}{marker}\n{}\n{SEPARATOR}\n{expected}\n\n",
            case.name, case.source
        )
        .expect("writes to a string");
    }
    out.truncate(out.trim_end_matches('\n').len());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_into_its_cases_and_written_back() {
        let text = "=== one\na\nb\n---\nFile\n\n=== two [expected-failure]\n\n---\nsketch\n";
        let cases = parse_cases(text).expect("well formed");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].source, "a\nb");
        assert_eq!(cases[0].expected, "File");
        assert!(cases[1].expected_failure);
        let results = run_nothing(&cases);
        assert_eq!(rewrite(&results), text);
    }

    fn run_nothing(cases: &[Case]) -> Vec<CaseResult> {
        cases
            .iter()
            .map(|case| CaseResult {
                case: case.clone(),
                actual: case.expected.clone(),
                outcome: Outcome::Passed,
            })
            .collect()
    }

    #[test]
    fn mistakes_in_a_file_name_the_line() {
        let error = parse_cases("text\n").expect_err("no header");
        assert_eq!(
            error.to_string(),
            "line 1: expected a case header starting with `=== `"
        );
        let error = parse_cases("=== a\nx\n").expect_err("no separator");
        assert_eq!(
            error.to_string(),
            "line 1: case `a` has no `---` line after its source"
        );
    }
}
