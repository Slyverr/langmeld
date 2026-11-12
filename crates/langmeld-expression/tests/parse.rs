use langmeld_core::{SourceDatabase, SourceFile, Span};
use langmeld_expression::{
    Associativity, ExpressionGrammar, ExpressionKind, ExpressionLimits, ExpressionParser,
    OperatorSpec,
};

fn arithmetic() -> ExpressionGrammar {
    ExpressionGrammar::new([
        OperatorSpec::prefix("-", 4),
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("-", 1, Associativity::Left),
        OperatorSpec::infix("*", 3, Associativity::Left),
        OperatorSpec::infix("^", 5, Associativity::Right),
    ])
    .unwrap()
}

#[test]
fn respects_precedence_and_left_associativity() {
    let source = SourceFile::new("expression", "10 - 3 - 2 * 4");
    let parsed =
        ExpressionParser::new(&arithmetic()).parse(&source, Span::new(0, source.text().len()));

    assert!(parsed.is_valid());
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let parsed = parsed.into_parts();
    let Some(ExpressionKind::Infix {
        operator,
        left,
        right,
    }) = parsed.expression.map(|expression| expression.kind)
    else {
        panic!("expected an infix root");
    };
    assert_eq!(operator, "-");
    assert!(matches!(left.kind, ExpressionKind::Infix { operator, .. } if operator == "-"));
    assert!(matches!(right.kind, ExpressionKind::Infix { operator, .. } if operator == "*"));
}

#[test]
fn applies_right_associativity_and_distinguishes_prefix_operators() {
    let grammar = arithmetic();
    let source = SourceFile::new("expression", "-2 ^ 3 ^ 4");
    let parsed = ExpressionParser::new(&grammar).parse(&source, Span::new(0, source.text().len()));

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let parsed = parsed.into_parts();
    let Some(ExpressionKind::Prefix { operand, .. }) =
        parsed.expression.map(|expression| expression.kind)
    else {
        panic!("expected a prefix root");
    };
    let ExpressionKind::Infix {
        operator, right, ..
    } = operand.kind
    else {
        panic!("expected exponentiation beneath prefix operator");
    };
    assert_eq!(operator, "^");
    assert!(matches!(right.kind, ExpressionKind::Infix { operator, .. } if operator == "^"));
}

#[test]
fn parses_calls_members_and_indexes_with_source_spans() {
    let grammar = arithmetic();
    let source = SourceFile::new("expression", "make(1, object.field[2])");
    let parsed = ExpressionParser::new(&grammar).parse(&source, Span::new(0, source.text().len()));

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Some(expression) = parsed.expression() else {
        panic!("expected a call expression");
    };
    assert_eq!(expression.span, Span::new(0, source.text().len()));
    let ExpressionKind::Call { arguments, .. } = &expression.kind else {
        panic!("expected a call root");
    };
    assert_eq!(arguments.len(), 2);
    let ExpressionKind::Index { target, index } = &arguments[1].kind else {
        panic!("expected an indexed member expression");
    };
    assert!(matches!(&target.kind, ExpressionKind::Member { .. }));
    assert!(matches!(&index.kind, ExpressionKind::Integer(value) if value == "2"));
}

#[test]
fn parse_until_leaves_the_terminator_for_its_caller() {
    let grammar = arithmetic();
    let source = SourceFile::new("statement", "1 + 2; next");
    let parsed = ExpressionParser::new(&grammar).parse_until(
        &source,
        Span::new(0, source.text().len()),
        &[";"],
    );

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.consumed(), Span::new(0, 5));

    let parts = parsed.into_parts();
    assert!(parts.diagnostics.is_empty());
    assert!(parts.expression.is_some());
    assert_eq!(parts.consumed, Span::new(0, 5));
    assert_eq!(parts.source_identity, source.identity());
    assert!(!parts.resource_limited);
}

#[test]
fn reports_invalid_ranges_and_malformed_expressions() {
    let grammar = arithmetic();
    let parser = ExpressionParser::new(&grammar);
    let source = SourceFile::new("expression", "1 + )");

    let invalid_range = parser.parse(&source, Span::new(0, source.text().len() + 1));
    assert_eq!(invalid_range.diagnostics()[0].code, "LM-EXPR-001");

    let malformed = parser.parse(&source, Span::new(0, source.text().len()));
    assert!(!malformed.is_valid());
    assert!(malformed.expression().is_none());
    assert!(
        malformed
            .diagnostics()
            .iter()
            .any(|d| d.code == "LM-EXPR-002")
    );
}

#[test]
fn invalid_ranges_return_a_safe_empty_consumed_span() {
    let grammar = arithmetic();
    let parser = ExpressionParser::new(&grammar);
    let source = SourceFile::new("expression", "é + 1");
    let source_len = source.text().len();

    for range in [
        Span::new(1, source_len),
        Span::new(0, 1),
        Span::new(usize::MAX, usize::MAX),
    ] {
        let parsed = parser.parse(&source, range);
        let consumed = parsed.consumed();

        assert_eq!(parsed.diagnostics()[0].code, "LM-EXPR-001");
        assert!(parsed.expression().is_none());
        assert!(consumed.is_empty());
        assert!(consumed.end <= source_len);
        assert!(source.text().is_char_boundary(consumed.start));
    }
}

#[test]
fn parse_result_retains_the_managed_source_identity() {
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("expression", "1 + )").unwrap();
    let source = sources.get(source_id).unwrap();
    let grammar = arithmetic();
    let parser = ExpressionParser::new(&grammar);
    let invalid_range = parser.parse(source, Span::new(0, source.text().len() + 1));

    let parsed = parser.parse(source, Span::new(0, source.text().len()));

    assert_eq!(invalid_range.source_identity(), source.identity());
    assert!(!parsed.is_valid());
    assert_eq!(parsed.source_identity(), source.identity());

    let invalid_parts = invalid_range.into_parts();
    assert_eq!(invalid_parts.source_identity, source.identity());

    let parsed_parts = parsed.into_parts();
    assert_eq!(parsed_parts.source_identity, source.identity());
    assert!(!parsed_parts.diagnostics.is_empty());
}

#[test]
fn caps_nested_expression_depth() {
    let grammar = arithmetic();
    let nested = format!("{}1{}", "(".repeat(140), ")".repeat(140));
    let source = SourceFile::new("nested", nested);
    let parsed = ExpressionParser::new(&grammar).parse(&source, Span::new(0, source.text().len()));

    assert!(parsed.diagnostics().iter().any(|d| d.code == "LM-EXPR-004"));
    assert!(parsed.resource_limited());
}

#[test]
fn rejects_expression_ranges_over_the_configured_source_limit() {
    let source = SourceFile::new("expression", "prefix 1 + 2 suffix");
    let grammar = arithmetic();
    let parser = ExpressionParser::with_limits(
        &grammar,
        ExpressionLimits {
            max_source_bytes: 4,
            ..ExpressionLimits::default()
        },
    );

    let parsed = parser.parse(&source, Span::new(7, 12));

    assert!(!parsed.is_valid());
    assert!(parsed.resource_limited());
    assert!(parsed.expression().is_none());
    assert_eq!(parsed.diagnostics()[0].code, "LM-EXPR-012");
    assert_eq!(
        parsed.diagnostics()[0].location.as_ref().unwrap().span(),
        Span::new(11, 11)
    );
}

#[test]
fn token_limit_keeps_the_parsed_expression_prefix_and_marks_it_limited() {
    let source = SourceFile::new("expression", "1 + 2 + 3");
    let grammar = arithmetic();
    let parser = ExpressionParser::with_limits(
        &grammar,
        ExpressionLimits {
            max_tokens: 3,
            ..ExpressionLimits::default()
        },
    );

    let parsed = parser.parse(&source, Span::new(0, source.text().len()));

    assert!(parsed.resource_limited());
    assert!(parsed.expression().is_some());
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-EXPR-013")
    );
}

#[test]
fn limits_left_associative_and_postfix_tree_depth() {
    let grammar = arithmetic();
    let source = SourceFile::new("expression", "a.b.c.d");
    let parser = ExpressionParser::with_limits(
        &grammar,
        ExpressionLimits {
            max_nesting_depth: 3,
            ..ExpressionLimits::default()
        },
    );

    let parsed = parser.parse(&source, Span::new(0, source.text().len()));

    assert!(parsed.resource_limited());
    assert!(parsed.expression().is_none());
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-EXPR-004")
    );
}

#[test]
fn configured_expression_depth_cannot_raise_the_hard_limit() {
    let grammar = arithmetic();
    let nested = format!("{}1{}", "(".repeat(140), ")".repeat(140));
    let source = SourceFile::new("nested", nested);
    let parser = ExpressionParser::with_limits(
        &grammar,
        ExpressionLimits {
            max_nesting_depth: usize::MAX,
            ..ExpressionLimits::default()
        },
    );

    let parsed = parser.parse(&source, Span::new(0, source.text().len()));

    assert!(parsed.resource_limited());
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-EXPR-004")
    );
}

#[test]
fn rejects_duplicate_and_reserved_operator_declarations() {
    assert!(matches!(
        ExpressionGrammar::new([
            OperatorSpec::infix("+", 1, Associativity::Left),
            OperatorSpec::infix("+", 2, Associativity::Right),
        ]),
        Err(langmeld_expression::GrammarError::DuplicateOperator { .. })
    ));
    assert!(matches!(
        ExpressionGrammar::new([OperatorSpec::infix(".", 1, Associativity::Left)]),
        Err(langmeld_expression::GrammarError::InvalidOperator(_))
    ));
}
