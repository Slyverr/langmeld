//! Every diagnostic code has a long explanation.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use langmeld_syntax::{codes, explain};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Codes such as `LM-G001` written in `text`.
fn written(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("LM-").filter_map(|(at, _)| {
        let code = text.get(at..at + 7)?;
        let mut rest = code.bytes().skip(3);
        let letter = rest.next()?.is_ascii_uppercase();
        (letter && rest.all(|b| b.is_ascii_digit())).then_some(code)
    })
}

fn sources(folder: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).expect("a folder") {
        let path = entry.expect("readable").path();
        if path.is_dir() {
            sources(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

#[test]
fn every_code_the_crates_report_is_explained_and_listed() {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(repository().join("crates")).expect("crates") {
        sources(&entry.expect("readable").path().join("src"), &mut files);
    }
    let mut reported = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(file).expect("readable");
        reported.extend(
            text.lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .flat_map(written)
                .map(str::to_owned),
        );
    }
    let explained: BTreeSet<_> = codes().map(str::to_owned).collect();
    assert_eq!(explained, reported);
    let reference = repository().join("docs/reference/diagnostics.md");
    let listed: BTreeSet<_> = written(&std::fs::read_to_string(reference).expect("readable"))
        .map(str::to_owned)
        .collect();
    assert_eq!(explained, listed);
}

#[test]
fn explanations_are_text() {
    for code in codes() {
        let text = explain(code).expect("listed");
        assert!(
            text.lines().next().is_some_and(|l| l.len() > 10),
            "{code}: {text}"
        );
        assert!(!text.contains("## "), "{code}");
        assert_eq!(text, text.trim(), "{code}");
    }
    assert!(
        explain("LM-G001")
            .expect("known")
            .starts_with("A name is neither")
    );
    assert_eq!(explain("LM-X999"), None);
    assert_eq!(explain(""), None);
}
