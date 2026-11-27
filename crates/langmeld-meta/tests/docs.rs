//! Every complete `.langmeld` file shown in the documentation loads.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_meta::{Source, load_language};

/// The code blocks written as `langmeld` in a document, with the line each starts at.
fn blocks(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (number, line) in text.lines().enumerate() {
        match (&mut current, line.trim_end()) {
            (None, "```langmeld") => current = Some((number + 2, String::new())),
            (Some(_), "```") => found.extend(current.take()),
            (Some((_, code)), line) => {
                code.push_str(line);
                code.push('\n');
            }
            (None, _) => {}
        }
    }
    found
}

#[test]
fn the_examples_of_the_documents_load() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let (mut count, mut failures) = (0, String::new());
    for name in [
        "reference/langmeld-format.md",
        "guides/write-a-langmeld-file.md",
    ] {
        let text = std::fs::read_to_string(root.join(name)).expect("the document is readable");
        for (line, code) in blocks(&text) {
            let source = Source::text(name, code).generic();
            if let Err(error) = load_language(source) {
                failures.push_str(&format!("the example at line {line}:\n{error}\n"));
            }
            count += 1;
        }
    }
    assert!(failures.is_empty(), "{failures}");
    assert!(count >= 8, "only {count} examples were found");
}
