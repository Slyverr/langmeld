//! Properties of the lexer and the parser primitives over arbitrary text: tokens
//! cover the input, trees are lossless and well nested, and the same input always
//! gives the same result. The number of cases comes from the `PROPTEST_CASES`
//! environment variable.

#![cfg(not(target_arch = "wasm32"))]

mod support;

use langmeld_syntax::{
    IdentStyle, Interpolation, Lexed, Lexer, Parse, Parser, Preset, SourceFile, StringParts,
    SyntaxKind, SyntaxNode, Terminators, TextRange, TextSize, TokenSpec,
};
use proptest::prelude::*;
use support::*;

/// A small language with newline terminators, comments and interpolated strings.
fn language() -> Lexer {
    let mut specs = puncts();
    specs.extend([
        TokenSpec::regex(WS, "whitespace", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(LINE_COMMENT, "//"),
        TokenSpec::block_comment(BLOCK_COMMENT, "/*", "*/").nested(),
        TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::preset(INT, "integer", Preset::Int(int())),
        TokenSpec::keyword(LET_KW, "let"),
        TokenSpec::preset(
            STRING,
            "string",
            Preset::String(double_quoted().interpolation(Interpolation::new(
                "{",
                "}",
                StringParts {
                    start: STR_START,
                    text: STR_TEXT,
                    end: STR_END,
                    hole_open: HOLE_OPEN,
                    hole_close: HOLE_CLOSE,
                },
            ))),
        ),
    ]);
    let terminators = Terminators::new(NEWLINE, LINE_BREAK)
        .brackets([(L_PAREN, R_PAREN), (L_BRACK, R_BRACK)])
        .blocks([(L_BRACE, R_BRACE)])
        .continue_after([PLUS, COMMA]);
    Lexer::builder()
        .tokens(specs)
        .terminators(terminators)
        .build()
        .expect("the test language is valid")
}

/// Text made of pieces that stress the lexer, or of arbitrary characters.
fn text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("a".to_string()),
        Just("let".to_string()),
        Just("12".to_string()),
        Just("0x".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("\"".to_string()),
        Just("\\".to_string()),
        Just(";".to_string()),
        Just(",".to_string()),
        Just("+".to_string()),
        Just("..=".to_string()),
        Just("//".to_string()),
        Just("/*".to_string()),
        Just("*/".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\r".to_string()),
        Just(" ".to_string()),
        Just("😀".to_string()),
        Just("\u{FEFF}".to_string()),
        ".{0,3}",
    ];
    prop_oneof![
        3 => prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat()),
        1 => ".{0,80}",
    ]
}

/// `file = item*`, where an item is a pair of parentheses around items, or any
/// other token.
fn file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_end() {
        item(p);
    }
    m.complete(p, FILE);
}

fn item(p: &mut Parser<'_>) {
    if p.at(L_PAREN) {
        let m = p.start();
        p.bump();
        while !p.at(R_PAREN) && !p.at_end() {
            item(p);
        }
        p.expect(R_PAREN);
        m.complete(p, PAREN);
    } else if p.at(R_PAREN) {
        p.recover("an expression", &[L_PAREN]);
    } else {
        p.bump();
    }
}

fn parse(lexer: &Lexer, file_text: &SourceFile, lexed: Lexed) -> Parse {
    let mut parser = Parser::new(file_text, lexed, lexer);
    file(&mut parser);
    parser.finish()
}

/// Checks that a node's children follow each other inside it, and returns how many
/// nodes and tokens there are.
fn check_nesting(node: &SyntaxNode) -> Result<usize, TestCaseError> {
    let range = node.text_range();
    let mut at = range.start();
    let mut count = 1;
    for child in node.children_with_tokens() {
        let child_range = child.text_range();
        prop_assert_eq!(child_range.start(), at, "children follow each other");
        prop_assert!(range.contains_range(child_range), "children stay inside");
        at = child_range.end();
        count += match child {
            rowan::NodeOrToken::Node(node) => check_nesting(&node)?,
            rowan::NodeOrToken::Token(_) => 1,
        };
    }
    prop_assert_eq!(at, range.end(), "children fill the node");
    Ok(count)
}

proptest! {
    #[test]
    fn tokens_cover_the_input_exactly_and_in_order(text in text()) {
        let lexer = language();
        let file = source(&text);
        let lexed = lexer.lex(&file);
        let mut end = TextSize::new(0);
        for token in &lexed.tokens {
            prop_assert_eq!(token.range.start(), end);
            prop_assert!(!token.range.is_empty());
            end = token.range.end();
            prop_assert!(text.is_char_boundary(usize::from(end)));
        }
        prop_assert_eq!(usize::from(end), text.len());
        for diagnostic in &lexed.diagnostics {
            prop_assert!(usize::from(diagnostic.range.end()) <= text.len());
        }
    }

    #[test]
    fn lexing_is_deterministic(text in text()) {
        let lexer = language();
        let file = source(&text);
        prop_assert_eq!(lexer.lex(&file), lexer.lex(&file));
    }

    #[test]
    fn joint_tokens_are_never_trivia_and_never_last(text in text()) {
        let lexer = language();
        let lexed = lexer.lex(&source(&text));
        let last_code = lexed.tokens.iter().rposition(|token| !token.trivia);
        for (index, token) in lexed.tokens.iter().enumerate() {
            if token.joint {
                prop_assert!(!token.trivia);
                prop_assert!(last_code.is_some_and(|last| index < last));
            }
        }
    }

    #[test]
    fn the_tree_is_lossless_and_well_nested(text in text()) {
        let lexer = language();
        let file = source(&text);
        let parse = parse(&lexer, &file, lexer.lex(&file));
        let root = parse.syntax();
        prop_assert_eq!(root.text().to_string(), text.as_str());
        prop_assert_eq!(
            root.text_range(),
            TextRange::up_to(TextSize::new(u32::try_from(text.len()).expect("small")))
        );
        check_nesting(&root)?;
        prop_assert!(root.kind() != SyntaxKind::EOF);
    }

    #[test]
    fn parsing_makes_progress_and_reports_in_order(text in text()) {
        let lexer = language();
        let file = source(&text);
        let parse = parse(&lexer, &file, lexer.lex(&file));
        let starts: Vec<_> = parse
            .diagnostics
            .iter()
            .map(|d| (d.range.start(), d.range.end(), d.code))
            .collect();
        let mut sorted = starts.clone();
        sorted.sort();
        prop_assert_eq!(&starts, &sorted);
        for diagnostic in &parse.diagnostics {
            prop_assert!(usize::from(diagnostic.range.end()) <= text.len());
            prop_assert!(diagnostic.code != "LM-P004", "a loop stopped consuming");
        }
    }

    #[test]
    fn parsing_is_deterministic(text in text()) {
        let lexer = language();
        let file = source(&text);
        let first = parse(&lexer, &file, lexer.lex(&file));
        let second = parse(&lexer, &file, lexer.lex(&file));
        prop_assert_eq!(first, second);
    }
}
