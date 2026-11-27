//! Helpers for the tests that run the `langmeld` binary in folders of their own.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;

/// A new empty folder named `name`, inside the folder Cargo gives tests.
pub fn folder(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("can create folders");
    path
}

/// What a command ended with.
pub struct Ran {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Ran {
    pub fn of(output: Output) -> Self {
        let text = |bytes| String::from_utf8(bytes).expect("UTF-8");
        Self {
            code: output.status.code().expect("exits"),
            out: text(output.stdout),
            err: text(output.stderr),
        }
    }

    /// The exit code and both outputs, as text for snapshots.
    pub fn shown(&self) -> String {
        format!(
            "exit: {}\n--- stdout\n{}--- stderr\n{}",
            self.code, self.out, self.err
        )
    }
}

/// Runs `langmeld` in `folder`.
pub fn langmeld(folder: &Path, arguments: &[&str]) -> Ran {
    let output = Command::cargo_bin("langmeld")
        .expect("the binary is built")
        .current_dir(folder)
        .args(arguments)
        .output()
        .expect("the binary runs");
    Ran::of(output)
}

/// The facade crate of this repository, so that new projects build against it.
pub fn facade() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../langmeld")
        .canonicalize()
        .expect("the facade is there")
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Every file under `root`, by path from it, with its text, in path order.
pub fn tree(root: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, at: &Path, into: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = fs::read_dir(at)
            .expect("a folder")
            .map(|entry| entry.expect("readable").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, into);
            } else {
                let name = path
                    .strip_prefix(root)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                into.push((name, fs::read_to_string(&path).unwrap_or_default()));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found
}
