use langmeld_core::{SourceFile, Span};
use langmeld_expression::{
    Atom, Expression, ExpressionGrammar, OperatorSpec, parse_expression_until,
    parse_expression_until_with_atoms,
};

fn grammar() -> ExpressionGrammar {
    ExpressionGrammar::new([OperatorSpec::infix("+", 10), OperatorSpec::infix("*", 20)])
        .expect("operator declarations are valid")
}

#[test]
fn atom_policy_can_reserve_booleans_or_treat_them_as_identifiers() {
    let grammar =
        ExpressionGrammar::with_atoms([], [langmeld_core::ExpressionAtomKind::Identifier])
            .expect("identifier-only atom policy is valid");
    let text = "true;";
    let source = SourceFile::new("expression.calc", text);
    let (expression, _) = parse_expression_until(&source, Span::new(0, text.len()), &grammar, ";")
        .expect("true is an identifier when boolean literals are disabled");
    assert!(matches!(expression, Expression::Atom(Atom::Name(name), _) if name == "true"));

    let text = "4;";
    let source = SourceFile::new("expression.calc", text);
    let error = parse_expression_until(&source, Span::new(0, text.len()), &grammar, ";")
        .expect_err("numeric literals are disabled");
    assert_eq!(error[0].code, "LE1015");
}

#[test]
fn parses_only_through_a_top_level_terminator() {
    let prefix = "let total = ";
    let expression_text = "1 + (2 * 3); next statement";
    let text = format!("{prefix}{expression_text}");
    let source = SourceFile::new("expression.calc", &text);
    let (expression, consumed) = parse_expression_until(
        &source,
        Span::new(prefix.len(), text.len()),
        &grammar(),
        ";",
    )
    .expect("the expression ends before the semicolon");

    assert!(matches!(expression, Expression::Infix { ref operator, .. } if operator == "+"));
    assert_eq!(
        consumed,
        Span::new(prefix.len(), text.find(';').expect("semicolon exists"))
    );
    assert_eq!(
        expression.span(),
        Span::new(prefix.len(), text.find(';').expect("semicolon exists"))
    );
}

#[test]
fn parses_until_a_closing_delimiter_outside_nested_groups() {
    let text = "(1 + 2) ) trailing";
    let source = SourceFile::new("expression.calc", text);
    let (expression, consumed) =
        parse_expression_until(&source, Span::new(0, text.len()), &grammar(), ")")
            .expect("the second closing parenthesis terminates the expression");

    assert!(matches!(expression, Expression::Group { .. }));
    assert_eq!(
        consumed,
        Span::new(0, text.find(" )").expect("outer closer exists") + 1)
    );
}

#[test]
fn custom_atoms_can_contain_terminator_text() {
    let text = "value + \"left;right\"; tail";
    let source = SourceFile::new("expression.calc", text);
    let mut parse_quoted_atom = |source: &SourceFile, _range: Span, offset: usize| {
        if source.text.as_bytes().get(offset) != Some(&b'"') {
            return Ok(None);
        }
        let end = source.text[offset + 1..]
            .find('"')
            .map(|relative| offset + relative + 2)
            .ok_or_else(|| {
                langmeld_core::Diagnostic::error(
                    "TEST0001",
                    "unterminated test string",
                    Span::new(offset, source.text.len()),
                )
            })?;
        let span = Span::new(offset, end);
        Ok(Some(langmeld_expression::CustomAtomToken {
            kind: "string".to_owned(),
            text: source.text[span.start..span.end].to_owned(),
            span,
        }))
    };
    let (expression, consumed) = parse_expression_until_with_atoms(
        &source,
        Span::new(0, text.len()),
        &grammar(),
        ";",
        &mut parse_quoted_atom,
    )
    .expect("semicolon inside the host atom does not terminate parsing");

    let Expression::Infix { right, .. } = expression else {
        panic!("the infix expression is retained");
    };
    assert!(matches!(
        *right,
        Expression::Atom(Atom::Custom { ref kind, ref text }, _) if kind == "string" && text == "\"left;right\""
    ));
    assert_eq!(
        consumed.end,
        text.find("; tail").expect("terminator exists")
    );
}

#[test]
fn reports_missing_and_ambiguous_terminators() {
    let text = "1 + 2";
    let source = SourceFile::new("expression.calc", text);
    let missing = parse_expression_until(&source, Span::new(0, text.len()), &grammar(), ";")
        .expect_err("a required terminator cannot be guessed at end of input");
    assert_eq!(missing[0].code, "LE1014");
    assert_eq!(missing[0].span, Some(Span::new(text.len(), text.len())));

    let conflicting = parse_expression_until(&source, Span::new(0, text.len()), &grammar(), "+")
        .expect_err("a terminator cannot also begin an operator token");
    assert_eq!(conflicting[0].code, "LE1013");

    for quote in ["'", "\"", "\\"] {
        let invalid = parse_expression_until(&source, Span::new(0, text.len()), &grammar(), quote)
            .expect_err("quote and escape punctuation cannot be terminators");
        assert_eq!(invalid[0].code, "LE1013");
    }
}
