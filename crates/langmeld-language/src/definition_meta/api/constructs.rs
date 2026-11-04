use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec};

const FIELD_CONSTRAINTS: &str = "[ {requirement}][ min {min}][ max {max}][ min_length {min_length}][ max_length {max_length}][ min_items {min_items}][ max_items {max_items}]";
const COLLECTION_CONSTRAINTS: &str =
    "[ {requirement}][ min_items {min_items}][ max_items {max_items}]";
const FIELD_TYPE_CHILDREN: [&str; 5] = [
    "field_type_scalar",
    "field_type_enum",
    "field_type_reference",
    "field_type_list",
    "field_type_map",
];

fn field_pattern(prefix: &str, default: Option<&str>) -> String {
    let default = default.map_or_else(String::new, |value| format!(" = {{{value}}}"));
    format!("{prefix}{FIELD_CONSTRAINTS}{default}")
}

fn collection_pattern(prefix: &str) -> String {
    format!("{prefix}{COLLECTION_CONSTRAINTS}")
}

pub(super) fn definitions() -> Vec<ConstructBuilder> {
    vec![
        ConstructBuilder::new("construct")
            .syntax_variant("root", "construct {id} {")
            .field(FieldSpec::required("id", FieldKind::Identifier))
            .block([
                "fields_block",
                "syntax_block",
                "syntax_list_decl",
                "block_declaration",
                "configured_block_declaration",
                "symbol_block",
                "implements_declaration",
                "custom_parser_decl",
            ]),
        ConstructBuilder::new("custom_parser_decl")
            .syntax_variant("default", "custom_parser: {id}")
            .field(FieldSpec::required("id", FieldKind::Identifier)),
        ConstructBuilder::new("implements_declaration")
            .syntax_variant("single", "implements: {category}")
            .syntax_variant("list", "implements: \\[({categories}[,])*\\]")
            .field(FieldSpec::optional("category", FieldKind::Identifier))
            .field(FieldSpec::optional(
                "categories",
                FieldKind::list(FieldKind::Identifier),
            )),
        ConstructBuilder::new("fields_block")
            .syntax_variant("root", "fields {")
            .block(["field", "typed_field", "property_use"]),
        ConstructBuilder::new("typed_field")
            .syntax_variant(
                "recursive",
                "field {name} {requirement}[ min {min}][ max {max}][ min_length {min_length}][ max_length {max_length}][ min_items {min_items}][ max_items {max_items}] {",
            )
            .field(FieldSpec::required("name", FieldKind::Identifier))
            .field(FieldSpec::required(
                "requirement",
                FieldKind::enumeration(["required", "optional"]),
            ))
            .field(FieldSpec::optional("min", FieldKind::Number))
            .field(FieldSpec::optional("max", FieldKind::Number))
            .field(FieldSpec::optional("min_length", FieldKind::Number))
            .field(FieldSpec::optional("max_length", FieldKind::Number))
            .field(FieldSpec::optional("min_items", FieldKind::Number))
            .field(FieldSpec::optional("max_items", FieldKind::Number))
            .block([
                "field_type_scalar",
                "field_type_enum",
                "field_type_reference",
                "field_type_list",
                "field_type_map",
                "field_default_text",
                "field_default_number",
                "field_default_identifier",
            ]),
        ConstructBuilder::new("field_type_scalar")
            .syntax_variant("scalar", "scalar: {kind}")
            .field(FieldSpec::required("kind", FieldKind::Identifier)),
        ConstructBuilder::new("field_type_enum")
            .syntax_variant("enum", "enum: \\[({values}[,])*\\]")
            .field(FieldSpec::optional(
                "values",
                FieldKind::list(FieldKind::Identifier),
            )),
        ConstructBuilder::new("field_type_reference")
            .syntax_variant("reference", "reference: {item_kind}")
            .field(FieldSpec::required("item_kind", FieldKind::Identifier)),
        ConstructBuilder::new("field_type_list")
            .syntax_variant("list", "list {")
            .block(FIELD_TYPE_CHILDREN),
        ConstructBuilder::new("field_type_map")
            .syntax_variant("map", "map<{key_kind}> {")
            .field(FieldSpec::required(
                "key_kind",
                FieldKind::enumeration(["identifier", "string"]),
            ))
            .block(FIELD_TYPE_CHILDREN),
        ConstructBuilder::new("field_default_text")
            .syntax_variant("text", "default_text: {value}")
            .field(FieldSpec::required("value", FieldKind::Text)),
        ConstructBuilder::new("field_default_number")
            .syntax_variant("number", "default_number: {value}")
            .field(FieldSpec::required("value", FieldKind::Number)),
        ConstructBuilder::new("field_default_identifier")
            .syntax_variant("identifier", "default_identifier: {value}")
            .field(FieldSpec::required("value", FieldKind::Identifier)),
        field_definition(),
    ]
}

fn field_definition() -> ConstructBuilder {
    ConstructBuilder::new("field")
        .syntax_variant(
            "inline_enum",
            field_pattern("{name}: enum {enum_values}", None),
        )
        .syntax_variant(
            "inline_enum_default",
            field_pattern("{name}: enum {enum_values}", Some("default_identifier")),
        )
        .syntax_variant("reference", collection_pattern("{name}: ref<{item_kind}>"))
        .syntax_variant(
            "reference_keyword",
            collection_pattern("{name}: reference<{item_kind}>"),
        )
        .syntax_variant("list", field_pattern("{name}: list<{item_kind}>", None))
        .syntax_variant(
            "list_reference",
            collection_pattern("{name}: list<ref<{item_kind}>>"),
        )
        .syntax_variant(
            "list_reference_keyword",
            collection_pattern("{name}: list<reference<{item_kind}>>"),
        )
        .syntax_variant(
            "list_inline_enum",
            field_pattern("{name}: list<enum {enum_values}>", None),
        )
        .syntax_variant(
            "list_properties",
            collection_pattern("{name}: list<properties {item_kind}>"),
        )
        .syntax_variant(
            "list_named_properties",
            collection_pattern("{name}: list<properties<{item_kind}>>"),
        )
        .syntax_variant(
            "nested_list",
            collection_pattern("{name}: list<list<{item_kind}>>"),
        )
        .syntax_variant(
            "map",
            collection_pattern("{name}: map<{key_kind}, {item_kind}>"),
        )
        .syntax_variant(
            "map_reference",
            collection_pattern("{name}: map<{key_kind}, ref<{item_kind}>>"),
        )
        .syntax_variant(
            "map_list",
            collection_pattern("{name}: map<{key_kind}, list<{item_kind}>>"),
        )
        .syntax_variant(
            "list_map",
            collection_pattern("{name}: list<map<{key_kind}, {item_kind}>>"),
        )
        .syntax_variant(
            "properties",
            collection_pattern("{name}: properties {item_kind}"),
        )
        .syntax_variant(
            "named_properties",
            collection_pattern("{name}: properties<{item_kind}>"),
        )
        .syntax_variant("scalar", field_pattern("{name}: {kind}", None))
        .syntax_variant(
            "scalar_text_default",
            field_pattern("{name}: {kind}", Some("default_text")),
        )
        .syntax_variant(
            "scalar_number_default",
            field_pattern("{name}: {kind}", Some("default_number")),
        )
        .syntax_variant(
            "scalar_identifier_default",
            field_pattern("{name}: {kind}", Some("default_identifier")),
        )
        .syntax_variant(
            "expression",
            "{name}: expression<{grammar}> until {terminator}[ {requirement}]",
        )
        .field(FieldSpec::required("name", FieldKind::Identifier))
        .field(FieldSpec::optional("kind", FieldKind::Identifier))
        .field(FieldSpec::optional("grammar", FieldKind::Identifier))
        .field(FieldSpec::optional("terminator", FieldKind::Text))
        .field(FieldSpec::optional("item_kind", FieldKind::Identifier))
        .field(FieldSpec::optional("key_kind", FieldKind::Identifier))
        .field(FieldSpec::optional(
            "enum_values",
            FieldKind::list(FieldKind::Identifier),
        ))
        .field(FieldSpec::optional(
            "requirement",
            FieldKind::enumeration(["required", "optional"]),
        ))
        .field(FieldSpec::optional("min", FieldKind::Number))
        .field(FieldSpec::optional("max", FieldKind::Number))
        .field(FieldSpec::optional("min_length", FieldKind::Number))
        .field(FieldSpec::optional("max_length", FieldKind::Number))
        .field(FieldSpec::optional("min_items", FieldKind::Number))
        .field(FieldSpec::optional("max_items", FieldKind::Number))
        .field(FieldSpec::optional("default_text", FieldKind::Text))
        .field(FieldSpec::optional("default_number", FieldKind::Number))
        .field(FieldSpec::optional(
            "default_identifier",
            FieldKind::Identifier,
        ))
}
