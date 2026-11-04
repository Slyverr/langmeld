use langmeld_core::{
    BehaviorStage, ConstructBuilder, ExpressionAssociativity, ExpressionAtomKind,
    ExpressionOperatorSpec, ExpressionSpec, FieldKind, FieldSpec, LanguageBuilder, MapKeyKind,
    NamedRecord, ScopeLookup, ScopeSpec, SourceFile, SymbolSpec,
};
use langmeld_expression::Expression;
use langmeld_language::generate_rust_ast;

extern crate self as langmeld;

pub mod core {
    pub use langmeld_core::*;
}

pub mod language {
    pub use langmeld_language::*;
}

pub mod expression {
    pub use langmeld_expression::*;
}

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "fixtures/generated_ast.rs"]
mod generated;

fn language() -> langmeld_core::ValidatedLanguageSpec {
    let properties = FieldKind::Properties {
        name: None,
        fields: vec![
            FieldSpec::required("title", FieldKind::Text),
            FieldSpec::required(
                "mode",
                FieldKind::named_enumeration("Mode", ["fast", "safe"]),
            ),
            FieldSpec::required("tags", FieldKind::list(FieldKind::Text)),
            FieldSpec::required(
                "attrs",
                FieldKind::map(MapKeyKind::Identifier, FieldKind::Text),
            ),
        ],
    };

    LanguageBuilder::new("GeneratedConsumer")
        .schema_version(3)
        .version("0.1.0")
        .root("box")
        .source_extensions(["lm", "lmd"])
        .category("statement")
        .rule("item_pattern", "item {name} config {config}")
        .named_enum("Mode", ["fast", "safe"])
        .expression(
            ExpressionSpec::new("arithmetic")
                .atom(ExpressionAtomKind::Number)
                .atom(ExpressionAtomKind::Identifier)
                .operator(ExpressionOperatorSpec::infix(
                    "+",
                    10,
                    ExpressionAssociativity::Left,
                )),
        )
        .record_type(NamedRecord::new(
            "Coordinates",
            [
                FieldSpec::required("x", FieldKind::Number),
                FieldSpec::required("y", FieldKind::Number),
            ],
        ))
        .behavior("semantic-check", BehaviorStage::SemanticAnalysis)
        .behavior("symbol-check", BehaviorStage::SymbolResolution)
        .behavior("type-check", BehaviorStage::WorkspaceTypeAnalysis)
        .construct(
            ConstructBuilder::new("box")
                .syntax_variant("root", "box {namespace}")
                .field(FieldSpec::required("namespace", FieldKind::Identifier))
                .symbol(SymbolSpec::new("namespace", "namespace"))
                .block(["statement"]),
        )
        .construct(
            ConstructBuilder::new("item")
                .implements(["statement"])
                .syntax_variant("item", "@item_pattern")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("config", properties))
                .symbol(SymbolSpec::new("item", "name").with_scope(
                    ScopeSpec::new("box", "namespace").lookup(ScopeLookup::LexicalParents),
                )),
        )
        .construct(
            ConstructBuilder::new("evaluate")
                .implements(["statement"])
                .syntax_variant("expression", "evaluate {result};")
                .field(FieldSpec::required(
                    "result",
                    FieldKind::expression("arithmetic", ";"),
                )),
        )
        .build()
        .expect("fixture language validates")
}

#[test]
fn generated_module_reconstructs_the_schema_and_converts_parsed_nodes() {
    let language = language();
    assert_eq!(generated::LANGUAGE_NAME, "GeneratedConsumer");
    assert_eq!(generated::LANGUAGE_VERSION, "0.1.0");
    assert_eq!(generated::language_spec(), *language.spec());
    assert!(generated::validated_language().is_ok());

    let source = SourceFile::new(
        "generated.lm",
        "box demo {\n  item sample config { title: \"hello\", mode: fast, tags: [\"one\", \"two\"], attrs: {owner: \"me\"} }\n  evaluate 1 + 2;\n}\n",
    );
    let (roots, diagnostics) =
        generated::parse_source(&source).expect("generated parser accepts the fixture program");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].children.len(), 2);

    let generated::BoxChild::Item(item) = &roots[0].children[0] else {
        panic!("first child is the item fixture");
    };
    assert_eq!(item.name.value, "sample");
    assert_eq!(item.config.value.title.value, "hello");
    assert_eq!(item.config.value.mode.value, generated::Mode::Fast);
    assert_eq!(item.config.value.mode.value.as_str(), "fast");
    assert_eq!(item.config.value.tags.value.len(), 2);
    assert_eq!(item.config.value.tags.value[1].value, "two");
    assert_eq!(item.config.value.attrs.value["owner"].value, "me");
    let generated::BoxChild::Evaluate(evaluate) = &roots[0].children[1] else {
        panic!("second child is the expression fixture");
    };
    assert!(matches!(
        evaluate.result.value,
        Expression::Infix { ref operator, .. } if operator == "+"
    ));
}

#[test]
fn typed_conversion_rejects_unknown_variants_and_wrong_field_kinds() {
    let source = SourceFile::new(
        "generated.lm",
        "box demo { item sample config { title: \"hello\", mode: fast, tags: [], attrs: {} } }",
    );
    let spec = language();
    let parsed = langmeld_language::PreparedLanguage::new(spec)
        .expect("fixture patterns compile")
        .configure()
        .expect("fixture has no custom callbacks")
        .parse(&source);
    let item = &parsed.program.nodes[0].children[0];

    let mut wrong_variant = item.clone();
    wrong_variant.syntax_variant = "unknown".to_owned();
    assert!(matches!(
        generated::Item::try_from(&wrong_variant),
        Err(generated::ConversionError::InvalidSyntaxVariant { .. })
    ));

    let mut wrong_kind = item.clone();
    wrong_kind.fields.insert(
        "name".to_owned(),
        langmeld_language::CapturedValue::new(
            langmeld_language::Value::Number(4.0),
            langmeld_core::Span::new(0, 1),
        ),
    );
    assert!(matches!(
        generated::Item::try_from(&wrong_kind),
        Err(generated::ConversionError::WrongFieldKind("name"))
    ));
}

#[test]
fn generated_rust_artifact_matches_the_compiled_fixture() {
    let generated_source = generate_rust_ast(&language());
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/generated_ast.rs"
    );

    if std::env::var_os("LANGMELD_UPDATE_AST_FIXTURE").is_some() {
        std::fs::write(fixture_path, &generated_source).expect("generated AST fixture writes");
        return;
    }

    assert_eq!(
        generated_source.replace("\r\n", "\n"),
        include_str!("fixtures/generated_ast.rs").replace("\r\n", "\n")
    );
}
