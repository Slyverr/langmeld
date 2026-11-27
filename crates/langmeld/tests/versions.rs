//! The version of the crates is written once, and the other places that need it agree.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

fn manifest() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    std::fs::read_to_string(path).expect("the workspace manifest is readable")
}

/// The lines of a table of the manifest.
fn table<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("[{name}]");
    text.lines()
        .skip_while(|line| line.trim() != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .collect()
}

#[test]
fn the_crates_that_depend_on_each_other_name_the_workspace_version() {
    let text = manifest();
    let version = table(&text, "workspace.package")
        .into_iter()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|version| version.trim_matches('"').to_owned())
        .expect("the workspace has a version");
    let crates: Vec<_> = table(&text, "workspace.dependencies")
        .into_iter()
        .filter(|line| line.starts_with("langmeld"))
        .collect();
    assert!(crates.len() >= 3, "{crates:?}");
    for line in crates {
        assert!(
            line.contains(&format!("version = \"{version}\"")),
            "{line} does not name version {version}"
        );
    }
}
