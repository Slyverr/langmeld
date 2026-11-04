use langmeld_core::{LexicalSpec, SourceFile, Span};
use langmeld_expression::{
    ExpressionGrammar, OperatorSpec, parse_expression_until_with_lexical_spec,
};

fn grammar() -> ExpressionGrammar {
    ExpressionGrammar::new([OperatorSpec::infix("+", 10)]).expect("valid expression grammar")
}

#[test]
fn configured_comments_are_skipped_until_the_expression_terminator() {
    let source = SourceFile::new("comments.expr", "1 + /* block */ 2 // line comment\n + 3;");
    let lexical = LexicalSpec::new()
        .line_comment("//")
        .block_comment("/*", "*/");

    let (expression, consumed) = parse_expression_until_with_lexical_spec(
        &source,
        Span::new(0, source.text.len()),
        &grammar(),
        ";",
        &lexical,
    )
    .expect("expression with comments parses");

    assert_eq!(expression.span(), Span::new(0, 38));
    assert_eq!(consumed, Span::new(0, 38));
}

#[test]
fn configured_unterminated_block_comments_report_their_source_span() {
    let source = SourceFile::new("comments.expr", "1 + /* never closes");
    let lexical = LexicalSpec::new().block_comment("/*", "*/");

    let diagnostics = parse_expression_until_with_lexical_spec(
        &source,
        Span::new(0, source.text.len()),
        &grammar(),
        ";",
        &lexical,
    )
    .expect_err("unterminated block comments fail");

    assert_eq!(diagnostics[0].code, "LE1015");
    assert_eq!(diagnostics[0].span, Some(Span::new(4, source.text.len())));
}

#[test]
fn invalid_comment_delimiters_are_rejected_before_lexing() {
    let source = SourceFile::new("comments.expr", "1 + 2;");
    let lexical = LexicalSpec::new().line_comment("");

    let diagnostics = parse_expression_until_with_lexical_spec(
        &source,
        Span::new(0, source.text.len()),
        &grammar(),
        ";",
        &lexical,
    )
    .expect_err("empty line-comment delimiters are invalid");

    assert_eq!(diagnostics[0].code, "LE1016");
}
