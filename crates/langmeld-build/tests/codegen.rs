use langmeld_build::generate_bindings;
use langmeld_language::{ConstructSpec, LanguageSpec, PatternPart};

#[test]
fn generates_typed_fields_and_named_capture_groups() {
    let parameters = PatternPart::optional_named(
        "parameters",
        [
            PatternPart::literal("("),
            PatternPart::identifier("first_parameter"),
            PatternPart::zero_or_more_named(
                "additional_parameters",
                [
                    PatternPart::literal(","),
                    PatternPart::identifier("parameter"),
                ],
            ),
            PatternPart::literal(")"),
        ],
    );
    let function = ConstructSpec::new(
        "function",
        [
            PatternPart::literal("fn"),
            PatternPart::identifier("name"),
            parameters,
        ],
    );
    let specification = LanguageSpec::new("Example", [function])
        .expect("the example language specification is valid");

    let generated = generate_bindings(&specification)
        .expect("named groups can be represented by typed bindings");

    assert!(generated.contains("pub struct Function<'source>"));
    assert!(
        generated.contains("pub fn name(&self) -> Result<CapturedIdentifier<'source>, Diagnostic>")
    );
    assert!(generated.contains(
        "pub fn parameters(&self) -> Result<Option<FunctionParameters<'source>>, Diagnostic>"
    ));
    assert!(generated.contains("pub fn additional_parameters(&self) -> Result<Vec<FunctionParametersAdditionalParameters<'source>>, Diagnostic>"));
}

#[test]
fn reports_unnamed_groups_with_an_actionable_error() {
    let construct = ConstructSpec::new(
        "function",
        [
            PatternPart::literal("fn"),
            PatternPart::optional([PatternPart::literal("("), PatternPart::literal(")")]),
        ],
    );
    let specification = LanguageSpec::new("Example", [construct])
        .expect("the example language specification is valid");

    let error = generate_bindings(&specification)
        .expect_err("typed bindings need stable names for optional groups");

    assert!(
        error
            .to_string()
            .contains("unnamed optional or repeated group")
    );
    assert!(error.to_string().contains("optional \"name\""));
}

#[test]
fn reports_construct_names_that_collide_after_rust_conversion() {
    let first = ConstructSpec::new("some-value", [PatternPart::literal("first")]);
    let second = ConstructSpec::new("some_value", [PatternPart::literal("second")]);
    let specification = LanguageSpec::new("Example", [first, second])
        .expect("the example language specification is valid");

    let error = generate_bindings(&specification)
        .expect_err("construct names must map to distinct Rust types");

    assert!(error.to_string().contains("multiple language constructs"));
    assert!(error.to_string().contains("SomeValue"));
}

#[test]
fn generates_variant_specific_views_for_named_syntax_alternatives() {
    let value = ConstructSpec::new(
        "value",
        [PatternPart::literal("let"), PatternPart::identifier("name")],
    )
    .with_first_variant_name("declaration")
    .variant_named(
        "assignment",
        [PatternPart::literal("set"), PatternPart::identifier("name")],
    );
    let specification =
        LanguageSpec::new("Example", [value]).expect("the example language specification is valid");

    let generated = generate_bindings(&specification)
        .expect("named syntax alternatives can be represented by Rust enums");

    assert!(generated.contains("pub enum Value<'source>"));
    assert!(generated.contains("Declaration(ValueDeclaration<'source>)"));
    assert!(generated.contains("Assignment(ValueAssignment<'source>)"));
}
