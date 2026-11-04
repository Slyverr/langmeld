use langmeld_core::{
    ConstructBuilder, Diagnostic, ExpressionAssociativity, ExpressionAtomKind,
    ExpressionOperatorSpec, ExpressionSpec, FieldKind, FieldSpec, LanguageBuilder, LexicalSpec,
    SourceFile, Span,
};
use langmeld_expression::{Atom, CustomAtomToken, Expression};
use langmeld_language::parse_definition;
use langmeld_language::{
    ExpressionAtomParserRegistration, PreparedLanguage, SyntaxElement, SyntaxNode, SyntaxNodeRole,
    Value, generate_tooling,
};

fn parser() -> langmeld_language::ConfiguredLanguageParser {
    parser_with_lexical(None)
}

fn parser_with_lexical(
    lexical: Option<LexicalSpec>,
) -> langmeld_language::ConfiguredLanguageParser {
    let mut language = LanguageBuilder::new("Calc")
        .schema_version(if lexical.is_some() { 4 } else { 3 })
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
        );
    if let Some(lexical) = lexical {
        language = language.lexical(lexical);
    }
    let language = language
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
fn expression_syntax_tree_exposes_nested_expression_nodes_and_keeps_all_tokens() {
    let source = SourceFile::new(
        "program.calc",
        "program {\neval foo.bar(1)[2] + (2 * 3);\n}",
    );
    let result = parser().parse(&source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let mut roles = Vec::new();
    let mut token_indices = Vec::new();
    collect_syntax(&result.syntax_tree, &mut roles, &mut token_indices);
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::Expression)
            .count(),
        1
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionOperator)
            .count(),
        2
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionGroup)
            .count(),
        1
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionAtom)
            .count(),
        5
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionCall)
            .count(),
        1
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionMember)
            .count(),
        1
    );
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == SyntaxNodeRole::ExpressionIndex)
            .count(),
        1
    );
    assert_eq!(
        token_indices,
        (0..result.syntax_tokens.len()).collect::<Vec<_>>()
    );
}

#[test]
fn expression_tooling_reports_why_tree_sitter_is_unavailable() {
    let parser = parser();
    let language = parser.language().spec();
    let artifacts = generate_tooling(language).expect("baseline tooling still generates");
    assert!(
        !artifacts
            .keys()
            .any(|path| path.starts_with("tree-sitter/"))
    );

    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capabilities are JSON");
    assert_eq!(
        capabilities["artifacts"]["tree_sitter_grammar"]["status"],
        "unavailable"
    );
    assert_eq!(
        capabilities["artifacts"]["tree_sitter_grammar"]["reason"],
        "Tree-sitter export does not yet represent expression field grammar `arithmetic`"
    );
}

fn collect_syntax(
    node: &SyntaxNode,
    roles: &mut Vec<SyntaxNodeRole>,
    token_indices: &mut Vec<usize>,
) {
    roles.push(node.role);
    for element in &node.children {
        match element {
            SyntaxElement::Node(child) => collect_syntax(child, roles, token_indices),
            SyntaxElement::Token(index) => token_indices.push(*index),
        }
    }
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
        &result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Infix { .. })
    ));
}

#[test]
fn configured_comments_are_shared_with_expression_field_parsing() {
    let source = SourceFile::new(
        "program.calc",
        "program {\neval 1 /* inline */ + // line comment\n 2;\n}",
    );
    let lexical = LexicalSpec::new()
        .line_comment("//")
        .block_comment("/*", "*/");
    let result = parser_with_lexical(Some(lexical)).parse(&source);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(matches!(
        &result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Infix { operator, .. }) if operator == "+"
    ));
}

#[test]
fn declarative_lexical_policy_bootstraps_and_drives_expression_parsing() {
    let definition = SourceFile::new(
        "language.langmeld",
        r#"language Calc {
  version: "0.1.0"
  schema_version: 4
  root: program

  lexical {
    line_comment: "//"
    block_comment: "/*" "*/"
  }

  expression arithmetic {
    atom: number
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
        .expect("the built-in definition grammar parses lexical settings")
        .validate()
        .expect("the lexical policy is valid");
    let source = SourceFile::new(
        "program.calc",
        "program {\neval 1 /* block */ * 2 // line comment\n;\n}",
    );
    let result = PreparedLanguage::new(model)
        .expect("expression grammar compiles")
        .configure()
        .expect("language has no host callbacks")
        .parse(&source);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(matches!(
        &result.program.nodes[0].children[0].fields["value"].value,
        Value::Expression(Expression::Infix { operator, .. }) if operator == "*"
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
