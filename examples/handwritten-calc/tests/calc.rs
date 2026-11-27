//! Calc, parsed by hand on the primitives.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../crates/langmeld-syntax/tests/corpus/format.rs"]
mod format;
// This test uses only part of the shared runner.
#[allow(dead_code)]
#[path = "../../../crates/langmeld-syntax/tests/corpus/runner.rs"]
mod runner;

use std::fmt::Write;
use std::path::Path;

use handwritten_calc::{kind_name, parse};
use langmeld_syntax::{FileId, SourceFile, SyntaxNode};

fn parsed(text: &str) -> langmeld_syntax::Parse {
    let file = SourceFile::new(FileId::new(0), "test.calc", text).expect("small");
    let parse = parse(&file);
    assert_eq!(
        parse.syntax().text().to_string(),
        text,
        "the tree is lossless"
    );
    parse
}

/// The tree without trivia, one node or token per line; a node holding only tokens
/// takes one line. Diagnostics follow.
fn render(text: &str) -> String {
    let parse = parsed(text);
    let mut out = String::new();
    render_node(&parse.syntax(), 0, &mut out);
    for d in &parse.diagnostics {
        let (start, end) = (u32::from(d.range.start()), u32::from(d.range.end()));
        writeln!(out, "! {} {start}..{end} {}", d.code, d.message).expect("writes to a string");
    }
    out.pop();
    out
}

fn render_node(node: &SyntaxNode, depth: usize, out: &mut String) {
    let children: Vec<_> = node
        .children_with_tokens()
        .filter(|child| {
            let name = kind_name(child.kind());
            !matches!(name, "Whitespace" | "Comment")
        })
        .collect();
    let indent = "  ".repeat(depth);
    let name = kind_name(node.kind());
    let only_tokens = depth > 0
        && !children.is_empty()
        && children.iter().all(|child| child.as_token().is_some())
        && matches!(name, "Name" | "NameRef" | "Literal" | "Error");
    out.push_str(&indent);
    out.push_str(name);
    for child in children {
        if let Some(token) = child.as_token() {
            if only_tokens {
                write!(out, " {:?}", token.text()).expect("writes to a string");
            } else {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                writeln!(out, "{indent}  {:?}", token.text()).expect("writes to a string");
            }
        } else if let Some(child) = child.as_node() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            render_node(child, depth + 1, out);
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Runs every `.txt` file of a folder, in name order, and snapshots the outcomes so an
/// added, removed or newly passing case shows in review.
fn check_directory(root: &Path, snapshot: &str, skip: &[&str]) -> usize {
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", root.display()))
        .map(|entry| entry.expect("directory entry is readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .filter(|path| !skip.iter().any(|name| path.ends_with(name)))
        .collect();
    paths.sort();
    let mut report = String::new();
    let mut problems = Vec::new();
    let mut count = 0;
    for path in paths {
        let name = path.file_name().expect("listed files have names");
        let name = name.to_string_lossy();
        let text = std::fs::read_to_string(&path).expect("corpus files are readable");
        let cases = format::parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        for case in cases {
            count += 1;
            let outcome = runner::run_case(&case, |source| Some(render(source)));
            let diagnostics = case
                .expected
                .lines()
                .filter(|l| l.starts_with("! "))
                .count();
            report.push_str(&format!(
                "{name}: {}: {}, {diagnostics} diagnostics\n",
                case.name,
                outcome.label()
            ));
            if let Some(problem) = outcome.problem(&case) {
                problems.push(format!("{name}:{}: {problem}", case.line));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n\n"));
    insta::with_settings!({prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(snapshot, report);
    });
    count
}

#[test]
fn corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let count = check_directory(&root, "corpus", &[]);
    assert!(count >= 15, "only {count} cases were found");
}

/// The syntax cases of the repository's `tests/real-code` folder. The files about syntax
/// that Calc does not have are run by the syntax crate's tests, on grammars of their own.
#[test]
fn real_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/real-code");
    check_directory(
        &root,
        "real-code",
        &["generics.txt", "operators.txt", "characters.txt"],
    );
}

const FIVE_MISTAKES: &str = "\
let a = 1
fn f(x { x }
let let = 2;
}
let ok = a < b < c;
";

/// One file with five different mistakes gives five diagnostics, one each, and a tree
/// that still covers every line.
#[test]
fn five_mistakes_give_five_diagnostics() {
    let parse = parsed(FIVE_MISTAKES);
    let found: Vec<_> = parse
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message.as_str()))
        .collect();
    assert_eq!(found.len(), 5, "{found:#?}");
    let codes: Vec<_> = found.iter().map(|(code, _)| *code).collect();
    assert_eq!(
        codes,
        ["LM-P001", "LM-P001", "LM-P002", "LM-P001", "CALC-001"],
        "{found:#?}"
    );
}

#[test]
fn deep_nesting_is_reported() {
    for text in [
        format!("{}1{};", "(".repeat(5000), ")".repeat(5000)),
        format!("{}1;", "-".repeat(5000)),
        format!("{}{}", "fn f() {".repeat(5000), "}".repeat(5000)),
        format!("1{};", " + 1".repeat(20_000)),
        "if a { ".repeat(5000),
    ] {
        let parse = parsed(&text);
        assert!(
            parse.diagnostics.iter().any(|d| d.code == "LM-P003"),
            "{:?}",
            parse.diagnostics
        );
        assert_within_limit(&parse);
    }
}

/// No node has more ancestors, itself included, than the tree depth limit allows.
fn assert_within_limit(parse: &langmeld_syntax::Parse) {
    let limit = langmeld_syntax::ParseLimits::default().max_tree_depth as usize;
    let deepest = parse
        .syntax()
        .descendants()
        .map(|node| node.ancestors().count())
        .max()
        .unwrap_or(0);
    assert!(deepest <= limit, "the tree is {deepest} deep, not {limit}");
}

/// When the root has to be left out, the node that holds the rest is part of the tree
/// too, so the tree must still fit. The input is one that the fuzzer found: many `-`
/// and `<` in a row, which deepen the tree without recursion.
#[test]
fn a_tree_without_its_root_still_fits_the_limit() {
    const PIECES: [&str; 28] = [
        "let ", "fn ", "if ", "else ", "true", "x", "1", "2.5", "\"s\"", "(", ")", "{", "}", ",",
        ";", "=", "==", "<", "+", "-", "*", "/", " ", "\n", "//", "/*", "*/", "@",
    ];
    let bytes = include_bytes!("../../../fuzz/seeds/parse_handwritten_calc/deep_tree");
    let text: String = bytes
        .iter()
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    assert_within_limit(&parsed(&text));
}

#[test]
fn any_short_text_can_be_parsed() {
    const PIECES: [&str; 16] = [
        "let ", "fn ", "if ", "else ", "x", "1", "(", ")", "{", "}", ",", ";", "=", "<", "-", "\n",
    ];
    for len in 0..=4u32 {
        for mut number in 0..PIECES.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push_str(PIECES[number % PIECES.len()]);
                number /= PIECES.len();
            }
            let diagnostics = parsed(&text).diagnostics;
            assert!(
                diagnostics.iter().all(|d| d.code != "LM-P004"),
                "{text:?} made no progress"
            );
        }
    }
}
