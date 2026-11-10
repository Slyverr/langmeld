use langmeld_core::{SourceFile, Span};
use langmeld_expression::{Associativity, ExpressionGrammar, ExpressionParser, OperatorSpec};
use langmeld_language::{CapturedValue, parse, parse_language_definition};

fn main() {
    let definition_source = SourceFile::new(
        "language.lm",
        r#"language "Mini" {
            construct "let" {
                syntax {
                    literal "let";
                    identifier "name";
                    literal "=";
                    integer "value";
                    literal ";";
                }
            }
        }"#,
    );
    let definition = parse_language_definition(&definition_source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.expect("validated language definition");

    let program = SourceFile::new("main.mini", "let answer = 42;");
    let parsed = parse(&program, &language);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );

    let operators = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .expect("valid operator declarations");
    let expression_source = SourceFile::new("value.expr", "20 + 4 * 2");
    let expression = ExpressionParser::new(&operators).parse(
        &expression_source,
        Span::new(0, expression_source.text.len()),
    );
    assert!(expression.diagnostics.is_empty(), "{:?}", expression.diagnostics);
    assert!(expression.expression.is_some());
    println!("standalone Langmeld consumer passed");
}
