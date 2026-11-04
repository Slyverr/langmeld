use langmeld_core::{FieldKind, FieldSpec, LanguageSpec, MapKeyKind};

#[test]
fn language_spec_round_trips_nested_types_and_flattened_field_kinds() {
    let spec: LanguageSpec = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "name": "Canvas DSL",
        "version": "0.1.0",
        "root": "scene",
        "constructs": [{
            "id": "scene",
            "syntax": ["scene {name}"],
            "fields": [{ "name": "name", "kind": "identifier" }, {
                "name": "properties",
                "kind": "map",
                "key": "text",
                "value": { "kind": "list", "item": { "kind": "number" } }
            }]
        }]
    }))
    .expect("valid language spec deserializes");

    let properties = &spec.constructs[0].fields[1];
    assert_eq!(properties.name, "properties");
    assert!(matches!(
        properties.kind,
        FieldKind::Map {
            key: MapKeyKind::Text,
            ..
        }
    ));
    assert!(spec.constructs[0].fields[0].required);

    let serialized = serde_json::to_value(&spec).expect("language spec serializes");
    assert_eq!(
        serialized["constructs"][0]["fields"][0]["kind"],
        "identifier"
    );
    assert_eq!(serialized["constructs"][0]["fields"][1]["key"], "text");
    assert_eq!(
        serde_json::from_value::<LanguageSpec>(serialized).expect("round trip deserializes"),
        spec
    );
}

#[test]
fn optional_schema_collections_default_and_are_omitted_when_empty() {
    let spec: LanguageSpec = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "name": "Tiny",
        "version": "0.1.0",
        "root": "program",
        "constructs": [{ "id": "program", "syntax": ["program"] }]
    }))
    .expect("omitted optional collections deserialize");

    assert!(spec.categories.is_empty());
    assert!(spec.rules.is_empty());
    assert!(spec.enums.is_empty());
    assert!(spec.types.is_empty());
    assert!(spec.behaviors.is_empty());

    let serialized = serde_json::to_value(&spec).expect("language spec serializes");
    for key in ["categories", "rules", "enums", "types", "behaviors"] {
        assert!(serialized.get(key).is_none(), "{key} should be omitted");
    }
}

#[test]
fn schema_v1_block_layout_defaults_round_trip_without_changing_wire_shape() {
    let spec: LanguageSpec = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "name": "Legacy",
        "version": "0.1.0",
        "root": "program",
        "constructs": [{
            "id": "program",
            "syntax": ["program {"],
            "block": { "allowed": ["statement"] }
        }, {
            "id": "statement",
            "syntax": ["statement"]
        }]
    }))
    .expect("schema-v1 block deserializes without layout fields");

    let block = spec.constructs[0].block.as_ref().expect("block is present");
    assert_eq!(block.open, "{");
    assert_eq!(block.close, "}");
    assert_eq!(block.separators.len(), 2);
    assert_eq!(
        block.separator_policy,
        langmeld_core::BlockSeparatorPolicy::Optional
    );

    let serialized = serde_json::to_value(&spec).expect("language spec serializes");
    assert_eq!(
        serialized["constructs"][0]["block"],
        serde_json::json!({
            "allowed": ["statement"]
        })
    );
}

#[test]
fn schema_v2_block_layout_round_trips_delimiters_and_separator_policy() {
    let spec: LanguageSpec = serde_json::from_value(serde_json::json!({
        "schema_version": 2,
        "name": "Configured",
        "version": "0.1.0",
        "root": "program",
        "constructs": [{
            "id": "program",
            "syntax": ["program"],
            "block": {
                "allowed": ["statement"],
                "open": "begin",
                "close": "end",
                "separators": [{"kind": "literal", "value": ";"}],
                "separator_policy": "required"
            }
        }, {
            "id": "statement",
            "syntax": ["statement"]
        }]
    }))
    .expect("schema-v2 configured block deserializes");

    assert_eq!(
        serde_json::from_value::<LanguageSpec>(
            serde_json::to_value(&spec).expect("language spec serializes")
        )
        .expect("round trip deserializes"),
        spec
    );
}

#[test]
fn closed_schema_rejects_unknown_language_and_constraint_properties() {
    let invalid_language = serde_json::json!({
        "schema_version": 1,
        "name": "Tiny",
        "version": "0.1.0",
        "root": "program",
        "extra": true,
        "constructs": []
    });
    assert!(serde_json::from_value::<LanguageSpec>(invalid_language).is_err());

    let invalid_constraints = serde_json::json!({
        "name": "value",
        "kind": "number",
        "constraints": { "surprise": true }
    });
    assert!(serde_json::from_value::<FieldSpec>(invalid_constraints).is_err());
}

#[test]
fn flattened_field_kind_rejects_unknown_sibling_properties() {
    let invalid = serde_json::json!({
        "name": "value",
        "kind": "number",
        "surprise": true
    });

    assert!(serde_json::from_value::<FieldSpec>(invalid).is_err());
}

#[test]
fn flattened_field_kind_rejects_properties_from_other_variants() {
    let invalid = serde_json::json!({
        "name": "value",
        "kind": "identifier",
        "values": ["unexpected"]
    });

    assert!(serde_json::from_value::<FieldSpec>(invalid).is_err());
}

#[test]
fn nested_field_kinds_still_reject_unknown_properties() {
    let invalid = serde_json::json!({
        "name": "items",
        "kind": "list",
        "item": { "kind": "number", "surprise": true }
    });

    let result = serde_json::from_value::<FieldSpec>(invalid);
    assert!(result.is_err(), "unexpectedly accepted {result:?}");
}

#[test]
fn standalone_field_kinds_reject_unknown_properties() {
    let invalid = serde_json::json!({
        "kind": "number",
        "surprise": true
    });

    assert!(serde_json::from_value::<FieldKind>(invalid).is_err());
}

#[test]
fn flattened_field_kind_roundtrips_every_variant() {
    let inputs = [
        serde_json::json!({"name": "identifier", "kind": "identifier"}),
        serde_json::json!({"name": "number", "kind": "number"}),
        serde_json::json!({"name": "text", "kind": "text"}),
        serde_json::json!({"name": "choice", "kind": "enum", "values": ["a", "b"]}),
        serde_json::json!({"name": "named", "kind": "named_enum", "enum_name": "Mode", "values": ["a"]}),
        serde_json::json!({"name": "reference", "kind": "reference", "target": "node"}),
        serde_json::json!({"name": "properties", "kind": "properties", "record": "Point", "fields": [{"name": "x", "kind": "number"}]}),
        serde_json::json!({"name": "list", "kind": "list", "item": {"kind": "text"}}),
        serde_json::json!({"name": "map", "kind": "map", "key": "text", "value": {"kind": "number"}}),
    ];

    for input in inputs {
        let field: FieldSpec = serde_json::from_value(input).expect("field kind deserializes");
        let serialized = serde_json::to_value(&field).expect("field kind serializes");
        let decoded: FieldSpec =
            serde_json::from_value(serialized).expect("serialized field kind deserializes");
        assert_eq!(decoded, field);
    }
}
