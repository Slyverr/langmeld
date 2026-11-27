//! The grammar gives the same trees as the parser written by hand, and a diagnostic at
//! the same place for every mistake. The two do not share codes for what only Calc has:
//! the hand-written parser reports a chained comparison as `CALC-001`, and the generic
//! engine with its own code, so diagnostics are compared by range and count.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../crates/langmeld-syntax/tests/corpus/format.rs"]
mod format;
#[path = "../../../crates/langmeld-syntax/tests/support/render.rs"]
mod render;

use std::path::Path;

use langmeld_syntax::{FileId, Parse, SourceFile, SyntaxKind, SyntaxNode};

fn source(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test.calc", text).expect("small")
}

/// What the two parsers say for a text: the tree as the corpus writes it, and where each
/// diagnostic is.
struct Outcome {
    tree: String,
    ranges: Vec<(u32, u32)>,
}

/// Splits rendered output into the tree and the ranges of the diagnostics.
fn split(rendered: &str) -> Outcome {
    let (mut tree, mut ranges) = (Vec::new(), Vec::new());
    for line in rendered.lines() {
        match line.strip_prefix("! ") {
            Some(diagnostic) => {
                let range = diagnostic
                    .split(' ')
                    .nth(1)
                    .expect("a range follows the code");
                let (start, end) = range.split_once("..").expect("a range has two ends");
                ranges.push((
                    start.parse().expect("a number"),
                    end.parse().expect("a number"),
                ));
            }
            None => tree.push(line),
        }
    }
    Outcome {
        tree: tree.join("\n"),
        ranges,
    }
}

fn by_grammar(text: &str) -> Outcome {
    let language = calc::language();
    let parsed = langmeld_syntax::parse(language, &source(text));
    assert_eq!(parsed.syntax().text().to_string(), text);
    if parsed.diagnostics.is_empty() {
        let broken = language
            .schema()
            .check(&parsed.syntax(), |kind| language.kind_name(kind).to_owned());
        assert!(broken.is_empty(), "{text:?}: {broken:?}");
    }
    split(&render::render(
        &parsed,
        &|kind| language.kind_name(kind).to_owned(),
        &|kind| language.is_trivia(kind),
    ))
}

fn by_hand(text: &str) -> Outcome {
    let name = handwritten_calc::kind_name;
    split(&render::render(
        &handwritten_calc::parse(&source(text)),
        &|kind| name(kind).to_owned(),
        &|kind| matches!(name(kind), "Whitespace" | "Comment"),
    ))
}

/// Both parsers give the same tree and a diagnostic at the same places.
fn assert_same(text: &str, what: &str) {
    let (grammar, hand) = (by_grammar(text), by_hand(text));
    assert_eq!(grammar.tree, hand.tree, "{what}");
    assert_eq!(grammar.ranges, hand.ranges, "{what}");
    let (grammar, hand) = full_trees(text);
    assert_eq!(grammar, hand, "{what}");
}

/// The whole tree, trivia included, with the kinds of nodes only: the two give tokens
/// their own kinds.
fn shape(node: &SyntaxNode, name: &dyn Fn(SyntaxKind) -> String, out: &mut String) {
    out.push('(');
    out.push_str(&name(node.kind()));
    for child in node.children_with_tokens() {
        if let Some(node) = child.as_node() {
            shape(node, name, out);
        } else if let Some(token) = child.as_token() {
            out.push_str(&format!(" {:?}", token.text()));
        }
    }
    out.push(')');
}

fn full_trees(text: &str) -> (String, String) {
    let language = calc::language();
    let (mut grammar, mut hand) = (String::new(), String::new());
    shape(
        &langmeld_syntax::parse(language, &source(text)).syntax(),
        &|kind| language.kind_name(kind).to_owned(),
        &mut grammar,
    );
    shape(
        &handwritten_calc::parse(&source(text)).syntax(),
        &|kind| handwritten_calc::kind_name(kind).to_owned(),
        &mut hand,
    );
    (grammar, hand)
}

fn corpus() -> Vec<format::Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../handwritten-calc/corpus");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .expect("the corpus folder exists")
        .map(|entry| entry.expect("directory entry is readable").path())
        .collect();
    paths.sort();
    paths
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).expect("corpus files are readable");
            format::parse(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        })
        .collect()
}

/// The cases of `tests/real-code` about syntax that Calc has, which the hand-written
/// parser passes too.
#[test]
fn the_real_code_cases_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/real-code");
    let mut count = 0;
    for name in ["errors.txt", "expressions.txt", "literals.txt", "names.txt"] {
        let text = std::fs::read_to_string(root.join(name)).expect("the cases are readable");
        for case in format::parse(&text).expect("the cases are well formed") {
            let expected = split(&case.expected);
            let grammar = by_grammar(&case.source);
            assert_eq!(grammar.tree, expected.tree, "{name}: `{}`", case.name);
            assert_eq!(grammar.ranges, expected.ranges, "{name}: `{}`", case.name);
            assert_same(&case.source, &format!("{name}: `{}`", case.name));
            count += 1;
        }
    }
    assert!(count >= 5, "only {count} cases were found");
}

#[test]
fn the_grammar_is_accepted_without_warnings() {
    assert!(calc::language().warnings().is_empty());
}

#[test]
fn every_corpus_case_gives_the_expected_tree_and_diagnostics() {
    let cases = corpus();
    assert!(cases.len() >= 15);
    for case in cases {
        let expected = split(&case.expected);
        let grammar = by_grammar(&case.source);
        assert_eq!(grammar.tree, expected.tree, "case `{}`", case.name);
        assert_eq!(grammar.ranges, expected.ranges, "case `{}`", case.name);
        assert_same(&case.source, &format!("case `{}`", case.name));
    }
}

const FIVE_MISTAKES: &str = "\
let a = 1
fn f(x { x }
let let = 2;
}
let ok = a < b < c;
";

/// The chained comparison is the one mistake with a different code: the framework's own,
/// where the hand-written parser has `CALC-001`.
#[test]
fn five_mistakes_give_the_same_five_diagnostics() {
    let language = calc::language();
    let parsed = langmeld_syntax::parse(language, &source(FIVE_MISTAKES));
    let codes: Vec<_> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        ["LM-P001", "LM-P001", "LM-P002", "LM-P001", "LM-P006"]
    );
    let by_hand = handwritten_calc::parse(&source(FIVE_MISTAKES));
    let hand: Vec<_> = by_hand.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        hand,
        ["LM-P001", "LM-P001", "LM-P002", "LM-P001", "CALC-001"]
    );
    assert_same(FIVE_MISTAKES, "the five mistakes");
}

/// Writes random well-formed Calc, the same on every run.
struct Programs {
    state: u64,
    text: String,
}

impl Programs {
    fn pick(&mut self, choices: u64) -> u64 {
        // A linear congruential generator; the high bits are the random ones.
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 33) % choices
    }

    fn write(&mut self, piece: &str) {
        self.text.push_str(piece);
        let gap = ["", " ", " ", "\n", " /* c */ ", " // c\n"];
        let gap = gap[self.pick(gap.len() as u64) as usize];
        // A word must not run into the next one.
        let word = piece.ends_with(|c: char| c.is_alphanumeric());
        self.text
            .push_str(if word && gap.is_empty() { " " } else { gap });
    }

    fn item(&mut self, depth: u32) {
        match self.pick(if depth > 2 { 2 } else { 3 }) {
            0 => {
                self.write("let");
                self.write("x");
                self.write("=");
                self.expression(depth + 1);
                self.write(";");
            }
            1 => {
                self.expression(depth + 1);
                self.write(";");
            }
            _ => {
                self.write("fn");
                self.write("f");
                self.write("(");
                for _ in 0..self.pick(3) {
                    self.write("a");
                    self.write(",");
                }
                self.write(")");
                self.block(depth + 1);
            }
        }
    }

    fn block(&mut self, depth: u32) {
        self.write("{");
        for _ in 0..self.pick(3) {
            self.item(depth);
        }
        // The last statement may leave out its `;`.
        if self.pick(2) == 0 {
            self.expression(depth + 1);
        }
        self.write("}");
    }

    fn expression(&mut self, depth: u32) {
        let choices = if depth > 4 { 2 } else { 8 };
        match self.pick(choices) {
            0 => {
                let literals = ["1", "0xf_f", "2.5e-3", "\"s\"", "true", "false"];
                let literal = literals[self.pick(literals.len() as u64) as usize];
                self.write(literal);
            }
            1 => self.write("name"),
            2 => {
                self.write("(");
                self.expression(depth + 1);
                self.write(")");
            }
            3 => {
                self.write("-");
                self.expression(depth + 1);
            }
            4 | 5 => {
                self.expression(depth + 1);
                // One comparison at most, since they do not chain.
                let operators = ["+", "-", "*", "/"];
                let operator = operators[self.pick(4) as usize];
                self.write(operator);
                self.expression(depth + 1);
            }
            6 => {
                self.write("f");
                self.write("(");
                for _ in 0..self.pick(3) {
                    self.expression(depth + 1);
                    self.write(",");
                }
                self.write(")");
            }
            _ => {
                self.write("if");
                self.expression(depth + 1);
                self.block(depth + 1);
                if self.pick(2) == 0 {
                    self.write("else");
                    self.block(depth + 1);
                }
            }
        }
    }
}

#[test]
fn well_formed_programs_give_identical_trees() {
    for seed in 0..3000 {
        let mut programs = Programs {
            state: seed,
            text: String::new(),
        };
        for _ in 0..=programs.pick(4) {
            programs.item(0);
        }
        let text = programs.text;
        let problems = handwritten_calc::parse(&source(&text)).diagnostics;
        assert!(problems.is_empty(), "seed {seed}: {text:?} {problems:?}");
        let (grammar, hand) = full_trees(&text);
        assert_eq!(grammar, hand, "seed {seed}: {text:?}");
        assert_same(&text, &format!("seed {seed}: {text:?}"));
    }
}

#[test]
fn a_larger_program_gives_identical_trees() {
    let program = "
// Adds up.
fn sum(a, b, c,) {
    let total = a + b * -c;  /* nested /* comment */ */
    if total < 10 { total } else { sum(total - 1, 0xff, 2.5e-3) }
}
let x = (sum(1, 2, 3) == 6) == true;
fn none() {}
\"text\"; x
";
    let broken = "let = 1;\nlet y 2;\nfn (a) { a }\nlet z = ;\nx = ) 3;\n";
    for text in [program.replace("; x\n", "; x;\n").as_str(), program, broken] {
        let (grammar, hand) = full_trees(text);
        assert_eq!(grammar, hand, "{text:?}");
    }
}
