//! The corpus file format.

use std::fmt;

const HEADER: &str = "=== ";
const SEPARATOR: &str = "---";
const EXPECTED_FAILURE: &str = "[expected-failure]";

/// One case of a corpus file.
#[derive(Debug, PartialEq, Eq)]
pub struct Case {
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
#[derive(Debug, PartialEq, Eq)]
pub struct FormatError {
    /// One-based line of the problem.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Splits a corpus file into its cases.
pub fn parse(text: &str) -> Result<Vec<Case>, FormatError> {
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
        let message = "case has no name".to_owned();
        return Err(FormatError { line, message });
    }
    Ok((name.to_owned(), expected_failure))
}
