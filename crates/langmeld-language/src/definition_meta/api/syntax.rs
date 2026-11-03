use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec};

pub(super) fn definitions() -> Vec<ConstructBuilder> {
    vec![
        ConstructBuilder::new("syntax_block")
            .syntax_variant("root", "syntax {")
            .block(["syntax_variant"]),
        ConstructBuilder::new("syntax_variant")
            .syntax_variant("named", "{id}: {pattern}")
            .syntax_variant("unnamed", "{pattern}")
            .field(FieldSpec::optional("id", FieldKind::Identifier))
            .field(FieldSpec::required("pattern", FieldKind::Text)),
        ConstructBuilder::new("syntax_list_decl")
            .syntax_variant("default", "syntax: \\[({patterns}[,])*\\]")
            .field(FieldSpec::optional(
                "patterns",
                FieldKind::list(FieldKind::Text),
            )),
        ConstructBuilder::new("block_declaration")
            .syntax_variant("default", "block: \\[({allowed}[,])*\\]")
            .field(FieldSpec::optional(
                "allowed",
                FieldKind::list(FieldKind::Identifier),
            )),
        ConstructBuilder::new("symbol_block")
            .syntax_variant("root", "symbol {")
            .block([
                "symbol_kind",
                "symbol_name",
                "symbol_scope",
                "symbol_scope_name",
                "symbol_scope_lookup",
            ]),
        ConstructBuilder::new("symbol_kind")
            .syntax_variant("default", "kind: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("symbol_name")
            .syntax_variant("default", "name: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("symbol_scope")
            .syntax_variant("default", "scope: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("symbol_scope_name")
            .syntax_variant("default", "scope_name: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("symbol_scope_lookup")
            .syntax_variant("default", "scope_lookup: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
    ]
}
