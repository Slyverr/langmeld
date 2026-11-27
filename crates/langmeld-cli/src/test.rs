//! `langmeld test`.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use langmeld_meta::corpus::{CaseResult, Outcome, parse_cases, rewrite, run as run_cases};
use langmeld_meta::{Source, load_language};

use crate::project::{self, shown};

/// The name of the folder of corpus files, inside the project.
const CORPUS: &str = "corpus";

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

/// How the cases of one file came out, as text.
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

pub(crate) fn run(path: &Path, update: bool) -> ExitCode {
    let found = match project::find(path) {
        Ok(found) => found,
        Err(refused) => {
            eprintln!("{}", refused.0.trim_end());
            return ExitCode::from(2);
        }
    };
    let loaded = match load_language(Source::path(&found.root).generic()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprint!("{}", error.rendered());
            eprintln!(
                "error: {} has problems, so its corpus cannot be run",
                shown(&found.root)
            );
            return ExitCode::FAILURE;
        }
    };
    let mut files = Vec::new();
    corpus_files(&found.dir.join(CORPUS), &mut files);
    if files.is_empty() {
        eprintln!(
            "error: there are no corpus files in `{}`; add `.txt` files of cases there",
            shown(&found.dir.join(CORPUS))
        );
        return ExitCode::from(2);
    }
    let (mut failures, mut cases) = (0, 0);
    for file in files {
        let name = file
            .strip_prefix(&found.dir)
            .map_or_else(|_| shown(&file), shown);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("error: cannot read `{name}`: {error}");
                return ExitCode::from(2);
            }
        };
        let parsed = match parse_cases(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("error: `{name}`, {error}");
                return ExitCode::FAILURE;
            }
        };
        let results = run_cases(&loaded.language, parsed);
        cases += results.len();
        if update {
            let changed = results
                .iter()
                .filter(|r| r.outcome == Outcome::Failed)
                .count();
            if changed > 0 {
                if let Err(error) = std::fs::write(&file, rewrite(&results)) {
                    eprintln!("error: cannot write `{name}`: {error}");
                    return ExitCode::from(2);
                }
                println!("{name}: updated {changed} cases");
            } else {
                println!("{name}: nothing to update");
            }
            continue;
        }
        let (text, failed) = report(&name, &results);
        print!("{text}");
        failures += failed;
    }
    if update {
        return ExitCode::SUCCESS;
    }
    if failures > 0 {
        println!("test result: FAILED. {failures} of {cases} cases failed");
        ExitCode::FAILURE
    } else {
        println!("test result: ok. {cases} cases");
        ExitCode::SUCCESS
    }
}
