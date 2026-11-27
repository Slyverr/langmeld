//! Runs the corpus cases in the repository's `tests/` folder.
//!
//! `tests/README.md` describes the file format.

#![cfg(not(target_arch = "wasm32"))]

mod format;
#[path = "../support/real_code.rs"]
mod real_code;
#[path = "../support/render.rs"]
mod render;
mod runner;
#[path = "../support/mod.rs"]
mod support;

use std::path::Path;

use format::{Case, FormatError};
use langmeld_syntax::{Lexer, Parse, SyntaxKind, SyntaxNode};
use runner::{Outcome, run_case};

/// Nothing parses the framework corpus yet.
fn nothing(_file: &str, _source: &str) -> Option<String> {
    None
}

#[test]
fn framework_corpus() {
    check_directory("corpus", |_| true, nothing);
}

/// The cases of `real-code/` that Calc's syntax does not cover, parsed by grammars
/// declared for them.
#[test]
fn real_code_corpus() {
    check_directory("real-code", real_code::covers, real_code::parse_tree);
}

#[test]
fn real_code_tokens() {
    check_directory(
        "real-code/tokens",
        |_| true,
        |_, source| lex_tokens(&support::c_like(), source),
    );
}

#[test]
fn real_code_tokens_with_newlines() {
    check_directory(
        "real-code/tokens/terminated",
        |_| true,
        |_, source| lex_tokens(&support::terminated(), source),
    );
}

#[test]
fn real_code_tokens_with_interpolation() {
    check_directory(
        "real-code/tokens/interpolated",
        |_| true,
        |_, source| lex_tokens(&support::interpolating("{"), source),
    );
}

/// Every token of the source as `KIND "text"`, then every diagnostic.
fn lex_tokens(lexer: &Lexer, source: &str) -> Option<String> {
    let lexed = support::lex(lexer, source);
    let tokens = lexed.tokens.iter().map(|token| {
        let kind = support::kind_name(token.kind);
        let joint = if token.joint { " joint" } else { "" };
        format!("{kind} {:?}{joint}", &source[token.range])
    });
    let problems = support::errors(&lexed.diagnostics);
    let problems = problems.iter().map(|problem| format!("! {problem}"));
    Some(tokens.chain(problems).collect::<Vec<_>>().join("\n"))
}

/// Runs the `.txt` files of one corpus folder that `covered` accepts, in name order.
fn check_directory(
    directory: &str,
    covered: fn(&str) -> bool,
    subject: fn(&str, &str) -> Option<String>,
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests")
        .join(directory);
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", root.display()))
        .map(|entry| entry.expect("directory entry is readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .filter(|path| {
            covered(
                &path
                    .file_name()
                    .expect("listed files have names")
                    .to_string_lossy(),
            )
        })
        .collect();
    paths.sort();

    let mut report = String::new();
    let mut problems = Vec::new();
    for path in paths {
        let file = path.file_name().expect("listed files have names");
        let file = file.to_string_lossy();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {file}: {error}"));
        let cases =
            format::parse(&text).unwrap_or_else(|error| panic!("{directory}/{file}: {error}"));
        assert!(!cases.is_empty(), "{directory}/{file} has no cases");
        for case in cases {
            let outcome = run_case(&case, |source| subject(&file, source));
            report.push_str(&format!("{file}: {}: {}\n", case.name, outcome.label()));
            if let Some(problem) = outcome.problem(&case) {
                problems.push(format!("{directory}/{file}:{}: {problem}", case.line));
            }
        }
    }

    assert!(problems.is_empty(), "\n{}", problems.join("\n\n"));
    // The snapshot makes an added, removed or newly passing case visible in review.
    insta::with_settings!({prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(directory.replace('/', "-"), report);
    });
}

// The format.

fn case(name: &str, line: usize, source: &str, expected: &str) -> Case {
    Case {
        name: name.into(),
        line,
        source: source.into(),
        expected: expected.into(),
        expected_failure: false,
    }
}

fn error(line: usize, message: &str) -> Result<Vec<Case>, FormatError> {
    Err(FormatError {
        line,
        message: message.into(),
    })
}

#[test]
fn parses_cases_in_order() {
    let text =
        "=== first\nlet x = 1\nlet y = 2;\n---\nFile\n  LetStmt\n\n=== second\nx\n---\nFile\n";
    assert_eq!(
        format::parse(text),
        Ok(vec![
            case("first", 1, "let x = 1\nlet y = 2;", "File\n  LetStmt"),
            case("second", 8, "x", "File"),
        ])
    );
}

#[test]
fn empty_text_has_no_cases() {
    assert_eq!(format::parse(""), Ok(vec![]));
    assert_eq!(format::parse("\n\n"), Ok(vec![]));
}

#[test]
fn source_is_kept_byte_for_byte() {
    // CRLF, a lone CR, a byte order mark inside the source and an emoji.
    let text = "=== bytes\r\n\u{FEFF}a\r\nb\rc 😀\r\n---\r\nFile\r\n  Name\r\n";
    assert_eq!(
        format::parse(text),
        Ok(vec![case(
            "bytes",
            1,
            "\u{FEFF}a\r\nb\rc 😀",
            "File\n  Name"
        )])
    );
}

#[test]
fn blank_line_before_the_separator_keeps_a_final_newline() {
    let cases = format::parse("=== newline\nx\n\n---\nFile\n").expect("valid");
    assert_eq!(cases[0].source, "x\n");
}

#[test]
fn source_may_be_empty_and_expected_may_be_missing() {
    assert_eq!(
        format::parse("=== nothing\n---"),
        Ok(vec![case("nothing", 1, "", "")])
    );
}

#[test]
fn only_the_first_separator_ends_the_source() {
    let cases = format::parse("=== rules\n=== not a header\n---\nFile\n---\n").expect("valid");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].source, "=== not a header");
    assert_eq!(cases[0].expected, "File\n---");
}

#[test]
fn reads_the_expected_failure_marker() {
    let cases = format::parse("=== later [expected-failure]\nx\n---\nFile\n").expect("valid");
    assert_eq!(cases[0].name, "later");
    assert!(cases[0].expected_failure);
}

#[test]
fn byte_order_mark_before_the_first_header_is_ignored() {
    let cases = format::parse("\u{FEFF}=== bom\nx\n---\n").expect("valid");
    assert_eq!(cases[0].name, "bom");
}

#[test]
fn rejects_malformed_files() {
    assert_eq!(
        format::parse("\nstray\n=== a\n---\n"),
        error(2, "expected a case header starting with `=== `")
    );
    assert_eq!(
        format::parse("=== a\nx\n"),
        error(1, "case `a` has no `---` line after its source")
    );
    assert_eq!(format::parse("===  \n---\n"), error(1, "case has no name"));
    assert_eq!(
        format::parse("=== [expected-failure]\n---\n"),
        error(1, "case has no name")
    );
    assert_eq!(
        format::parse("=== a [expected-fail]\n---\n"),
        error(
            1,
            "unknown marker `[expected-fail]`; the only marker is `[expected-failure]`"
        )
    );
    assert_eq!(
        format::parse("=== a\n---\n=== a\n---\n"),
        error(3, "there is already a case named `a`")
    );
}

#[test]
fn format_errors_name_the_line() {
    let error = FormatError {
        line: 4,
        message: "case has no name".into(),
    };
    assert_eq!(error.to_string(), "line 4: case has no name");
}

#[test]
fn never_panics_on_any_short_text() {
    const PIECES: [&str; 8] = ["=== ", "---", "a", " [", "]", "\n", "\r", "\u{FEFF}"];
    for len in 0..=5u32 {
        for mut number in 0..PIECES.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push_str(PIECES[number % PIECES.len()]);
                number /= PIECES.len();
            }
            // Either result is fine; returning at all is the test.
            let _ = format::parse(&text);
        }
    }
}

// The runner.

fn upper(source: &str) -> Option<String> {
    Some(source.to_uppercase())
}

#[test]
fn matching_output_passes() {
    let outcome = run_case(&case("a", 1, "x", "X"), upper);
    assert_eq!(outcome, Outcome::Passed);
    assert_eq!(outcome.label(), "passed");
    assert_eq!(outcome.problem(&case("a", 1, "x", "X")), None);
}

#[test]
fn different_output_fails_and_shows_both() {
    let case = case("a", 1, "x", "Y");
    let outcome = run_case(&case, upper);
    assert_eq!(
        outcome,
        Outcome::Failed {
            actual: Some("X".into())
        }
    );
    assert_eq!(outcome.label(), "failed");
    assert_eq!(
        outcome.problem(&case).as_deref(),
        Some("case `a` failed\n--- expected\nY\n--- actual\nX")
    );
}

#[test]
fn missing_parser_fails_a_normal_case() {
    let case = case("a", 1, "x", "X");
    let outcome = run_case(&case, |_| None);
    assert_eq!(outcome, Outcome::Failed { actual: None });
    assert_eq!(
        outcome.problem(&case).as_deref(),
        Some("case `a` failed\n--- expected\nX\n--- actual\n(nothing can parse this source)")
    );
}

#[test]
fn expected_failure_accepts_any_failure() {
    let mut case = case("a", 1, "x", "Y");
    case.expected_failure = true;
    for outcome in [run_case(&case, upper), run_case(&case, |_| None)] {
        assert_eq!(outcome, Outcome::ExpectedFailure);
        assert_eq!(outcome.label(), "expected failure");
        assert_eq!(outcome.problem(&case), None);
    }
}

#[test]
fn expected_failure_that_passes_is_a_problem() {
    let mut case = case("a", 1, "x", "X");
    case.expected_failure = true;
    let outcome = run_case(&case, upper);
    assert_eq!(outcome, Outcome::UnexpectedPass);
    assert_eq!(outcome.label(), "unexpected pass");
    assert_eq!(
        outcome.problem(&case).as_deref(),
        Some("case `a` passes now; remove its `[expected-failure]` marker")
    );
}

#[test]
fn line_endings_of_the_output_do_not_matter() {
    let case = case("a", 1, "x", "A\nB");
    let outcome = run_case(&case, |_| Some("A\r\nB\r\n".into()));
    assert_eq!(outcome, Outcome::Passed);
}
