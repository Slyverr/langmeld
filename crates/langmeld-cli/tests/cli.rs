//! The text and exit codes of every command, in full.

use std::path::Path;

use assert_cmd::Command;

/// Runs `langmeld` in the folder of the sample projects, and writes what it did.
fn run_in(folder: &Path, args: &[&str]) -> String {
    let output = Command::cargo_bin("langmeld")
        .expect("the binary is built")
        .current_dir(folder)
        .args(args)
        .output()
        .expect("the binary runs");
    format!(
        "exit: {}\n--- stdout\n{}--- stderr\n{}",
        output.status.code().expect("exited"),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn projects() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/projects"))
}

fn run(args: &[&str]) -> String {
    run_in(projects(), args)
}

#[test]
fn with_no_command_it_shows_what_it_can_do() {
    insta::assert_snapshot!(run(&[]));
}

#[test]
fn check_accepts_a_project_and_says_so() {
    insta::assert_snapshot!(run(&["check", "pairs"]));
}

#[test]
fn check_can_explain_where_it_looks_ahead() {
    insta::assert_snapshot!(run(&["check", "pairs", "--explain"]));
}

#[test]
fn check_shows_every_problem_with_its_place_and_fix() {
    insta::assert_snapshot!(run(&["check", "broken/broken.langmeld"]));
}

#[test]
fn check_of_a_path_that_does_not_exist_cannot_run() {
    insta::assert_snapshot!(run(&["check", "nowhere"]));
}

#[test]
fn test_runs_the_corpus_and_shows_what_differs() {
    insta::assert_snapshot!(run(&["test", "pairs"]));
}

#[test]
fn test_reports_failures_and_passes_that_should_not_be() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("failing-corpus");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("corpus")).expect("a folder");
    for name in ["langmeld.toml", "pairs.langmeld"] {
        std::fs::copy(projects().join("pairs").join(name), folder.join(name)).expect("copied");
    }
    std::fs::write(
        folder.join("corpus/cases.txt"),
        "=== wrong tree\nspeed: 3\n---\nFile\n\n=== now fine [expected-failure]\n\n---\nFile\n",
    )
    .expect("written");
    let shown = run_in(&folder, &["test"]);
    insta::assert_snapshot!(shown);
    // Updating rewrites what the cases expect, and then they pass.
    let updated = run_in(&folder, &["test", "--update"]);
    assert!(updated.starts_with("exit: 0\n--- stdout\ncorpus/cases.txt: updated 1 cases\n"));
    let after = run_in(&folder, &["test"]);
    assert!(after.contains("corpus/cases.txt: 1 passed"), "{after}");
}

#[test]
fn parse_reads_a_file_with_only_the_grammar_and_reports_custom_rules_once() {
    insta::assert_snapshot!(run(&[
        "parse",
        "--grammar",
        "external/external.langmeld",
        "external/sample.arrows"
    ]));
}

#[test]
fn parse_of_a_file_that_cannot_be_read_cannot_run() {
    insta::assert_snapshot!(run(&[
        "parse",
        "--grammar",
        "pairs/pairs.langmeld",
        "pairs/missing.pairs"
    ]));
}

#[test]
fn parse_does_not_run_with_a_grammar_that_has_problems() {
    insta::assert_snapshot!(run(&[
        "parse",
        "--grammar",
        "broken/broken.langmeld",
        "pairs/corpus/lines.txt"
    ]));
}
