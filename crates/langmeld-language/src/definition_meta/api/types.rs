use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec};

pub(super) fn definitions() -> Vec<ConstructBuilder> {
    vec![
        ConstructBuilder::new("enum_decl")
            .syntax_variant("root", "enum {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block(["enum_value"]),
        ConstructBuilder::new("enum_value")
            .syntax_variant("default", "{value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        ConstructBuilder::new("record_type")
            .syntax_variant("root", "type {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block(["field", "typed_field", "property_use"]),
        ConstructBuilder::new("properties_bundle")
            .syntax_variant("root", "properties {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block(["field", "typed_field", "property_use"]),
        ConstructBuilder::new("property_use")
            .syntax_variant("default", "use {id}")
            .field(FieldSpec::required("id", FieldKind::Identifier)),
    ]
}
