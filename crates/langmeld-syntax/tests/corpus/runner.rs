//! Runs one corpus case.

use crate::format::Case;

/// What running a case showed.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    /// `actual` is `None` when nothing could process the source.
    Failed {
        actual: Option<String>,
    },
    ExpectedFailure,
    /// The case is marked as an expected failure, but it passes.
    UnexpectedPass,
}

impl Outcome {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed { .. } => "failed",
            Self::ExpectedFailure => "expected failure",
            Self::UnexpectedPass => "unexpected pass",
        }
    }

    /// What someone has to fix, if anything.
    pub fn problem(&self, case: &Case) -> Option<String> {
        let name = &case.name;
        match self {
            Self::Passed | Self::ExpectedFailure => None,
            Self::Failed { actual } => {
                let expected = &case.expected;
                let actual = actual
                    .as_deref()
                    .unwrap_or("(nothing can parse this source)");
                Some(format!(
                    "case `{name}` failed\n--- expected\n{expected}\n--- actual\n{actual}"
                ))
            }
            Self::UnexpectedPass => Some(format!(
                "case `{name}` passes now; remove its `[expected-failure]` marker"
            )),
        }
    }
}

/// Compares what `subject` makes of the source with the expected output.
pub fn run_case(case: &Case, subject: impl Fn(&str) -> Option<String>) -> Outcome {
    let actual = subject(&case.source).map(|output| {
        output
            .replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_owned()
    });
    let matches = actual.as_deref() == Some(case.expected.as_str());
    match (case.expected_failure, matches) {
        (false, true) => Outcome::Passed,
        (false, false) => Outcome::Failed { actual },
        (true, false) => Outcome::ExpectedFailure,
        (true, true) => Outcome::UnexpectedPass,
    }
}
