use langmeld::core::{ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, SourceFile, Span};
use langmeld::expression::{ExpressionGrammar, OperatorSpec, parse_expression};
use langmeld::language::{FormatOptions, PreparedLanguage, format_source};

#[test]
fn consumer_uses_core_expression_and_language_through_the_facade() {
    let source = SourceFile::new("sample.demo", "greeting {\nhello user\n}");
    let language = LanguageBuilder::new("Greeting")
        .version("0.1.0")
        .root("greeting")
        .construct(
            ConstructBuilder::new("greeting")
                .syntax("greeting {")
                .block(["message"]),
        )
        .construct(
            ConstructBuilder::new("message")
                .syntax("hello {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier)),
        )
        .build()
        .expect("language schema builds");
    let parser = PreparedLanguage::new(language.clone())
        .expect("language grammar prepares")
        .with_custom_parsers([])
        .expect("declarative parser configures");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.program.nodes[0].children[0].construct, "message");
    assert_eq!(
        format_source(&language, &source, FormatOptions::default()).expect("valid program formats"),
        "greeting {\n  hello user\n}"
    );

    let expression_source = SourceFile::new("expression.demo", "1 + 2");
    let grammar =
        ExpressionGrammar::new([OperatorSpec::infix("+", 1)]).expect("operator table validates");
    let expression = parse_expression(
        &expression_source,
        Span::new(0, expression_source.text.len()),
        &grammar,
    )
    .expect("expression parses");
    assert_eq!(
        expression.span(),
        Span::new(0, expression_source.text.len())
    );
}
