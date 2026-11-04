// Initial Rust representation of the canonical Langmeld language model.
// Edit this model directly; Langmeld does not overwrite authored host source.
use langmeld::core as langmeld_core;

/// Reconstructs this module's canonical language definition.
pub fn language_spec() -> langmeld_core::LanguageSpec {
    langmeld_core::LanguageSpec {
        schema_version: 1,
        name: "Brief".to_owned(),
        version: "0.1.0".to_owned(),
        root: "program".to_owned(),
        source_extensions: vec![".brief.lmprog".to_owned()],
        lexical: None,
        categories: vec![langmeld_core::CategorySpec {
            id: "program_statement".to_owned(),
        }],
        rules: vec![],
        enums: vec![],
        expressions: vec![],
        types: vec![],
        behaviors: vec![],
        constructs: vec![
            langmeld_core::ConstructSpec {
                id: "program".to_owned(),
                implements: vec![],
                custom_parser: None,
                syntax: vec!["program {".to_owned()],
                syntax_ids: vec!["default".to_owned()],
                fields: vec![],
                block: Some(langmeld_core::BlockSpec {
                    allowed: vec!["program_statement".to_owned()],
                    ..langmeld_core::BlockSpec::default()
                }),
                symbol: None,
            },
            langmeld_core::ConstructSpec {
                id: "statement".to_owned(),
                implements: vec!["program_statement".to_owned()],
                custom_parser: None,
                syntax: vec!["print {message}".to_owned()],
                syntax_ids: vec!["default".to_owned()],
                fields: vec![langmeld_core::FieldSpec {
                    name: "message".to_owned(),
                    kind: langmeld_core::FieldKind::Text,
                    required: true,
                    default: None,
                    constraints: langmeld_core::FieldConstraints {
                        min: None,
                        max: None,
                        min_length: None,
                        max_length: None,
                        min_items: None,
                        max_items: None,
                    },
                }],
                block: None,
                symbol: None,
            },
        ],
    }
}

/// Validates the embedded language definition.
pub fn validated_language()
-> Result<langmeld_core::ValidatedLanguageSpec, Vec<langmeld_core::SpecError>> {
    language_spec().validate()
}
