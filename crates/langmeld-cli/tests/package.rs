//! `langmeld package vscode`: the files it writes, in full for Calc, and what each
//! setting changes.

mod support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use support::{folder, langmeld, tree};

fn read(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("written");
    serde_json::from_str(&text).unwrap_or_else(|problem| panic!("{}: {problem}", path.display()))
}

const GRAMMAR: &str = r##"langmeld 1
language Shape_Script {
    extensions "shapes" "shp"
    comments { line "#"  block "(*" "*)" }
}
tokens {
    skip space = /[ \n]+/
    word       = preset ident(ascii)
    text       = preset string(double, single)
}
rule File = Item*
rule Item = "draw" word "(" text ")" "+="
"##;

#[test]
fn the_extension_of_calc_is_written_in_full() {
    let calc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/calc");
    let root = folder("package-calc");
    let ran = langmeld(
        &root,
        &[
            "package",
            "vscode",
            calc.to_str().expect("UTF-8"),
            "--out",
            "extension",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let mut shown = String::new();
    for (path, text) in tree(&root.join("extension")) {
        shown.push_str(&format!("=== {path}\n{text}\n"));
    }
    insta::assert_snapshot!(shown);
}

#[test]
fn an_extension_follows_the_grammar_and_the_manifest() {
    let root = folder("package-vscode");
    fs::create_dir_all(root.join("language")).expect("created");
    fs::write(root.join("language/main.langmeld"), GRAMMAR).expect("written");
    fs::write(root.join("logo.png"), "not really a picture").expect("written");
    fs::write(
        root.join("langmeld.toml"),
        "[bins]\nserver = [\"shapesc\", \"lsp\", \"--quiet\"]\n[package.vscode]\n\
         display-name = \"Shapes\"\npublisher = \"someone\"\nicon = \"logo.png\"\n\
         out = \"editor\"\n[project]\nmanifest = \"shapes.toml\"\n",
    )
    .expect("written");
    let ran = langmeld(&root, &["package", "vscode"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("npm install"), "{}", ran.out);
    let out = root.join("editor");

    let package = read(&out.join("package.json"));
    assert_eq!(package["name"], "shape-script-language");
    assert_eq!(package["displayName"], "Shapes");
    assert_eq!(package["publisher"], "someone");
    assert_eq!(package["icon"], "logo.png");
    assert!(out.join("logo.png").is_file());
    let language = &package["contributes"]["languages"][0];
    assert_eq!(language["id"], "shape-script");
    assert_eq!(language["extensions"], json!([".shapes", ".shp"]));
    assert!(package["dependencies"]["vscode-languageclient"].is_string());

    let configuration = read(&out.join("language-configuration.json"));
    assert_eq!(configuration["comments"]["lineComment"], "#");
    assert_eq!(
        configuration["comments"]["blockComment"],
        json!(["(*", "*)"])
    );
    assert_eq!(configuration["brackets"], json!([["(", ")"]]));
    assert_eq!(
        configuration["autoClosingPairs"],
        json!([
            { "open": "(", "close": ")" },
            { "open": "\"", "close": "\"" },
            { "open": "'", "close": "'" },
        ])
    );
    assert_eq!(
        configuration["wordPattern"],
        r"(-?\d+(\.\d+)?)|([A-Za-z_][A-Za-z0-9_]*)"
    );

    let colours = read(&out.join("syntaxes/shape-script.tmLanguage.json"));
    assert_eq!(colours["scopeName"], "source.shape-script");
    let rules = colours["patterns"].to_string();
    for part in [
        "\\\\b(draw)\\\\b",
        "comment.line",
        "comment.block",
        "string.quoted",
    ] {
        assert!(rules.contains(part), "{part} in {rules}");
    }

    let extension = fs::read_to_string(out.join("src/extension.ts")).expect("written");
    assert!(extension.contains("LanguageClient"), "{extension}");
    assert!(extension.contains("\"shapesc\""), "{extension}");
    assert!(extension.contains("[\"lsp\",\"--quiet\"]"), "{extension}");
    // The project file of the language's users goes to the server too.
    assert!(
        extension.contains("{ pattern: \"**/shapes.toml\" }"),
        "{extension}"
    );
    assert_eq!(
        package["activationEvents"],
        json!(["workspaceContains:**/shapes.toml"])
    );
    for file in ["tsconfig.json", ".vscodeignore", "README.md", ".gitignore"] {
        assert!(out.join(file).is_file(), "{file}");
    }
}

#[test]
fn without_settings_the_program_is_named_after_the_language() {
    let root = folder("package-plain");
    fs::write(
        root.join("tiny.langmeld"),
        "langmeld 1\nlanguage Tiny {}\nrule File = \"!\"\n",
    )
    .expect("written");
    let ran = langmeld(&root, &["package", "vscode", "tiny.langmeld", "--out", "x"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let package = read(&root.join("x/package.json"));
    assert_eq!(package["publisher"], "local");
    assert!(package.get("icon").is_none());
    let extension = fs::read_to_string(root.join("x/src/extension.ts")).expect("written");
    assert!(
        extension.contains("\"tiny\"") && extension.contains("[\"lsp\"]"),
        "{extension}"
    );
}

#[test]
fn a_split_project_starts_its_tools_program() {
    let root = folder("package-split");
    fs::write(
        root.join("tiny.langmeld"),
        "langmeld 1\nlanguage Tiny {}\nrule File = \"!\"\n",
    )
    .expect("written");
    fs::write(
        root.join("langmeld.toml"),
        "root = \"tiny.langmeld\"\n[bins]\nlayout = \"split\"\n",
    )
    .expect("written");
    let ran = langmeld(&root, &["package", "vscode", "--out", "x"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let extension = fs::read_to_string(root.join("x/src/extension.ts")).expect("written");
    assert!(extension.contains("\"tinyc\""), "{extension}");
}

#[test]
fn grammar_files_have_an_extension_of_their_own() {
    let root = folder("package-langmeld");
    let ran = langmeld(&root, &["package", "vscode", "--langmeld", "--out", "x"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let package = read(&root.join("x/package.json"));
    assert_eq!(
        package["contributes"]["languages"][0]["extensions"],
        json!([".langmeld"])
    );
    let extension = fs::read_to_string(root.join("x/src/extension.ts")).expect("written");
    assert!(extension.contains("\"langmeld\""), "{extension}");
    assert!(extension.contains("[\"lsp\"]"), "{extension}");
}

#[test]
fn a_grammar_with_problems_makes_no_extension() {
    let root = folder("package-broken");
    fs::write(
        root.join("bad.langmeld"),
        "langmeld 1\nrule File = Missing\n",
    )
    .expect("written");
    let ran = langmeld(&root, &["package", "vscode", "bad.langmeld", "--out", "x"]);
    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(!root.join("x").exists());
    assert!(
        ran.err
            .contains("so there is nothing to make an extension from"),
        "{}",
        ran.err
    );
}

#[test]
fn a_path_that_does_not_exist_cannot_run() {
    let root = folder("package-missing");
    let ran = langmeld(&root, &["package", "vscode", "nowhere"]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(
        ran.err.contains("`nowhere` is not a file or a folder"),
        "{}",
        ran.err
    );
}
