use langmeld::core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, SpecError, SymbolSpec,
    ValidatedLanguageSpec,
};

/// Builds a Rust-authored language model for function declarations.
pub fn validated_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    let module = ConstructBuilder::new("module")
        .syntax_variant("root", "module {")
        .block(["function"]);
    let function = ConstructBuilder::new("function")
        .syntax_variant(
            "signature",
            r"(fn|func) [{modifier}] {name}\(({parameters.name}: {parameters.type}[,])*\) -> {return_type}",
        )
        .field(FieldSpec::required("name", FieldKind::Identifier))
        .field(FieldSpec::optional(
            "modifier",
            FieldKind::Enum {
                values: vec!["unsafe".to_owned()],
            },
        ))
        .field(FieldSpec::optional(
            "parameters",
            FieldKind::list(FieldKind::Properties {
                name: None,
                fields: vec![
                    FieldSpec::required("name", FieldKind::Identifier),
                    FieldSpec::required("type", FieldKind::Identifier),
                ],
            }),
        ))
        .field(FieldSpec::required("return_type", FieldKind::Identifier))
        .symbol(SymbolSpec::new("function", "name"));

    LanguageBuilder::new("Functions")
        .version("0.1.0")
        .root("module")
        .source_extension(".functions.lmprog")
        .construct(module)
        .construct(function)
        .build()
}
