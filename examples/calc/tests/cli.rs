//! The text, the JSON and the exit codes of every command of the `calc` program, in full.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Runs `calc` in `folder`, and writes what it did.
fn calc_in(folder: &Path, args: &[&str]) -> String {
    let output = Command::cargo_bin("calc")
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

fn calc(args: &[&str]) -> String {
    calc_in(&here(), args)
}

// check

#[test]
fn check_of_a_good_file_says_so() {
    insta::assert_snapshot!(calc(&["check", "tests/files/good.calc"]));
}

#[test]
fn check_shows_every_problem_with_its_place() {
    insta::assert_snapshot!(calc(&["check", "tests/files/broken.calc"]));
}

#[test]
fn check_can_write_the_problems_as_json() {
    insta::assert_snapshot!(calc(&[
        "check",
        "--format",
        "json",
        "tests/files/broken.calc"
    ]));
}

#[test]
fn check_of_a_good_file_as_json_has_no_problems() {
    insta::assert_snapshot!(calc(&[
        "check",
        "--format",
        "json",
        "tests/files/good.calc"
    ]));
}

#[test]
fn check_reads_the_files_of_a_folder() {
    insta::assert_snapshot!(calc(&["check", "tests/files"]));
}

#[test]
fn check_of_a_path_that_does_not_exist_cannot_run() {
    insta::assert_snapshot!(calc(&["check", "tests/nowhere.calc"]));
}

#[test]
fn check_of_a_folder_with_no_calc_files_cannot_run() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-calc-files");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("a folder");
    insta::assert_snapshot!(calc_in(&folder, &["check"]));
}

#[test]
fn check_rejects_a_format_it_does_not_know() {
    let shown = calc(&["check", "--format", "xml", "tests/files/good.calc"]);
    assert!(shown.starts_with("exit: 2\n"), "{shown}");
    assert!(shown.contains("possible values: text, json"), "{shown}");
}

// test

fn project(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("corpus")).expect("a folder");
    folder
}

#[test]
fn test_runs_the_corpus_of_calc() {
    insta::assert_snapshot!(calc(&["test"]));
}

#[test]
fn test_shows_what_differs_and_update_makes_it_pass() {
    let folder = project("calc-corpus-failing");
    fs::write(
        folder.join("corpus/cases.txt"),
        "=== wrong tree\nlet x = 1;\n---\nFile\n",
    )
    .expect("written");
    insta::assert_snapshot!("failing", calc_in(&folder, &["test"]));
    let updated = calc_in(&folder, &["test", "--update"]);
    assert!(
        updated.starts_with("exit: 0\n--- stdout\ncorpus/cases.txt: updated 1 cases\n"),
        "{updated}"
    );
    let after = calc_in(&folder, &["test"]);
    assert!(after.starts_with("exit: 0\n"), "{after}");
    assert!(after.contains("corpus/cases.txt: 1 passed"), "{after}");
}

#[test]
fn test_without_a_corpus_cannot_run() {
    let folder = project("calc-corpus-none");
    fs::remove_dir_all(folder.join("corpus")).expect("removed");
    insta::assert_snapshot!(calc_in(&folder, &["test"]));
}

#[test]
fn test_can_take_the_folder_of_the_corpus() {
    let shown = calc(&["test", "--corpus", "corpus"]);
    assert!(shown.starts_with("exit: 0\n"), "{shown}");
}

// explain

#[test]
fn explain_gives_the_long_form_of_a_code_of_the_framework() {
    insta::assert_snapshot!(calc(&["explain", "LM-P006"]));
}

#[test]
fn explain_gives_the_long_form_of_a_code_of_the_language() {
    insta::assert_snapshot!(calc(&["explain", "CALC002"]));
}

#[test]
fn explain_of_a_code_that_does_not_exist_cannot_run() {
    insta::assert_snapshot!(calc(&["explain", "CALC999"]));
}

// the commands of the language

#[test]
fn run_shows_what_a_file_shows() {
    insta::assert_snapshot!(calc(&["run", "tests/files/good.calc"]));
}

#[test]
fn run_ends_with_the_problems_of_a_file_that_does_not_parse() {
    insta::assert_snapshot!(calc(&["run", "tests/files/broken.calc"]));
}

#[test]
fn run_ends_with_the_mistake_a_program_makes() {
    insta::assert_snapshot!(calc(&["run", "tests/files/wrong.calc"]));
}

#[test]
fn run_of_a_file_that_is_not_there_cannot_run() {
    insta::assert_snapshot!(calc(&["run", "tests/nowhere.calc"]));
}

#[test]
fn eval_works_out_an_expression() {
    insta::assert_snapshot!(calc(&["eval", "(1 + 2) * 3"]));
    insta::assert_snapshot!(
        "eval_with_a_semicolon",
        calc(&["eval", "let x = 4; x * x;"])
    );
    let wrong = calc(&["eval", "1 / 0"]);
    assert!(wrong.starts_with("exit: 1\n"), "{wrong}");
}

// the program itself

#[test]
fn help_and_version_say_what_the_program_is() {
    insta::assert_snapshot!(calc(&["--help"]));
    let version = calc(&["--version"]);
    assert_eq!(
        version,
        format!(
            "exit: 0\n--- stdout\ncalc {}\n--- stderr\n",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn a_command_that_does_not_exist_cannot_run() {
    let shown = calc(&["frobnicate"]);
    assert!(shown.starts_with("exit: 2\n"), "{shown}");
    assert!(shown.contains("unrecognized subcommand"), "{shown}");
    let nothing = calc(&[]);
    assert!(nothing.starts_with("exit: 2\n"), "{nothing}");
}

#[test]
fn a_copy_named_for_a_command_acts_as_that_command() {
    let built = assert_cmd::cargo::cargo_bin("calc");
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("calc-multicall");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("a folder");
    let name = if cfg!(windows) {
        "calc-eval.exe"
    } else {
        "calc-eval"
    };
    fs::copy(built, folder.join(name)).expect("copied");
    let output = std::process::Command::new(folder.join(name))
        .arg("2 + 2")
        .output()
        .expect("runs");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n");
    assert_eq!(output.status.code(), Some(0));
}

// names across files

#[test]
fn check_follows_use_across_files() {
    insta::assert_snapshot!(calc(&["check", "tests/project"]));
}

#[test]
fn check_shows_names_nothing_defines_with_a_suggestion() {
    insta::assert_snapshot!(calc(&["check", "tests/typo/main.calc"]));
}

#[test]
fn run_runs_the_files_a_program_uses() {
    insta::assert_snapshot!(calc(&["run", "tests/project/main.calc"]));
}

#[test]
fn run_says_when_a_used_file_cannot_be_read() {
    insta::assert_snapshot!(calc(&["run", "tests/typo/lost.calc"]));
}
