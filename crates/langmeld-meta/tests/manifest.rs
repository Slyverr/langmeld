//! `langmeld.toml`: what is read, and the full text of what is refused.

use langmeld_meta::{Layout, Manifest, render};
use langmeld_syntax::{FileId, SourceFile};

fn read(text: &str) -> Result<Manifest, String> {
    let file = SourceFile::new(FileId::new(0), "langmeld.toml", text).expect("small");
    Manifest::parse(&file).map_err(|problems| render(&[file], &problems))
}

#[test]
fn an_empty_manifest_means_the_defaults() {
    let manifest = read("").expect("valid");
    assert_eq!(manifest, Manifest::default());
    assert_eq!(manifest.root, "language/main.langmeld");
    assert_eq!(manifest.bins.layout, Layout::Single);
}

#[test]
fn every_table_is_read() {
    let manifest = read(
        r#"
root = "src/calc.langmeld"

[bins]
layout = "split"
server = ["calc", "lsp"]

[project]
manifest = "calc.toml"

[package.vscode]
display-name = "Calc"
publisher = "someone"
icon = "assets/icon.png"
out = "../editors/vscode"
"#,
    )
    .expect("valid");
    assert_eq!(manifest.root, "src/calc.langmeld");
    assert_eq!(manifest.bins.layout, Layout::Split);
    assert_eq!(manifest.bins.server, ["calc", "lsp"]);
    assert_eq!(manifest.project.expect("set").manifest, "calc.toml");
    let vscode = manifest.package.vscode.expect("set");
    assert_eq!(vscode.display_name.as_deref(), Some("Calc"));
    assert_eq!(vscode.out.as_deref(), Some("../editors/vscode"));
}

#[test]
fn repeating_a_language_field_is_refused() {
    insta::assert_snapshot!(
        read("extensions = [\"calc\"]\n\n[language]\nname = \"Calc\"\n").expect_err("repeated")
    );
}

#[test]
fn an_unknown_key_is_refused_with_the_known_ones() {
    insta::assert_snapshot!(read("[bins]\nlayuot = \"split\"\n").expect_err("unknown"));
}

#[test]
fn wrong_values_are_refused_where_they_are_written() {
    insta::assert_snapshot!(
        read("root = 3\n[bins]\nlayout = \"many\"\nserver = \"calc\"\n").expect_err("wrong")
    );
}

#[test]
fn text_that_is_not_toml_is_one_mistake() {
    insta::assert_snapshot!(read("root = \n[bins\n").expect_err("broken"));
}
