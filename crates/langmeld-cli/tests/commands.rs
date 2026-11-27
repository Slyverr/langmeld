//! `langmeld explain` and `langmeld check --exhaustive`, in full.

mod support;

use std::fs;

use support::{folder, langmeld};

#[test]
fn explain_says_what_a_code_means_and_how_to_solve_it() {
    let root = folder("explain");
    let ran = langmeld(&root, &["explain", "LM-P001"]);
    insta::assert_snapshot!(ran.shown());
}

#[test]
fn explain_of_a_code_that_does_not_exist_cannot_run() {
    let root = folder("explain-unknown");
    let ran = langmeld(&root, &["explain", "LM-X999"]);
    insta::assert_snapshot!(ran.shown());
    let ran = langmeld(&root, &["explain"]);
    assert_eq!(ran.code, 2, "{}", ran.err);
}

/// A project with a grammar and the code that handles some of its nodes.
fn project(name: &str, code: &str) -> std::path::PathBuf {
    let root = folder(name);
    fs::create_dir_all(root.join("language")).expect("created");
    fs::create_dir_all(root.join("src")).expect("created");
    fs::write(
        root.join("language/main.langmeld"),
        "langmeld 1\nlanguage T { extensions \"t\" }\n\
         tokens { skip space = /[ \\n]+/\n word = preset ident(ascii)\n number = preset int(dec) }\n\
         rule File = Item*\nrule Item = Greeting | Count\n\
         rule Greeting = \"hello\" who:word\nrule Count = \"count\" total:number\n",
    )
    .expect("written");
    fs::write(root.join("src/lib.rs"), code).expect("written");
    root
}

#[test]
fn exhaustive_lists_the_nodes_the_code_never_handles() {
    let root = project(
        "exhaustive-missing",
        "use syntax::{File, Item};\n\
         fn run(file: &File) {\n    for item in file.items() {\n        match item {\n            \
         Item::Greeting(_) => {}\n            _ => {}\n        }\n    }\n}\n",
    );
    let ran = langmeld(&root, &["check", "--exhaustive"]);
    insta::assert_snapshot!(
        ran.shown()
            .replace(root.to_str().expect("UTF-8"), "<project>")
    );
}

#[test]
fn exhaustive_is_content_when_every_node_is_named() {
    let root = project(
        "exhaustive-all",
        "use syntax::{File, Item, Count, Greeting};\n\
         fn run(file: &File) {\n    for item in file.items() {\n        match item {\n            \
         Item::Greeting(_) => {}\n            Item::Count(_) => {}\n        }\n    }\n}\n\
         fn visit(_: &Count, _: &Greeting) {}\n",
    );
    let ran = langmeld(&root, &["check", "--exhaustive"]);
    insta::assert_snapshot!(
        ran.shown()
            .replace(root.to_str().expect("UTF-8"), "<project>")
    );
}

#[test]
fn exhaustive_needs_a_src_folder_to_read() {
    let root = project("exhaustive-no-src", "");
    fs::remove_dir_all(root.join("src")).expect("removed");
    let ran = langmeld(&root, &["check", "--exhaustive"]);
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(ran.err.contains("there is no src folder in"), "{}", ran.err);
}

#[test]
fn a_grammar_with_problems_is_not_read_for_what_the_code_handles() {
    let root = project("exhaustive-broken", "");
    fs::write(
        root.join("language/main.langmeld"),
        "langmeld 1\nrule File = Nope\n",
    )
    .expect("written");
    let ran = langmeld(&root, &["check", "--exhaustive"]);
    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(!ran.out.contains("handles"), "{}", ran.out);
}
