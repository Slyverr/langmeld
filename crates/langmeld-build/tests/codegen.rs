use langmeld_build::{construct_type_name, generate_bindings};
use langmeld_language::{ConstructSpec, LanguageSpec, PatternPart};

#[test]
fn construct_type_name_matches_the_generated_view_name() {
    assert_eq!(
        construct_type_name("readHTTP-response").expect("the construct name maps to a Rust type"),
        "ReadHttpResponse"
    );
}

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
    assert!(generated.contains("pub const NAME: &'static str = \"function\";"));
    assert!(generated.contains("const CONSTRUCT: &'static str = Self::NAME;"));
    assert!(generated.contains(
        "pub fn name(&self) -> ::core::result::Result<::langmeld_language::CapturedIdentifier<'source>, ::langmeld_core::Diagnostic>"
    ));
    assert!(generated.contains("pub fn parameters(&self) -> ::core::result::Result<::core::option::Option<FunctionParameters<'source>>, ::langmeld_core::Diagnostic>"));
    assert!(generated.contains("pub fn additional_parameters(&self) -> ::core::result::Result<::std::vec::Vec<FunctionParametersAdditionalParameters<'source>>, ::langmeld_core::Diagnostic>"));
}

#[test]
fn generated_names_do_not_shadow_framework_or_prelude_types() {
    let names = [
        "diagnostic",
        "source_file",
        "span",
        "capture_group",
        "capture_group_kind",
        "capture_group_occurrence",
        "captured_field",
        "captured_identifier",
        "captured_text",
        "captured_value",
        "parsed_construct",
        "typed_construct",
        "option",
        "result",
        "vec",
    ];
    let mut constructs = names
        .iter()
        .map(|name| ConstructSpec::new(*name, [PatternPart::literal(*name)]))
        .collect::<Vec<_>>();
    constructs.push(ConstructSpec::new(
        "collection",
        [
            PatternPart::optional_named("maybe", [PatternPart::literal("maybe")]),
            PatternPart::literal("with"),
            PatternPart::one_or_more_named("items", [PatternPart::literal("item")]),
        ],
    ));
    let specification =
        LanguageSpec::new("Example", constructs).expect("all constructs have unique names");

    let generated = generate_bindings(&specification)
        .expect("user names may match imported Rust and Langmeld type names");

    for name in [
        "Diagnostic",
        "SourceFile",
        "Span",
        "CaptureGroup",
        "CaptureGroupKind",
        "CaptureGroupOccurrence",
        "CapturedField",
        "CapturedIdentifier",
        "CapturedText",
        "CapturedValue",
        "ParsedConstruct",
        "TypedConstruct",
        "Option",
        "Result",
        "Vec",
    ] {
        assert!(generated.contains(&format!("pub struct {name}<'source>")));
    }

    assert!(generated.contains("::langmeld_core::Diagnostic"));
    assert!(generated.contains("::langmeld_language::TypedConstruct"));
    assert!(generated.contains("::core::option::Option<"));
    assert!(generated.contains("::core::result::Result<"));
    assert!(generated.contains("::std::vec::Vec<"));
    assert!(!generated.contains("use langmeld_core::"));
    assert!(!generated.contains("use langmeld_language::"));
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
    assert!(error.to_string().contains("zero_or_more \"name\""));
    assert!(error.to_string().contains("one_or_more \"name\""));
    assert!(
        error
            .to_string()
            .contains("ParsedConstruct::capture_groups")
    );
}

#[test]
fn reports_unnamed_syntax_variants_with_the_dynamic_api_alternative() {
    let construct = ConstructSpec::new(
        "function",
        [PatternPart::literal("fn"), PatternPart::identifier("name")],
    )
    .variant([
        PatternPart::literal("function"),
        PatternPart::identifier("name"),
    ]);
    let specification = LanguageSpec::new("Example", [construct])
        .expect("the example language specification is valid");

    let error = generate_bindings(&specification)
        .expect_err("typed bindings need stable names for syntax variants");

    assert!(error.to_string().contains("syntax \"variant_name\""));
    assert!(error.to_string().contains("use the dynamic API"));
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
fn generates_accessors_named_new_without_colliding_with_view_construction() {
    let construct = ConstructSpec::new(
        "greet",
        [
            PatternPart::literal("hello"),
            PatternPart::identifier("new"),
        ],
    );
    let specification = LanguageSpec::new("Example", [construct])
        .expect("the example language specification is valid");

    let generated = generate_bindings(&specification)
        .expect("capture names may use ordinary Rust method names");

    assert!(
        generated.contains("pub fn new(&self) -> ::core::result::Result<::langmeld_language::CapturedIdentifier<'source>, ::langmeld_core::Diagnostic>")
    );
    assert!(!generated.contains("fn new(source:"));
    assert!(generated.contains("Ok(Self {"));
}

#[test]
fn reports_accessors_that_conflict_with_typed_view_construction() {
    let construct = ConstructSpec::new(
        "greet",
        [
            PatternPart::literal("hello"),
            PatternPart::identifier("from_construct"),
        ],
    );
    let specification = LanguageSpec::new("Example", [construct])
        .expect("the example language specification is valid");

    let error = generate_bindings(&specification)
        .expect_err("the accessor would shadow the typed-view constructor");

    assert!(
        error
            .to_string()
            .contains("capture or group named `from_construct`")
    );
    assert!(error.to_string().contains("use the dynamic API"));
}

#[test]
fn reports_group_names_that_conflict_with_typed_view_construction() {
    let construct = ConstructSpec::new(
        "greet",
        [
            PatternPart::literal("hello"),
            PatternPart::optional_named("from_construct", [PatternPart::literal("!")]),
        ],
    );
    let specification = LanguageSpec::new("Example", [construct])
        .expect("the example language specification is valid");

    let error = generate_bindings(&specification)
        .expect_err("the group accessor would shadow the typed-view constructor");

    assert!(
        error
            .to_string()
            .contains("top-level capture or group named")
    );
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
    assert!(generated.contains("pub const NAME: &'static str = \"value\";"));
    assert!(generated.contains("Declaration(ValueDeclaration<'source>)"));
    assert!(generated.contains("Assignment(ValueAssignment<'source>)"));
}
