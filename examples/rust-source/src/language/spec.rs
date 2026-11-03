use langmeld::core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, SpecError, SymbolSpec,
    ValidatedLanguageSpec,
};

/// Builds a language model for indexing top-level Rust function declarations.
pub fn validated_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    let file = ConstructBuilder::new("rust_file")
        .custom_parser("rust_source")
        .block(["function"]);
    let function = ConstructBuilder::new("function")
        .field(FieldSpec::required("name", FieldKind::Identifier))
        .field(FieldSpec::required("signature", FieldKind::Text))
        .symbol(SymbolSpec::new("function", "name"));

    LanguageBuilder::new("RustSource")
        .version("0.1.0")
        .root("rust_file")
        .source_extension(".rs")
        .construct(file)
        .construct(function)
        .build()
}
