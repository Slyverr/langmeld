use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec};

pub(super) fn definitions() -> Vec<ConstructBuilder> {
    vec![
        ConstructBuilder::new("language")
            .syntax_variant("root", "language {name} {")
            .syntax_variant("constructs", "constructs {")
            .syntax_variant("expressions", "expressions {")
            .syntax_variant("types", "types {")
            .field(FieldSpec::optional("name", FieldKind::Identifier))
            .block([
                "language_version",
                "language_root",
                "schema_version_decl",
                "source_extension_decl",
                "import_decl",
                "lexical_decl",
                "category_decl",
                "behavior_decl",
                "rule_decl",
                "construct",
                "enum_decl",
                "record_type",
                "properties_bundle",
                "expression_decl",
            ]),
        ConstructBuilder::new("language_version")
            .syntax_variant("default", "version: {value}")
            .field(FieldSpec::required("value", FieldKind::Text)),
        ConstructBuilder::new("language_root")
            .syntax_variant("default", "root: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("schema_version_decl")
            .syntax_variant("default", "schema_version: {value}")
            .field(FieldSpec::required("value", FieldKind::Number)),
        ConstructBuilder::new("source_extension_decl")
            .syntax_variant("default", "source_extension: {value}")
            .field(FieldSpec::required("value", FieldKind::Text)),
        ConstructBuilder::new("import_decl")
            .syntax_variant("default", "import: {path}")
            .field(FieldSpec::required("path", FieldKind::Text)),
        ConstructBuilder::new("lexical_decl")
            .syntax_variant("root", "lexical {")
            .block(["line_comment_decl", "block_comment_decl"]),
        ConstructBuilder::new("line_comment_decl")
            .syntax_variant("default", "line_comment: {marker}")
            .field(FieldSpec::required("marker", FieldKind::Text)),
        ConstructBuilder::new("block_comment_decl")
            .syntax_variant("default", "block_comment: {open} {close}")
            .field(FieldSpec::required("open", FieldKind::Text))
            .field(FieldSpec::required("close", FieldKind::Text)),
        ConstructBuilder::new("category_decl")
            .syntax_variant("root", "category {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block(std::iter::empty::<&str>()),
        ConstructBuilder::new("behavior_decl")
            .syntax_variant("default", "behavior {id}: {stage}")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .field(FieldSpec::required("stage", FieldKind::Identifier)),
        ConstructBuilder::new("rule_decl")
            .syntax_variant("simple", "rule {id}: {pattern}")
            .syntax_variant(
                "parameterized",
                "rule {id}\\(({parameters}[,])*\\): {pattern}",
            )
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .field(FieldSpec::optional(
                "parameters",
                FieldKind::list(FieldKind::Identifier),
            ))
            .field(FieldSpec::required("pattern", FieldKind::Text)),
    ]
}
