//! `langmeld init` and `langmeld generate`: what they write, and what they refuse.

mod support;

use std::fs;

use support::{facade, folder, langmeld, tree};

fn paths(files: &[(String, String)]) -> Vec<&str> {
    files.iter().map(|(path, _)| path.as_str()).collect()
}

#[test]
fn a_new_language_is_a_project_of_its_own_that_the_command_can_check_and_test() {
    let root = folder("init-new");
    let ran = langmeld(&root, &["init", "tiny-lang", "--langmeld-path", &facade()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        "Created the Tiny language in tiny-lang. Try it:\n\n    cd tiny-lang\n    \
         cargo run -- run examples/hello.tiny\n\nREADME.md says what to change.\n"
    );
    let project = root.join("tiny-lang");
    let files = tree(&project);
    assert_eq!(
        paths(&files),
        [
            ".gitignore",
            "Cargo.toml",
            "README.md",
            "build.rs",
            "corpus/basics.txt",
            "examples/hello.tiny",
            "langmeld.toml",
            "language/main.langmeld",
            "src/cli.rs",
            "src/interp.rs",
            "src/lib.rs",
            "src/main.rs"
        ]
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml")).expect("written");
    assert!(manifest.contains("name = \"tiny-lang\""), "{manifest}");
    // A project of its own, even inside another workspace.
    assert!(manifest.contains("[workspace]"), "{manifest}");
    assert!(manifest.contains(&facade()), "{manifest}");

    let ran = langmeld(&project, &["check"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("language Tiny is valid"), "{}", ran.out);
    assert!(ran.err.is_empty(), "{}", ran.err);
    let ran = langmeld(&project, &["test"]);
    assert_eq!(
        (ran.code, ran.out.as_str()),
        (0, "corpus/basics.txt: 2 passed\ntest result: ok. 2 cases\n"),
        "{}",
        ran.err
    );
    let ran = langmeld(
        &project,
        &[
            "parse",
            "--grammar",
            "language/main.langmeld",
            "examples/hello.tiny",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
}

#[test]
fn a_release_is_what_a_new_project_depends_on_unless_told_otherwise() {
    let root = folder("init-release");
    assert_eq!(langmeld(&root, &["init", "plain"]).code, 0);
    let manifest = fs::read_to_string(root.join("plain/Cargo.toml")).expect("written");
    assert!(
        manifest.contains(&format!(
            "langmeld = {{ version = \"{}\", features = [\"cli\", \"kit\"] }}",
            env!("CARGO_PKG_VERSION")
        )),
        "{manifest}"
    );
}

#[test]
fn the_layout_of_the_programs_is_chosen_with_bins() {
    let root = folder("init-bins");
    let path = facade();
    let init = |name: &str, bins: &str| {
        langmeld(
            &root,
            &["init", name, "--bins", bins, "--langmeld-path", &path],
        )
    };
    let ran = init("one-lang", "single");
    assert_eq!(ran.code, 0, "{}", ran.err);
    let ran = init("split-lang", "split");
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out
            .contains("cargo run --bin split -- run examples/hello.split"),
        "{}",
        ran.out
    );
    let ran = init("many-lang", "multicall");
    assert_eq!(ran.code, 0, "{}", ran.err);

    let main_of = |name: &str| paths_of(&root.join(name));
    assert!(main_of("one-lang").contains(&"src/main.rs".to_owned()));
    let split = main_of("split-lang");
    assert!(split.contains(&"src/bin/split.rs".to_owned()), "{split:?}");
    assert!(split.contains(&"src/bin/splitc.rs".to_owned()), "{split:?}");
    assert!(!split.contains(&"src/main.rs".to_owned()), "{split:?}");
    assert!(main_of("many-lang").contains(&"src/main.rs".to_owned()));
    for (name, layout) in [
        ("one-lang", "single"),
        ("split-lang", "split"),
        ("many-lang", "multicall"),
    ] {
        let manifest = fs::read_to_string(root.join(name).join("langmeld.toml")).expect("there");
        assert_eq!(manifest, format!("[bins]\nlayout = \"{layout}\"\n"));
        // The manifest is one the tools read.
        assert_eq!(langmeld(&root.join(name), &["check"]).code, 0, "{name}");
    }
    // The README says how to run the programs that the layout makes.
    let readme = |name: &str| fs::read_to_string(root.join(name).join("README.md")).expect("there");
    assert!(readme("one-lang").contains("cargo run -- run examples/hello.one"));
    assert!(readme("many-lang").contains("cargo run -- test"));
    let split = readme("split-lang");
    assert!(
        split.contains("cargo run --bin split -- run examples/hello.split"),
        "{split}"
    );
    assert!(
        split.contains("cargo run --bin splitc -- check examples"),
        "{split}"
    );
    assert!(!split.contains("{{"), "{split}");
    let ran = init("bad-lang", "sideways");
    assert_eq!(ran.code, 2);
    assert!(
        ran.err
            .contains("possible values: single, split, multicall"),
        "{}",
        ran.err
    );
}

fn paths_of(project: &std::path::Path) -> Vec<String> {
    tree(project).into_iter().map(|(path, _)| path).collect()
}

#[test]
fn a_library_joins_a_workspace_and_changes_nothing_else() {
    let root = folder("init-lib");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"3\"\nmembers = [\n    \"app\",\n]\n",
    )
    .expect("written");
    fs::create_dir_all(root.join("app/src")).expect("created");
    fs::write(
        root.join("app/Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("written");
    fs::write(root.join("app/src/main.rs"), "fn main() {}\n").expect("written");
    let before = tree(&root);

    let ran = langmeld(
        &root,
        &[
            "init",
            "--lib",
            "crates/shapes-lang",
            "--langmeld-path",
            &facade(),
        ],
    );
    assert_eq!(ran.code, 0, "{}{}", ran.err, ran.out);
    assert_eq!(
        ran.out,
        "Added crates/shapes-lang to the workspace.\nCreated the Shapes language in \
         crates/shapes-lang. Its programs come from `shapes_lang::cli`.\n"
    );

    let after = tree(&root);
    let (new, old): (Vec<_>, Vec<_>) = after
        .iter()
        .partition(|(path, _)| path.starts_with("crates/shapes-lang/"));
    // Besides the new crate, only the list of members of the workspace changed.
    let changed: Vec<_> = old
        .iter()
        .filter(|entry| !before.contains(entry))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(changed, ["Cargo.toml"]);
    assert_eq!(old.len(), before.len());
    let manifest = &old
        .iter()
        .find(|(path, _)| path == "Cargo.toml")
        .expect("there")
        .1;
    assert_eq!(
        manifest,
        "[workspace]\nresolver = \"3\"\nmembers = [\n    \"app\",\n    \"crates/shapes-lang\",\n]\n"
    );

    let names: Vec<_> = new.iter().map(|(path, _)| path.as_str()).collect();
    let readme = &new
        .iter()
        .find(|(path, _)| path.ends_with("README.md"))
        .expect("written")
        .1;
    assert!(readme.contains("shapes_lang::cli::app().run()"), "{readme}");
    assert!(!readme.contains("cargo run"), "{readme}");
    assert!(
        names.contains(&"crates/shapes-lang/src/lib.rs"),
        "{names:?}"
    );
    assert!(
        !names.contains(&"crates/shapes-lang/src/main.rs"),
        "{names:?}"
    );
    let member = &new
        .iter()
        .find(|(path, _)| path.ends_with("Cargo.toml"))
        .expect("written")
        .1;
    assert!(!member.contains("[workspace]"), "{member}");
    assert!(member.contains("name = \"shapes-lang\""), "{member}");
}

#[test]
fn a_library_goes_where_the_workspace_already_expects_members() {
    let root = folder("init-lib-glob");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .expect("written");
    let ran = langmeld(
        &root,
        &[
            "init",
            "--lib",
            "crates/glob-lang",
            "--langmeld-path",
            &facade(),
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.contains("Added"), "{}", ran.out);
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("there");
    assert_eq!(manifest, "[workspace]\nmembers = [\"crates/*\"]\n");
}

#[test]
fn init_refuses_what_it_would_overwrite_or_cannot_name() {
    let root = folder("init-refuse");
    fs::create_dir_all(root.join("taken")).expect("created");
    fs::write(root.join("taken/file"), "x").expect("written");
    let ran = langmeld(&root, &["init", "taken"]);
    assert_eq!(ran.code, 2);
    assert_eq!(ran.err, "error: taken is not empty\n");
    assert_eq!(
        fs::read_to_string(root.join("taken/file")).expect("kept"),
        "x"
    );

    let ran = langmeld(&root, &["init", "1abc"]);
    assert_eq!(ran.code, 2);
    assert_eq!(
        ran.err,
        "error: `1abc` cannot name a package: use letters, digits, `-` and `_`, starting \
         with a letter\n"
    );
    for name in ["a b", "ü", "-x", "langmeld", "core"] {
        let ran = langmeld(&root, &["init", "--", name]);
        assert_eq!(ran.code, 2, "{name:?}: {}", ran.err);
    }
    assert_eq!(langmeld(&root, &["init"]).code, 2);

    // Outside any workspace there is nothing for a library to join.
    let outside = std::env::temp_dir().join(format!("langmeld-init-{}", std::process::id()));
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(&outside).expect("created");
    let ran = langmeld(&outside, &["init", "--lib", "crates/x"]);
    let left = fs::read_dir(&outside).expect("there").count();
    fs::remove_dir_all(&outside).expect("removed");
    assert_eq!((ran.code, left), (2, 0), "{}", ran.err);
    assert_eq!(
        ran.err,
        "error: there is no workspace around that folder; `langmeld init NAME` makes a \
         project of its own\n"
    );
}

#[test]
fn rules_and_handlers_are_added_to_a_project() {
    let root = folder("generate");
    assert_eq!(langmeld(&root, &["init", "gen-lang"]).code, 0);
    let project = root.join("gen-lang");

    let ran = langmeld(&project, &["generate", "rule", "Repeat"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.starts_with("Added rule `Repeat` to "),
        "{}",
        ran.out
    );
    let grammar = fs::read_to_string(project.join("language/main.langmeld")).expect("there");
    assert!(
        grammar.ends_with("\nrule Repeat = \"repeat\" \";\"\n"),
        "{grammar}"
    );
    // Until something refers to it, the new rule is unused.
    let ran = langmeld(&project, &["check"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.err.contains("LM-G006"), "{}", ran.err);
    let ran = langmeld(&project, &["generate", "rule", "Repeat"]);
    assert_eq!(
        (ran.code, ran.err.as_str()),
        (2, "error: there is already a `Repeat`\n")
    );
    let ran = langmeld(&project, &["generate", "rule", "lower"]);
    assert_eq!(
        (ran.code, ran.err.as_str()),
        (
            2,
            "error: write the rule's name in PascalCase, not `lower`\n"
        )
    );

    let ran = langmeld(&project, &["generate", "handler", "Let"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let handlers = fs::read_to_string(project.join("src/handlers.rs")).expect("written");
    assert!(
        handlers.contains("pub fn let_(node: &syntax::Let)"),
        "{handlers}"
    );
    assert_eq!(
        langmeld(&project, &["generate", "handler", "Print"]).code,
        0
    );
    let handlers = fs::read_to_string(project.join("src/handlers.rs")).expect("written");
    assert_eq!(handlers.matches("use crate::syntax;").count(), 1);
    assert!(
        handlers.contains("pub fn print(node: &syntax::Print)"),
        "{handlers}"
    );
    let ran = langmeld(&project, &["generate", "handler", "Let"]);
    assert_eq!(ran.code, 2);
    assert!(ran.err.contains("already has `let_`"), "{}", ran.err);
    let ran = langmeld(&project, &["generate", "handler", "Nope"]);
    assert_eq!(
        (ran.code, ran.err.as_str()),
        (2, "error: there is no node `Nope` in the grammar\n")
    );
}
