//! Every diagnostic code that the source mentions is explained in the reference and
//! shown by at least one test.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Every file under `root` with one of the extensions, in no particular order.
fn files(root: &Path, extensions: &[&str], found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, extensions, found);
        } else if path
            .extension()
            .is_some_and(|ext| extensions.iter().any(|wanted| ext == *wanted))
        {
            found.push(path);
        }
    }
}

/// The codes written in `text`: `LM-`, a capital letter and three digits.
fn codes_in(text: &str) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    for (at, _) in text.match_indices("LM-") {
        let code: String = text[at..].chars().take(7).collect();
        let mut chars = code.chars().skip(3);
        let shaped = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.clone().count() == 3
            && chars.all(|c| c.is_ascii_digit());
        if shaped {
            codes.insert(code);
        }
    }
    codes
}

fn codes_under(root: &Path, extensions: &[&str], skip: &[&str]) -> BTreeSet<String> {
    let mut paths = Vec::new();
    files(root, extensions, &mut paths);
    let mut codes = BTreeSet::new();
    for path in paths {
        if skip.iter().any(|name| path.ends_with(name)) {
            continue;
        }
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        codes.extend(codes_in(&text));
    }
    codes
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn in_source() -> BTreeSet<String> {
    codes_under(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &["rs"],
        &[],
    )
}

#[test]
fn the_source_has_codes() {
    let codes = in_source();
    assert!(codes.len() >= 30, "only {codes:?} were found");
    // The scan reads codes the way they are written.
    assert_eq!(
        codes_in("`LM-P001`, LM-G12, LM-g123, LM-L1234 and LM-S001."),
        BTreeSet::from([
            "LM-P001".to_owned(),
            "LM-L123".to_owned(),
            "LM-S001".to_owned()
        ])
    );
}

#[test]
fn every_code_is_explained_in_the_reference() {
    let reference = repository().join("docs/reference/diagnostics.md");
    let text = fs::read_to_string(&reference).expect("the reference is readable");
    let explained = codes_in(&text);
    let missing: Vec<_> = in_source().difference(&explained).cloned().collect();
    assert!(
        missing.is_empty(),
        "docs/reference/diagnostics.md does not explain {missing:?}"
    );
    let unknown: Vec<_> = explained.difference(&in_source()).cloned().collect();
    assert!(
        unknown.is_empty(),
        "docs/reference/diagnostics.md explains codes that nothing reports: {unknown:?}"
    );
}

#[test]
fn every_code_is_shown_by_a_test() {
    let root = repository();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut shown = BTreeSet::new();
    for folder in [manifest.join("tests"), root.join("tests")] {
        shown.extend(codes_under(
            &folder,
            &["rs", "txt", "snap"],
            &["diagnostic_codes.rs"],
        ));
    }
    let mut examples = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join("examples")) {
        examples.extend(entries.flatten().map(|entry| entry.path()));
    }
    for example in examples {
        for folder in ["tests", "corpus"] {
            shown.extend(codes_under(
                &example.join(folder),
                &["rs", "txt", "snap"],
                &[],
            ));
        }
    }
    let untested: Vec<_> = in_source().difference(&shown).cloned().collect();
    assert!(untested.is_empty(), "no test shows {untested:?}");
}
