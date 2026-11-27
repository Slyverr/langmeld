//! Development commands for the repository: `cargo xtask <command>`.
//!
//! `check` is the fast loop to run before pushing; `ci` runs the set a pull request
//! must pass. Each step is printed, and the first failure stops the run.

use std::env;
use std::process::{Command, ExitCode};
use std::time::Instant;

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  check   Fast local checks: format, lint, tests (64 property cases), WASM build
  ci      The pull-request set: check with 256 cases, doc tests, WASM tests in Node,
          and the end-to-end build of new languages
";

/// Runs the command named on the command line.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (steps, label) = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["check"] => (check_steps("64"), "check"),
        ["ci"] => (ci_steps(), "ci"),
        [] | ["help" | "--help" | "-h"] => {
            print!("{USAGE}");
            return ExitCode::from(2);
        }
        _ => {
            eprintln!("unknown command: {}\n", args.join(" "));
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    run_all(label, &steps)
}

/// One command to run, with a name to print and extra environment variables.
struct Step {
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    env: Vec<(&'static str, &'static str)>,
}

/// Builds a step that runs `cargo` with the given arguments.
fn cargo(name: &'static str, args: &'static [&'static str]) -> Step {
    Step {
        name,
        program: "cargo",
        args,
        env: Vec::new(),
    }
}

/// The steps of `cargo xtask check`, with the given number of property test cases.
fn check_steps(cases: &'static str) -> Vec<Step> {
    let mut tests = cargo("tests", &["nextest", "run", "--workspace", "--locked"]);
    tests.env.push(("PROPTEST_CASES", cases));
    vec![
        cargo("format", &["fmt", "--all", "--check"]),
        cargo(
            "format (fuzz)",
            &["fmt", "--manifest-path", "fuzz/Cargo.toml", "--check"],
        ),
        cargo(
            "format (xtask)",
            &["fmt", "--manifest-path", "xtask/Cargo.toml", "--check"],
        ),
        cargo(
            "lint",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        tests,
        cargo(
            "WASM build",
            &[
                "build",
                "-p",
                "langmeld-syntax",
                "-p",
                "langmeld-meta",
                "-p",
                "langmeld-ide",
                "--target",
                "wasm32-unknown-unknown",
                "--locked",
            ],
        ),
    ]
}

/// The steps of `cargo xtask ci`: the checks with more cases, plus slower ones.
fn ci_steps() -> Vec<Step> {
    let mut steps = check_steps("256");
    steps.push(cargo(
        "doc tests",
        &["test", "--workspace", "--doc", "--locked"],
    ));
    let mut wasm = cargo(
        "WASM tests (Node)",
        &[
            "test",
            "-p",
            "langmeld-syntax",
            "--target",
            "wasm32-unknown-unknown",
            "--locked",
        ],
    );
    wasm.env.push((
        "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER",
        "wasm-bindgen-test-runner",
    ));
    steps.push(wasm);
    // Starting a language and building it takes minutes without a warm build folder, so
    // the fast loop leaves it out.
    steps.push(cargo(
        "end-to-end (init, then build every layout)",
        &[
            "nextest",
            "run",
            "-p",
            "langmeld-cli",
            "--test",
            "e2e",
            "--run-ignored",
            "only",
            "--locked",
        ],
    ));
    steps.push(cargo(
        "lint (xtask)",
        &[
            "clippy",
            "--manifest-path",
            "xtask/Cargo.toml",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ));
    steps
}

/// Runs the steps in order, stopping at the first failure, and prints the total time.
fn run_all(label: &str, steps: &[Step]) -> ExitCode {
    let start = Instant::now();
    let total = steps.len();
    for (number, step) in steps.iter().enumerate() {
        println!("[{}/{total}] {}", number + 1, step.name);
        let mut command = Command::new(step.program);
        command.args(step.args).envs(step.env.iter().copied());
        match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("xtask {label}: step \"{}\" failed ({status})", step.name);
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!(
                    "xtask {label}: cannot run `{}` for step \"{}\": {error}",
                    step.program, step.name
                );
                return ExitCode::FAILURE;
            }
        }
    }
    println!(
        "xtask {label}: all {total} steps passed in {:.1?}",
        start.elapsed()
    );
    ExitCode::SUCCESS
}
