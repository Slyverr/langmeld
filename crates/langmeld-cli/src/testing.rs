//! Running the corpus files of a language.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use langmeld_meta::corpus::{CaseResult, Outcome, parse_cases, rewrite, run as run_cases};
use langmeld_syntax::Language;

use crate::{Failure, error_line, line};

/// A path as it is shown: with `/`, and without a leading `./`.
fn shown(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_prefix("./").map_or(text.clone(), str::to_owned)
}

/// Every `.txt` file under `folder`, in order.
fn corpus_files(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            corpus_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "txt") {
            found.push(path);
        }
    }
}

/// How the cases of one file came out, as text, and how many failed.
fn report(name: &str, results: &[CaseResult]) -> (String, usize) {
    let count = |wanted: Outcome| results.iter().filter(|r| r.outcome == wanted).count();
    let (passed, failed) = (count(Outcome::Passed), count(Outcome::Failed));
    let (expected, surprising) = (
        count(Outcome::ExpectedFailure),
        count(Outcome::UnexpectedPass),
    );
    let mut out = format!("{name}: {passed} passed");
    if failed + surprising > 0 {
        write!(out, ", {} failed", failed + surprising).expect("writes to a string");
    }
    if expected > 0 {
        write!(out, ", {expected} expected to fail").expect("writes to a string");
    }
    out.push('\n');
    for result in results {
        let case = &result.case;
        match result.outcome {
            Outcome::Failed => {
                writeln!(out, "  FAIL {} (line {})", case.name, case.line).expect("writes");
                for (label, text) in [("expected", &case.expected), ("actual", &result.actual)] {
                    writeln!(out, "    {label}:").expect("writes to a string");
                    for line in text.lines() {
                        writeln!(out, "      {line}").expect("writes to a string");
                    }
                }
            }
            Outcome::UnexpectedPass => {
                writeln!(
                    out,
                    "  FAIL {} (line {}) passes now: remove `[expected-failure]` from its name",
                    case.name, case.line
                )
                .expect("writes to a string");
            }
            _ => {}
        }
    }
    (out, failed + surprising)
}

/// Runs the corpus files under `folder` and shows how each came out, with file names
/// relative to `base`. With `update`, the expectations of failing cases are rewritten to
/// what the language gives.
fn run_corpus_in(
    language: &Language,
    folder: &Path,
    base: &Path,
    update: bool,
) -> Result<(), Failure> {
    let mut files = Vec::new();
    corpus_files(folder, &mut files);
    if files.is_empty() {
        return Err(Failure::Unable(format!(
            "there are no corpus files in `{}`; add `.txt` files of cases there",
            shown(folder)
        )));
    }
    let (mut failures, mut cases) = (0, 0);
    for file in files {
        let name = file.strip_prefix(base).map_or_else(|_| shown(&file), shown);
        let text = std::fs::read_to_string(&file)
            .map_err(|error| Failure::Unable(format!("cannot read `{name}`: {error}")))?;
        let parsed = match parse_cases(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                error_line(&format!("error: `{name}`, {error}"));
                return Err(Failure::Found);
            }
        };
        let results = run_cases(language, parsed);
        cases += results.len();
        if update {
            let changed = results
                .iter()
                .filter(|r| r.outcome == Outcome::Failed)
                .count();
            if changed > 0 {
                std::fs::write(&file, rewrite(&results))
                    .map_err(|error| Failure::Unable(format!("cannot write `{name}`: {error}")))?;
                line(&format!("{name}: updated {changed} cases"));
            } else {
                line(&format!("{name}: nothing to update"));
            }
            continue;
        }
        let (text, failed) = report(&name, &results);
        print!("{text}");
        failures += failed;
    }
    if update {
        return Ok(());
    }
    if failures > 0 {
        line(&format!(
            "test result: FAILED. {failures} of {cases} cases failed"
        ));
        Err(Failure::Found)
    } else {
        line(&format!("test result: ok. {cases} cases"));
        Ok(())
    }
}

/// Runs the corpus files under `folder` and shows how each came out. With `update`, the
/// expectations of failing cases are rewritten to what the language gives.
///
/// Fails when a case fails (exit code 1), and when there are no corpus files (exit
/// code 2).
pub fn run_corpus(language: &Language, folder: &Path, update: bool) -> Result<(), Failure> {
    let base = folder.parent().unwrap_or_else(|| Path::new(""));
    run_corpus_in(language, folder, base, update)
}
