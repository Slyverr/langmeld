use langmeld_core::{
    ConstructBuilder, Diagnostic, ExpressionAssociativity, ExpressionAtomKind,
    ExpressionOperatorSpec, ExpressionSpec, FieldKind, FieldSpec, LanguageBuilder, SourceFile,
    Span,
};
use langmeld_expression::{Atom, CustomAtomToken, Expression};
use langmeld_language::parse_definition;
use langmeld_language::{ExpressionAtomParserRegistration, PreparedLanguage, Value};

fn parser() -> langmeld_language::ConfiguredLanguageParser {
    let language = LanguageBuilder::new("Calc")
        .schema_version(3)
        .version("0.1.0")
        .root("program")
        .expression(
            ExpressionSpec::new("arithmetic")
                .atom(ExpressionAtomKind::Number)
                .atom(ExpressionAtomKind::Identifier)
                .operator(ExpressionOperatorSpec::infix(
                    "+",
                    10,
                    ExpressionAssociativity::Left,
                ))
                .operator(ExpressionOperatorSpec::infix(
                    "*",
                    20,
                    ExpressionAssociativity::Left,
                )),
        )
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"]),
        )
        .construct(
            ConstructBuilder::new("statement")
                .syntax("eval {value};")
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("arithmetic", ";"),
                )),
        )
        .build()
        .expect("expression language definition is valid");
    PreparedLanguage::new(language)
        .expect("expression grammar compiles")
        .configure()
        .expect("language has no custom parser requirements")
}

#[test]
fn parses_expression_field_to_structured_ast_and_stops_at_its_terminator() {
    let source = SourceFile::new("program.calc", "program {\neval 1 + 2 * 3;\n}");
    let result = parser().parse(&source);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let captured = &result.program.nodes[0].children[0].fields["value"];
    let expression_start = source.text.find("1 +").expect("expression exists");
    let expression_end = source.text.find(';').expect("terminator exists");
    assert_eq!(captured.span, Span::new(expression_start, expression_end));
    let Value::Expression(expression) = &captured.value else {
        panic!("expression field should retain its parsed tree");
    };
    assert!(
        matches!(expression, Expression::Infix { operator, right, .. }
        if operator == "+" && matches!(right.as_ref(), Expression::Infix { operator, .. } if operator == "*"))
    );
}

#[test]
fn expression_parser_reports_missing_boundary_and_incomplete_expression() {
    for text in ["program {\neval 1 + 2\n}", "program {\neval 1 + ;\n}"] {
        let result = parser().parse(&SourceFile::new("program.calc", text));
        assert!(!result.diagnostics.is_empty(), "{text} should be diagnosed");
    }
}

#[test]
fn expression_comments_do_not_terminate_expression_capture() {
    let source = SourceFile::new("program.calc", "program {\neval 1 + # comment\n 2;\n}");
    let result = parser().parse(&source);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(matches!(
        result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Infix { .. })
    ));
}

#[test]
fn declarative_definition_compiles_and_parses_expression_fields() {
    let definition = SourceFile::new(
        "language.langmeld",
        r#"language Calc {
  version: "0.1.0"
  schema_version: 3
  root: program

  expression arithmetic {
    atom: number
    atom: identifier
    operator "+" infix 10 left
    operator "*" infix 20 left
  }

  construct program {
    syntax { root: "program" }
    block: [statement]
  }

  construct statement {
    syntax { default: "eval {value};" }
    fields { value: expression<arithmetic> until ";" required }
  }
}
"#,
    );
    let model = parse_definition(&definition)
        .expect("the built-in .langmeld grammar parses expression declarations")
        .validate()
        .expect("the declarative expression grammar validates");
    let prepared = PreparedLanguage::new(model).expect("expression grammar compiles");
    let program = SourceFile::new("program.calc", "program {\neval 1 + 2 * 3;\n}");
    let result = prepared
        .configure()
        .expect("declarative language has no custom callbacks")
        .parse(&program);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(matches!(
        result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Infix { .. })
    ));
}

#[test]
fn declarative_custom_atoms_require_and_use_a_registered_host_callback() {
    let definition = SourceFile::new(
        "language.langmeld",
        r#"language Canvas {
  version: "0.1.0"
  schema_version: 3
  root: program

  expression object_reference {
    host_atom_parser: canvas_reference
  }

  construct program {
    syntax { root: "program" }
    block: [evaluate]
  }

  construct evaluate {
    syntax { default: "evaluate {value};" }
    fields { value: expression<object_reference> until ";" required }
  }
}
"#,
    );
    let model = parse_definition(&definition)
        .expect("host atom declaration uses the built-in definition grammar")
        .validate()
        .expect("a host atom callback permits an empty built-in atom set");
    let prepared = PreparedLanguage::new(model.clone()).expect("host grammar compiles");
    let missing = prepared
        .clone()
        .configure()
        .expect_err("required host atom parser is checked before source parsing");
    assert!(missing.to_string().contains("canvas_reference"));

    let callback = ExpressionAtomParserRegistration::new(
        "canvas_reference",
        |source: &SourceFile, range: Span, offset: usize| {
            let Some(fragment) = source.text.get(offset..range.end) else {
                return Err(Diagnostic::error(
                    "TEST001",
                    "callback offset is outside source",
                    Span::new(offset, offset),
                ));
            };
            let Some(body) = fragment.strip_prefix('@') else {
                return Ok(None);
            };
            let length = body
                .char_indices()
                .take_while(|(_, character)| character.is_alphanumeric() || *character == '_')
                .map(|(index, character)| index + character.len_utf8())
                .last()
                .unwrap_or(0);
            if length == 0 {
                return Ok(None);
            }
            let end = offset + 1 + length;
            Ok(Some(CustomAtomToken {
                kind: "canvas_object".to_owned(),
                text: source.text[offset..end].to_owned(),
                span: Span::new(offset, end),
            }))
        },
    );
    let parser = prepared
        .with_custom_parsers_and_expression_atom_parsers([], [callback])
        .expect("declared host atom parser binds successfully");
    let result = parser.parse(&SourceFile::new(
        "program.canvas",
        "program {\nevaluate @mesh;\n}",
    ));

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(matches!(
        &result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Atom(Atom::Custom { kind, text }, _))
            if kind == "canvas_object" && text == "@mesh"
    ));
}
