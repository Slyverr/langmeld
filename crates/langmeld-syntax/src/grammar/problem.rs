//! Problems in a grammar, told as diagnostics.
//!
//! They are about the declarations, not about a source file, so their file is
//! `FileId::new(0)` and their range is empty, as for the lexer's.

use std::fmt::Display;

use crate::{Diagnostic, FileId, TextRange, TextSize};

fn place() -> (FileId, TextRange) {
    (FileId::new(0), TextRange::empty(TextSize::new(0)))
}

/// A problem that refuses the grammar.
pub(super) fn error(code: &'static str, message: impl Into<String>) -> Diagnostic {
    let (file, range) = place();
    Diagnostic::error(code, file, range, message)
}

/// A problem that leaves the grammar usable.
pub(super) fn warning(code: &'static str, message: impl Into<String>) -> Diagnostic {
    let (file, range) = place();
    Diagnostic::warning(code, file, range, message)
}

/// Adds the note that says how to solve the problem.
pub(super) fn to_fix(diagnostic: Diagnostic, fix: impl Display) -> Diagnostic {
    diagnostic.note(format!("To fix this, {fix}."))
}

/// Adds a note for those who know the theory, which the message does without.
pub(super) fn technically(diagnostic: Diagnostic, explanation: impl Display) -> Diagnostic {
    diagnostic.note(format!("In technical terms: {explanation}."))
}

/// How many single-letter changes turn `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &y) in b.iter().enumerate() {
            let changed = diagonal + usize::from(x != y);
            diagonal = row[j + 1];
            row[j + 1] = changed.min(row[j] + 1).min(diagonal + 1);
        }
    }
    row[b.len()]
}

/// The name among `known` that `name` is most likely a misspelling of.
pub(super) fn closest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    let allowed = name.chars().count() / 3 + 1;
    known
        .iter()
        .map(|&candidate| (distance(name, candidate), candidate))
        .filter(|&(away, _)| away <= allowed)
        .min()
        .map(|(_, candidate)| candidate)
}

/// A name that nothing declares. `message` says where it is used.
pub(super) fn undefined(message: String, name: &str, known: &[&str]) -> Diagnostic {
    let fix = match closest(name, known) {
        Some(candidate) => format!("did you mean `{candidate}`? If not, declare `{name}`"),
        None => format!("declare `{name}` as a rule or a token"),
    };
    to_fix(error("LM-G001", message), fix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closest_name_is_a_near_miss() {
        let known = ["NameRef", "File", "Expr"];
        assert_eq!(closest("NameRfe", &known), Some("NameRef"));
        assert_eq!(closest("Fil", &known), Some("File"));
        assert_eq!(closest("zzzzzz", &known), None);
    }
}
