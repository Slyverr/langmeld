use langmeld::core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, SpecError, SymbolSpec,
    ValidatedLanguageSpec,
};

/// Builds a Rust-authored language model for function declarations.
pub fn validated_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    let module = ConstructBuilder::new("module")
        .syntax_variant("root", "module {")
        .block(["function0", "function2"]);
    let function0 = ConstructBuilder::new("function0")
        .syntax_variant("signature", "fn {name}\\(\\) -> {return_type}")
        .field(FieldSpec::required("name", FieldKind::Identifier))
        .field(FieldSpec::required("return_type", FieldKind::Identifier))
        .symbol(SymbolSpec::new("function", "name"));
    let function2 = ConstructBuilder::new("function2")
        .syntax_variant(
            "signature",
            "fn {name}\\({left}: {left_type}, {right}: {right_type}\\) -> {return_type}",
        )
        .field(FieldSpec::required("name", FieldKind::Identifier))
        .field(FieldSpec::required("left", FieldKind::Identifier))
        .field(FieldSpec::required("left_type", FieldKind::Identifier))
        .field(FieldSpec::required("right", FieldKind::Identifier))
        .field(FieldSpec::required("right_type", FieldKind::Identifier))
        .field(FieldSpec::required("return_type", FieldKind::Identifier))
        .symbol(SymbolSpec::new("function", "name"));

    LanguageBuilder::new("Functions")
        .version("0.1.0")
        .root("module")
        .source_extension(".functions.lmprog")
        .construct(module)
        .construct(function0)
        .construct(function2)
        .build()
}
