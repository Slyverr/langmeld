//! Starts a language with `langmeld init` for every layout of programs, builds it, and
//! runs what was built. Building takes minutes the first time, so these tests are
//! ignored by `cargo xtask check` and run by `cargo xtask ci`:
//!
//! ```sh
//! cargo nextest run -p langmeld-cli --test e2e --run-ignored only
//! ```

mod support;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use support::{Ran, facade, folder, langmeld};

/// The folder that every project builds into, so that the framework is built once.
fn target() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-target")
}

/// Runs Cargo in `project`, with the versions of the packages that this repository uses.
fn cargo(project: &Path, arguments: &[&str]) -> Ran {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    if !project.join("Cargo.lock").exists() {
        fs::copy(lock, project.join("Cargo.lock")).expect("the lock file is copied");
    }
    let output = Command::new(env!("CARGO"))
        .args(arguments)
        .current_dir(project)
        .env("CARGO_TARGET_DIR", target())
        .output()
        .expect("cargo runs");
    Ran::of(output)
}

/// Runs every command that the README of `project` tells a newcomer to run, as it says.
fn follow_readme(project: &Path) {
    let readme = fs::read_to_string(project.join("README.md")).expect("written");
    let commands: Vec<_> = readme
        .lines()
        .filter(|line| line.starts_with("cargo run "))
        .map(|line| line.split('#').next().unwrap_or(line).trim())
        .collect();
    assert!(commands.len() >= 4, "{commands:?}");
    // The language server waits for an editor, so a test of its own talks to it.
    assert!(
        commands.iter().any(|command| command.ends_with(" lsp")),
        "the README does not say how to start the language server: {commands:?}"
    );
    for command in commands.into_iter().filter(|c| !c.ends_with(" lsp")) {
        let words: Vec<_> = command.split_whitespace().skip(1).collect();
        let ran = cargo(project, &words);
        assert_eq!(ran.code, 0, "{command}: {}", ran.err);
    }
}

/// A new project laid out as `layout`, named `name`-lang, built.
fn started(name: &str, layout: &str) -> PathBuf {
    let root = folder(&format!("e2e-{name}"));
    let ran = langmeld(
        &root,
        &[
            "init",
            &format!("{name}-lang"),
            "--bins",
            layout,
            "--langmeld-path",
            &facade(),
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let project = root.join(format!("{name}-lang"));
    let built = cargo(&project, &["build", "--quiet"]);
    assert_eq!(built.code, 0, "{}", built.err);
    let formatted = cargo(&project, &["fmt", "--check"]);
    assert_eq!(formatted.code, 0, "{}{}", formatted.out, formatted.err);
    follow_readme(&project);
    project
}

fn program(name: &str) -> Command {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    Command::new(target().join("debug").join(file))
}

fn ran(command: &mut Command, project: &Path) -> Ran {
    Ran::of(
        command
            .current_dir(project)
            .output()
            .expect("the program runs"),
    )
}

#[test]
#[ignore = "builds a new project; `cargo xtask ci` runs it"]
fn a_single_program_builds_and_has_every_command() {
    let project = started("single", "single");
    let run = ran(
        program("single").args(["run", "examples/hello.single"]),
        &project,
    );
    assert_eq!((run.code, run.out.as_str()), (0, "42\n"), "{}", run.err);
    let test = ran(program("single").arg("test"), &project);
    assert_eq!(
        (test.code, test.out.as_str()),
        (0, "corpus/basics.txt: 2 passed\ntest result: ok. 2 cases\n"),
        "{}",
        test.err
    );
    // An editor starts the language server, asks to initialize and shut down, and goes.
    let framed = |text: &str| format!("Content-Length: {}\r\n\r\n{text}", text.len());
    let mut server = program("single")
        .arg("lsp")
        .current_dir(&project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let mut input = server.stdin.take().expect("piped");
    for message in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ] {
        input
            .write_all(framed(message).as_bytes())
            .expect("the server reads");
    }
    drop(input);
    let served = server.wait_with_output().expect("the server ends");
    assert_eq!(served.status.code(), Some(0));
    let said = String::from_utf8_lossy(&served.stdout);
    assert!(said.contains("language server"), "{said}");
    assert!(said.contains("documentSymbolProvider"), "{said}");
    let check = ran(program("single").args(["check", "examples"]), &project);
    assert_eq!(
        (check.code, check.out.as_str()),
        (0, "checked 1 file, 0 errors, 0 warnings\n"),
        "{}",
        check.err
    );
    let explain = ran(program("single").args(["explain", "SINGLE002"]), &project);
    assert_eq!(explain.code, 0, "{}", explain.err);
    assert!(explain.out.contains("divided by zero"), "{}", explain.out);
    // What the program finds wrong is shown, and ends in failure.
    fs::write(
        project.join("examples/bad.single"),
        "let x = 1 / 0;\nprint x;\nprint y;\n",
    )
    .expect("written");
    let bad = ran(
        program("single").args(["run", "examples/bad.single"]),
        &project,
    );
    assert_eq!(bad.code, 1, "{}", bad.err);
    assert!(bad.err.contains("SINGLE002"), "{}", bad.err);
}

#[test]
#[ignore = "builds a new project; `cargo xtask ci` runs it"]
fn split_programs_each_have_their_own_commands() {
    let project = started("split", "split");
    let run = ran(
        program("split").args(["run", "examples/hello.split"]),
        &project,
    );
    assert_eq!((run.code, run.out.as_str()), (0, "42\n"), "{}", run.err);
    // The program that runs programs has no tools in it, and the other has no `run`.
    let missing = ran(program("split").args(["check", "examples"]), &project);
    assert_eq!(missing.code, 2, "{}", missing.err);
    let tools = ran(program("splitc").args(["check", "examples"]), &project);
    assert_eq!(
        (tools.code, tools.out.as_str()),
        (0, "checked 1 file, 0 errors, 0 warnings\n"),
        "{}",
        tools.err
    );
    let test = ran(program("splitc").arg("test"), &project);
    assert_eq!(test.code, 0, "{}", test.err);
    let missing = ran(
        program("splitc").args(["run", "examples/hello.split"]),
        &project,
    );
    assert_eq!(missing.code, 2, "{}", missing.err);
}

#[test]
#[ignore = "builds a new project; `cargo xtask ci` runs it"]
fn a_multicall_program_acts_as_the_command_it_is_named_for() {
    let project = started("multi", "multicall");
    let run = ran(
        program("multi").args(["run", "examples/hello.multi"]),
        &project,
    );
    assert_eq!((run.code, run.out.as_str()), (0, "42\n"), "{}", run.err);
    // A copy under another name behaves as that command.
    let copy = |name: &str| {
        let from = target()
            .join("debug")
            .join(if cfg!(windows) { "multi.exe" } else { "multi" });
        let to = target()
            .join("debug")
            .join(format!("{name}{}", if cfg!(windows) { ".exe" } else { "" }));
        fs::copy(from, &to).expect("copied");
        to
    };
    let check = copy("multi-check");
    let acted = ran(Command::new(check).arg("examples"), &project);
    assert_eq!(
        (acted.code, acted.out.as_str()),
        (0, "checked 1 file, 0 errors, 0 warnings\n"),
        "{}",
        acted.err
    );
    let named = copy("multi-run");
    let acted = ran(Command::new(named).arg("examples/hello.multi"), &project);
    assert_eq!(
        (acted.code, acted.out.as_str()),
        (0, "42\n"),
        "{}",
        acted.err
    );
}

#[test]
#[ignore = "builds a new project; `cargo xtask ci` runs it"]
fn a_language_library_builds_inside_a_workspace() {
    let root = folder("e2e-lib");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"3\"\nmembers = []\n",
    )
    .expect("written");
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
    assert_eq!(ran.code, 0, "{}", ran.err);
    let built = cargo(&root, &["build", "--quiet", "--workspace"]);
    assert_eq!(built.code, 0, "{}", built.err);
    let formatted = cargo(&root, &["fmt", "--all", "--check"]);
    assert_eq!(formatted.code, 0, "{}{}", formatted.out, formatted.err);
}
