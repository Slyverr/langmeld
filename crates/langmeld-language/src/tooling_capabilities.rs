use std::collections::{BTreeMap, BTreeSet};

use langmeld_core::{FieldKind, LanguageSpec, LexicalSpec, ValidatedLanguageSpec};
use serde::Serialize;

use crate::PreparedLanguage;

/// Describes how completely Langmeld can provide a tooling feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Langmeld can provide this feature from the validated language model.
    Supported,
    /// Langmeld provides a useful subset, with a documented limitation.
    Partial,
    /// A host-specific implementation can provide this feature.
    Customized,
    /// Langmeld cannot provide this feature for the current language model.
    Unavailable,
}

/// One generated artifact or language-service feature and its current support.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Capability {
    /// Current support level for the artifact or feature.
    pub status: CapabilityStatus,
    /// Explains a partial, customized, or unavailable capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Capability {
    /// Creates a capability provided by Langmeld without a known limitation.
    pub fn supported() -> Self {
        Self::new(CapabilityStatus::Supported, None)
    }

    /// Creates a capability with a limitation that consumers should surface.
    pub fn partial(reason: impl Into<String>) -> Self {
        Self::new(CapabilityStatus::Partial, Some(reason.into()))
    }

    /// Creates a capability that requires a host-provided implementation.
    pub fn customized(reason: impl Into<String>) -> Self {
        Self::new(CapabilityStatus::Customized, Some(reason.into()))
    }

    /// Creates a capability that cannot be provided for this language model.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::new(CapabilityStatus::Unavailable, Some(reason.into()))
    }

    fn new(status: CapabilityStatus, reason: Option<String>) -> Self {
        Self { status, reason }
    }
}

/// Capability summary emitted alongside generated language tooling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ToolingCapabilities {
    /// Version of this capability report's JSON schema.
    pub schema_version: u32,
    /// Language name from the validated language definition.
    pub language: String,
    /// Language version from the validated language definition.
    pub language_version: String,
    /// Root construct identifier.
    pub root: String,
    /// Source file suffixes declared by the language.
    pub source_extensions: Vec<String>,
    /// Generated artifacts and their support levels.
    pub artifacts: BTreeMap<String, Capability>,
    /// Language-service or host features and their support levels.
    pub features: BTreeMap<String, Capability>,
    /// Language facts used when generating generic editor configuration.
    pub editor_configuration_assumptions: EditorConfigurationAssumptions,
}

/// Language facts used to generate generic editor configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorConfigurationAssumptions {
    /// Configured line comment delimiter, if any.
    pub line_comment: Option<String>,
    /// Configured opening and closing block-comment delimiters, if any.
    pub block_comment: Option<[String; 2]>,
    /// Bracket pairs inferred from declared syntax and structured fields.
    pub bracket_pairs: BTreeSet<(String, String)>,
    /// Explains the limits of the inference represented here.
    pub reason: &'static str,
}

pub(crate) fn capability_report(
    language: &ValidatedLanguageSpec,
    prepared: &PreparedLanguage,
    tree_sitter: Capability,
) -> ToolingCapabilities {
    let spec = language.spec();
    let default_lexical = LexicalSpec::default();
    let lexical = spec.lexical.as_ref().unwrap_or(&default_lexical);
    let has_symbols = spec
        .constructs
        .iter()
        .any(|construct| construct.symbol.is_some());
    let has_contextual_fields = spec
        .constructs
        .iter()
        .flat_map(|construct| &construct.fields)
        .any(|field| kind_has_enum(&field.kind) || kind_has_reference(&field.kind));
    let has_hover = spec
        .constructs
        .iter()
        .any(|construct| !construct.fields.is_empty() || construct.symbol.is_some());
    let has_custom_parsers = spec
        .constructs
        .iter()
        .any(|construct| construct.custom_parser.is_some());
    let textmate = if has_custom_parsers {
        Capability::partial(
            "TextMate highlighting is lexical and cannot provide parser-aware or semantic highlighting; custom parser callbacks may accept syntax outside the generated lexical rules.",
        )
    } else {
        Capability::partial(
            "TextMate highlighting is lexical and cannot provide parser-aware or semantic highlighting.",
        )
    };
    let artifacts = BTreeMap::from([
        ("agent_skill".to_owned(), Capability::supported()),
        (
            "language_configuration".to_owned(),
            if has_custom_parsers {
                Capability::partial(
                    "Generated comment and bracket settings cover declared lexical rules and structured fields; custom parser syntax may require host-specific settings.",
                )
            } else {
                Capability::supported()
            },
        ),
        ("language_metadata".to_owned(), Capability::supported()),
        ("reference_document".to_owned(), Capability::supported()),
        ("textmate_grammar".to_owned(), textmate),
        ("tree_sitter_grammar".to_owned(), tree_sitter),
        ("typed_rust_ast".to_owned(), Capability::supported()),
    ]);
    let mut features = BTreeMap::from([
        (
            "host.domain_lowering".to_owned(),
            Capability::customized(
                "Domain lowering is language-specific and must be implemented by the Rust host.",
            ),
        ),
        (
            "host.execution".to_owned(),
            Capability::customized(
                "Execution semantics and runtime state are owned by the Rust host.",
            ),
        ),
        (
            "host.type_system".to_owned(),
            Capability::customized(
                "Language-specific type rules require host-provided analysis callbacks.",
            ),
        ),
        (
            "language.embedded_completion_providers".to_owned(),
            Capability::customized(
                "Host applications can register typed completion providers for language-specific suggestions.",
            ),
        ),
        (
            "language.embedded_lsp_server".to_owned(),
            Capability::supported(),
        ),
        (
            "language.source_formatter".to_owned(),
            if has_custom_parsers {
                Capability::customized(
                    "Custom parser syntax requires a host-provided formatter for complete formatting.",
                )
            } else {
                Capability::supported()
            },
        ),
        ("lsp.document_symbols".to_owned(), Capability::supported()),
        (
            "lsp.hover".to_owned(),
            if has_hover {
                Capability::supported()
            } else {
                Capability::unavailable(
                    "The language declares no fields or symbols for schema-based hover information.",
                )
            },
        ),
        (
            "lsp.open_document_diagnostics".to_owned(),
            Capability::supported(),
        ),
        (
            "lsp.ranged_document_changes".to_owned(),
            Capability::supported(),
        ),
        (
            "lsp.semantic_tokens".to_owned(),
            if has_custom_parsers {
                Capability::partial(
                    "Generic semantic tokens classify built-in syntax; custom parser regions need classified host lexer tokens for complete highlighting.",
                )
            } else {
                Capability::supported()
            },
        ),
        (
            "lsp.parser_continuation_completion".to_owned(),
            Capability::partial(
                "Parser-derived continuations cover unambiguous literal and field sequences; optional, choice, and repeated patterns are not completed.",
            ),
        ),
        (
            "parser.custom_construct_callbacks".to_owned(),
            Capability::customized(
                "Host Rust callbacks can implement constructs that do not fit declarative syntax patterns.",
            ),
        ),
        (
            "parser.declarative_patterns".to_owned(),
            Capability::supported(),
        ),
        (
            "lsp.project_definition_reload".to_owned(),
            Capability::partial(
                "Project mode reloads valid definition changes when the editor client supports dynamic watched-file registration.",
            ),
        ),
    ]);
    features.insert(
        "lsp.literal_keyword_completion".to_owned(),
        if prepared.keywords().next().is_some() {
            Capability::supported()
        } else {
            Capability::unavailable("The language has no literal syntax keywords to suggest.")
        },
    );
    features.insert(
        "lsp.contextual_completion".to_owned(),
        if has_contextual_fields {
            Capability::supported()
        } else {
            Capability::unavailable(
                "The language declares no enum or reference fields for schema-based suggestions.",
            )
        },
    );
    features.insert(
        "lsp.symbol_navigation".to_owned(),
        if has_symbols {
            Capability::partial(
                "Generic navigation and rename use open documents and configured project source snapshots; project safety limits can disable cross-file navigation, and host callback resolvers cannot be renamed generically.",
            )
        } else {
            Capability::unavailable("The language declares no symbol metadata.")
        },
    );

    ToolingCapabilities {
        schema_version: 2,
        language: spec.name.clone(),
        language_version: spec.version.clone(),
        root: spec.root.clone(),
        source_extensions: spec.source_extensions.clone(),
        artifacts,
        features,
        editor_configuration_assumptions: editor_configuration_assumptions(spec, lexical),
    }
}

fn editor_configuration_assumptions(
    spec: &LanguageSpec,
    lexical: &LexicalSpec,
) -> EditorConfigurationAssumptions {
    EditorConfigurationAssumptions {
        line_comment: lexical.line_comment.clone(),
        block_comment: lexical
            .block_comment
            .as_ref()
            .map(|comment| [comment.open.clone(), comment.close.clone()]),
        bracket_pairs: editor_bracket_pairs(spec),
        reason: "Comment settings follow the validated lexical policy; bracket pairs are inferred from declared blocks and built-in list, map, property, and expression fields, and undeclared conventional pairs are omitted.",
    }
}

fn editor_bracket_pairs(spec: &LanguageSpec) -> BTreeSet<(String, String)> {
    let mut brackets = BTreeSet::new();
    for construct in &spec.constructs {
        if let Some(block) = &construct.block {
            brackets.insert((block.open.clone(), block.close.clone()));
        }
        for field in &construct.fields {
            collect_field_bracket_pairs(&field.kind, &mut brackets);
        }
    }
    for record in &spec.types {
        for field in &record.fields {
            collect_field_bracket_pairs(&field.kind, &mut brackets);
        }
    }
    brackets
}

fn collect_field_bracket_pairs(kind: &FieldKind, output: &mut BTreeSet<(String, String)>) {
    match kind {
        FieldKind::Properties { fields, .. } => {
            output.insert(("{".to_owned(), "}".to_owned()));
            for field in fields {
                collect_field_bracket_pairs(&field.kind, output);
            }
        }
        FieldKind::List { item } => {
            output.insert(("[".to_owned(), "]".to_owned()));
            collect_field_bracket_pairs(item, output);
        }
        FieldKind::Map { value, .. } => {
            output.insert(("{".to_owned(), "}".to_owned()));
            collect_field_bracket_pairs(value, output);
        }
        FieldKind::Expression { .. } => {
            output.insert(("(".to_owned(), ")".to_owned()));
        }
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. } => {}
    }
}

fn kind_has_enum(kind: &FieldKind) -> bool {
    match kind {
        FieldKind::Enum { .. } | FieldKind::NamedEnum { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| kind_has_enum(&field.kind))
        }
        FieldKind::List { item } => kind_has_enum(item),
        FieldKind::Map { value, .. } => kind_has_enum(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => false,
    }
}

fn kind_has_reference(kind: &FieldKind) -> bool {
    match kind {
        FieldKind::Reference { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| kind_has_reference(&field.kind))
        }
        FieldKind::List { item } => kind_has_reference(item),
        FieldKind::Map { value, .. } => kind_has_reference(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Expression { .. } => false,
    }
}
