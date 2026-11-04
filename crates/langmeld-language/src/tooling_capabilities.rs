use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

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
