use langmeld_core::{SourceFile, Span};
use langmeld_language::{
    ConstructSpec, LanguageSpec, LexicalRules, ParseLimits, PatternPart, SyntaxElement,
    SyntaxNodeKind, SyntaxTokenKind, parse, parse_with_limits,
};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "WidgetScript",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::string("message"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["program"])
    .unwrap()
    .with_lexical_rules(
        LexicalRules::default()
            .line_comment("//")
            .block_comment("/*", "*/"),
    )
    .unwrap()
}

fn token_indices(node: &langmeld_language::SyntaxNode, output: &mut Vec<usize>) {
    for child in &node.children {
        match child {
            SyntaxElement::Node(child) => token_indices(child, output),
            SyntaxElement::Token(index) => output.push(*index),
        }
    }
}

#[test]
fn parse_result_tree_preserves_source_and_construct_fields() {
    let source = SourceFile::new(
        "main.widget",
        "// greeting\r\nprogram { print \"Hello\"; /* trailing */ }\r\n",
    );

    let parsed = parse(&source, &language());

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.syntax_tree.root.kind, SyntaxNodeKind::SourceFile);
    assert_eq!(
        parsed.syntax_tree.root.span,
        Span::new(0, source.text.len())
    );

    let mut indices = Vec::new();
    token_indices(&parsed.syntax_tree.root, &mut indices);
    assert_eq!(indices.len(), parsed.syntax_tree.tokens.len());
    assert!(
        indices
            .iter()
            .copied()
            .eq(0..parsed.syntax_tree.tokens.len())
    );

    let reconstructed = indices
        .iter()
        .map(|index| {
            parsed.syntax_tree.tokens[*index]
                .text(&source)
                .expect("syntax token spans should refer to the source")
        })
        .collect::<String>();
    assert_eq!(reconstructed, source.text);

    assert!(
        parsed
            .syntax_tree
            .tokens
            .iter()
            .any(|token| token.kind == SyntaxTokenKind::Comment)
    );
    assert!(
        parsed
            .syntax_tree
            .tokens
            .iter()
            .any(|token| token.kind == SyntaxTokenKind::Newline)
    );
    assert_eq!(
        parsed
            .syntax_tree
            .tokens
            .iter()
            .filter(|token| token.kind == SyntaxTokenKind::Newline)
            .map(|token| token.text(&source))
            .collect::<Vec<_>>(),
        [Some("\r\n"), Some("\r\n")]
    );

    let program = parsed.syntax_tree.root.children.iter().find_map(|child| {
        let SyntaxElement::Node(node) = child else {
            return None;
        };
        (node.kind == SyntaxNodeKind::Construct && node.name.as_deref() == Some("program"))
            .then_some(node)
    });
    let program = program.expect("program construct should appear in the syntax tree");
    let print = program.children.iter().find_map(|child| {
        let SyntaxElement::Node(node) = child else {
            return None;
        };
        (node.kind == SyntaxNodeKind::Construct && node.name.as_deref() == Some("print"))
            .then_some(node)
    });
    let print = print.expect("nested print construct should appear in the syntax tree");
    let message = print.children.iter().find_map(|child| {
        let SyntaxElement::Node(node) = child else {
            return None;
        };
        (node.kind == SyntaxNodeKind::Field && node.name.as_deref() == Some("message"))
            .then_some(node)
    });
    let message = message.expect("captured field should appear in the syntax tree");
    let message_start = source.text.find("\"Hello\"").unwrap();
    let message_span = Span::new(message_start, message_start + "\"Hello\"".len());
    assert_eq!(message.span, message_span);
    assert!(matches!(
        message.children.as_slice(),
        [SyntaxElement::Token(index)]
            if parsed.syntax_tree.tokens[*index].text(&source) == Some("\"Hello\"")
    ));
}

#[test]
fn parser_retains_unrecognized_grammar_tokens_in_the_lossless_tree() {
    let source = SourceFile::new("malformed.widget", "program { print \"Hello\"; @ }");

    let parsed = parse(&source, &language());

    assert!(!parsed.diagnostics.is_empty());
    assert!(parsed.syntax_tree.tokens.iter().any(
        |token| token.text(&source) == Some("@") && token.kind == SyntaxTokenKind::Punctuation
    ));

    let mut indices = Vec::new();
    token_indices(&parsed.syntax_tree.root, &mut indices);
    let reconstructed = indices
        .iter()
        .map(|index| {
            parsed.syntax_tree.tokens[*index]
                .text(&source)
                .expect("syntax token spans should refer to the source")
        })
        .collect::<String>();
    assert_eq!(reconstructed, source.text);
}

#[test]
fn source_limit_returns_a_bounded_unparsed_tree() {
    let source = SourceFile::new("large.widget", "program {}");

    let parsed = parse_with_limits(
        &source,
        &language(),
        ParseLimits {
            max_source_bytes: 1,
            ..ParseLimits::default()
        },
    );

    assert!(parsed.resource_limited);
    assert_eq!(parsed.syntax_tree.tokens.len(), 1);
    assert_eq!(
        parsed.syntax_tree.tokens[0].span,
        Span::new(0, source.text.len())
    );
}

#[test]
fn token_limit_preserves_the_unlexed_source_suffix() {
    let source = SourceFile::new("large.widget", "program { print \"Hello\"; }");

    let parsed = parse_with_limits(
        &source,
        &language(),
        ParseLimits {
            max_tokens: 1,
            ..ParseLimits::default()
        },
    );

    assert!(parsed.resource_limited);
    assert!(
        parsed
            .syntax_tree
            .tokens
            .iter()
            .any(|token| token.kind == SyntaxTokenKind::Unknown)
    );

    let mut indices = Vec::new();
    token_indices(&parsed.syntax_tree.root, &mut indices);
    let reconstructed = indices
        .iter()
        .map(|index| {
            parsed.syntax_tree.tokens[*index]
                .text(&source)
                .expect("syntax token spans should refer to the source")
        })
        .collect::<String>();
    assert_eq!(reconstructed, source.text);
}
