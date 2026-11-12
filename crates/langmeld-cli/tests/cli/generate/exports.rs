use crate::{TestDirectory, write_widget_project};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

#[test]
fn generate_textmate_uses_imported_definitions_and_writes_json_to_stdout() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "textmate",
            "--manifest-path",
            project.join("langmeld.toml").to_str().unwrap(),
            "--scope-name",
            "source.widget",
            "--file-type",
            "widget",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(grammar["name"], "Widget");
    assert_eq!(grammar["scopeName"], "source.widget");
    assert_eq!(grammar["fileTypes"], json!(["widget"]));
    assert!(
        grammar["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| {
                pattern["name"] == "keyword.other.widget"
                    && pattern["match"].as_str().unwrap().contains("print")
            })
    );
}

#[test]
fn generate_tree_sitter_writes_a_grammar_js_source_file_to_stdout() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/tree-sitter-language/langmeld.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "tree-sitter",
            "--manifest-path",
            manifest.to_str().unwrap(),
            "--grammar-name",
            "widget_script",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar = String::from_utf8(output.stdout).expect("grammar source should be UTF-8");
    assert!(grammar.contains("module.exports = grammar({"));
    assert!(grammar.contains("name: \"widget_script\""));
    assert!(grammar.contains("source_file: $ => repeat(choice($.construct_"));
    assert!(grammar.contains("identifier: _ =>"));
    assert!(grammar.contains("optional(seq(\"to\", field(\"target\", $.identifier)))"));
    assert!(grammar.contains("repeat(seq(\"with\", field(\"label\", $.string)))"));
    assert!(grammar.contains("choice(seq(\"say\","));
    assert!(grammar.contains("field(\"value\", $.token)"));
    assert!(
        grammar.contains("token: $ => choice($.identifier, $.integer, $.string, $.punctuation)")
    );
}
