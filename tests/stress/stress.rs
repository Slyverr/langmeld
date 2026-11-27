//! The stress-test language passes the checks the format promises, deciding everything
//! with at most two tokens of lookahead, and its corpus holds for every hard feature,
//! both for text that is valid and for text that is broken.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use langmeld::{Source, load_language};
use langmeld_meta::corpus::{Outcome, parse_cases, run};

fn folder() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn loaded() -> langmeld::Loaded {
    load_language(Source::path(folder().join("pipes.langmeld")))
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn the_language_passes_the_check_with_automatic_lookahead_of_at_most_two() {
    let loaded = loaded();
    assert!(loaded.language.warnings().is_empty());
    let deepest = loaded.language.max_lookahead();
    assert_eq!(deepest, 2, "{:?}", loaded.language.lookahead());
    // The choices that need the second token are the ones the language is about.
    let starts: Vec<_> = loaded
        .language
        .lookahead()
        .into_iter()
        .filter(|place| place.rule == "Statement")
        .map(|place| place.starts)
        .collect();
    for word in ["retry", "ident", "pipeline", "task"] {
        assert!(
            starts.iter().any(|start| start == word),
            "{word}: {starts:?}"
        );
    }
    assert_eq!(loaded.extensions, ["pipe"]);
    assert_eq!(loaded.bindings.rules.len(), 8);
    assert_eq!(loaded.editor.rules.len(), 5);
}

fn cases() -> Vec<langmeld_meta::corpus::Case> {
    let mut paths: Vec<_> = std::fs::read_dir(folder().join("corpus"))
        .expect("the corpus folder exists")
        .map(|entry| entry.expect("readable").path())
        .collect();
    paths.sort();
    paths
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).expect("corpus files are readable");
            parse_cases(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        })
        .collect()
}

#[test]
fn every_case_of_the_corpus_passes() {
    let results = run(&loaded().language, cases());
    assert!(results.len() >= 30);
    for result in results {
        assert_eq!(
            result.outcome,
            Outcome::Passed,
            "case `{}`",
            result.case.name
        );
    }
}

/// The rows of the table the language is built to answer, by the start of case names.
const FEATURES: [&str; 12] = [
    "lines",
    "tags",
    "retry",
    "names",
    "units",
    "interpolation",
    "computed",
    "global",
    "extension",
    "expressions",
    "generics",
    "editor",
];

#[test]
fn every_feature_has_valid_and_broken_cases() {
    let cases = cases();
    for feature in FEATURES {
        for kind in ["valid", "broken"] {
            let prefix = format!("{feature}: {kind}");
            assert!(
                cases.iter().any(|case| case.name.starts_with(&prefix)),
                "no case starts with `{prefix}`"
            );
        }
    }
}

#[test]
fn valid_cases_have_no_problems_and_each_broken_one_has_one_per_mistake() {
    for result in run(&loaded().language, cases()) {
        let problems = result
            .actual
            .lines()
            .filter(|line| line.starts_with("! "))
            .count();
        if result.case.name.contains(": valid") {
            assert_eq!(problems, 0, "case `{}`", result.case.name);
        } else if !result.case.name.starts_with("recovery") {
            assert_eq!(problems, 1, "case `{}`", result.case.name);
        }
    }
}
