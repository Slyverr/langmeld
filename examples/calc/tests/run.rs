//! What Calc programs do: the values they show, and each mistake they can make, in full.

use calc::interp::{run, run_within};
use calc::syntax::{File, language};
use langmeld::ast::AstNode;
use langmeld::kit::interp::Budget;
use langmeld::{FileId, SourceFile, parse};

/// What a program shows, one line each, or the one problem it ends with, as the
/// command line writes it.
fn shown(budget: Option<Budget>, text: &str) -> String {
    let file = SourceFile::new(FileId::new(0), "p.calc", text).expect("small");
    let parsed = parse(language(), &file);
    assert!(
        parsed.diagnostics.is_empty(),
        "{text}: {:?}",
        parsed.diagnostics
    );
    let program = File::cast(parsed.syntax()).expect("a file");
    let mut out = String::new();
    let mut output = |line: &str| {
        out.push_str(line);
        out.push('\n');
    };
    let result = match budget {
        Some(budget) => run_within(&program, file.id(), budget, &mut output),
        None => run(&program, file.id(), &mut output),
    };
    if let Err(problem) = result {
        out.push_str(&langmeld_meta::render(
            &[file],
            std::slice::from_ref(&*problem),
        ));
    }
    out
}

fn ran(text: &str) -> String {
    shown(None, text)
}

#[test]
fn expressions_work_with_whole_numbers_fractions_text_and_booleans() {
    for (program, expected) in [
        ("1 + 2 * 3;", "7\n"),
        ("(1 + 2) * 3;", "9\n"),
        ("7 - 10;", "-3\n"),
        ("-(2 + 3);", "-5\n"),
        ("10 / 5;", "2\n"),
        ("10 / 4;", "2.5\n"),
        ("1.5 + 1;", "2.5\n"),
        ("0xff + 1_000;", "1255\n"),
        ("2e3;", "2000.0\n"),
        ("\"a\" + \"b\\n\";", "ab\n\n"),
        ("1 < 2;", "true\n"),
        ("\"b\" < \"a\";", "false\n"),
        ("1 == 1.0;", "true\n"),
        ("\"a\" == 1;", "false\n"),
        ("true == true;", "true\n"),
        ("sqrt(16);", "4.0\n"),
        ("abs(-3);", "3\n"),
        ("abs(-3.5);", "3.5\n"),
    ] {
        assert_eq!(ran(program), expected, "{program}");
    }
}

#[test]
fn names_functions_and_blocks() {
    let program = "\
fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
let x = fib(15);
x;
fn later() { early() }
fn early() { 7 }
later();
let y = 1;
fn shadow(y) { y + 1 }
shadow(10);
y;
if true { let inner = 5; inner * 2 } else { 0 };
print(\"sum\", 1 + 1, true);
print(1 < 2);
";
    assert_eq!(ran(program), "610\n7\n11\n1\n10\nsum 2 true\ntrue\n");
}

#[test]
fn a_function_does_not_see_the_names_of_its_caller() {
    let program = "fn f() { secret }\nfn g() { let secret = 1; f() }\ng();";
    insta::assert_snapshot!(ran(program));
}

#[test]
fn every_mistake_is_one_problem_with_its_place_and_a_code() {
    let cases = [
        "x;",
        "1 / 0;",
        "1.5 / 0;",
        "9223372036854775807 + 1;",
        "9223372036854775808;",
        "1e400;",
        "\"a\" * 2;",
        "-\"a\";",
        "1 < \"a\";",
        "if 1 { 2 };",
        "let f = 1;\nf(2);",
        "fn f(a) { a }\nf(1, 2);",
        "fn f(a) { a }\nf();",
        "abs(1, 2);",
        "sqrt(\"a\");",
        "unknown(1);",
    ];
    let mut all = String::new();
    for program in cases {
        all.push_str(&format!("=== {program}\n{}\n", ran(program)));
    }
    insta::assert_snapshot!(all);
}

#[test]
fn programs_that_run_away_are_stopped_and_reported() {
    let forever = "fn f(n) { f(n + 1) }\nf(0);";
    insta::assert_snapshot!("calls", ran(forever));
    // A small budget of steps ends a long sum, which is fine in a large one.
    let sum = "fn count(n) { if n < 1 { 0 } else { n + count(n - 1) } }\ncount(50);";
    assert_eq!(ran(sum), "1275\n");
    insta::assert_snapshot!("steps", shown(Some(Budget::new(100, 64)), sum));
    insta::assert_snapshot!("shallow", shown(Some(Budget::new(100_000, 10)), sum));
}

mod never_panics {
    use calc::interp::run_within;
    use calc::syntax::{File, language};
    use langmeld::ast::AstNode;
    use langmeld::kit::interp::Budget;
    use langmeld::{FileId, SourceFile, parse};
    use proptest::prelude::*;

    const PIECES: [&str; 28] = [
        "let ", "fn ", "if ", "else ", "x", "y", "f", "1", "2.5", "0", "\"s\"", "true", " = ", ";",
        "(", ")", "{", "}", ",", " + ", " - ", " * ", " / ", " < ", " == ", " ", "\n", "print",
    ];

    fn programs() -> impl Strategy<Value = String> {
        proptest::collection::vec(proptest::sample::select(&PIECES[..]), 0..40)
            .prop_map(|pieces| pieces.concat())
    }

    proptest! {
        /// Whatever the text, parsing and running it ends with a result.
        #[test]
        fn any_text_ends_with_a_result(text in programs()) {
            let file = SourceFile::new(FileId::new(0), "p.calc", text).expect("small");
            let parsed = parse(language(), &file);
            if parsed.diagnostics.is_empty() {
                let program = File::cast(parsed.syntax()).expect("a file");
                let _ = run_within(&program, file.id(), Budget::new(2_000, 16), &mut |_| {});
            }
        }
    }
}
