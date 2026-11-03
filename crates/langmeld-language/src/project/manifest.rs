//! Versioned project manifest types and their stable JSON representation.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{MAX_MANIFEST_BYTES, extensions::ManifestExtension, validate_manifest_paths};

/// The source of a project's base language model.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectLanguageSource {
    /// A checked-in declarative language definition, including its imports.
    Declarative { path: PathBuf },
    /// A canonical language model supplied by the project's compiled Rust host.
    RustHost {},
}

/// Project-level language source and program discovery settings in `langmeld.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectManifest {
    /// Version of the project-manifest format.
    pub schema_version: u32,
    /// Source mode for the project's base language model.
    pub language_source: ProjectLanguageSource,
    /// Explicit program source files, relative to this manifest.
    pub sources: Vec<PathBuf>,
    /// Directories recursively scanned for program source files.
    pub source_roots: Vec<PathBuf>,
    /// Source suffixes used when scanning roots; empty inherits the language definition.
    pub source_extensions: Vec<String>,
    /// Ordered local declarative extensions composed onto the base language.
    pub extensions: Vec<ManifestExtension>,
    /// Optional backend defaults used by the project's Rust host.
    pub build: Option<ProjectBuildConfig>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectManifestV1 {
    schema_version: u32,
    language: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source_roots: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extensions: Vec<ManifestExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<ProjectBuildConfig>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectManifestV2 {
    schema_version: u32,
    language_source: ProjectLanguageSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source_roots: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extensions: Vec<ManifestExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<ProjectBuildConfig>,
}

impl Serialize for ProjectManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match (&self.schema_version, &self.language_source) {
            (1, ProjectLanguageSource::Declarative { path }) => ProjectManifestV1 {
                schema_version: 1,
                language: path.clone(),
                sources: self.sources.clone(),
                source_roots: self.source_roots.clone(),
                source_extensions: self.source_extensions.clone(),
                extensions: self.extensions.clone(),
                build: self.build.clone(),
            }
            .serialize(serializer),
            (2, _) => ProjectManifestV2 {
                schema_version: 2,
                language_source: self.language_source.clone(),
                sources: self.sources.clone(),
                source_roots: self.source_roots.clone(),
                source_extensions: self.source_extensions.clone(),
                extensions: self.extensions.clone(),
                build: self.build.clone(),
            }
            .serialize(serializer),
            (1, ProjectLanguageSource::RustHost {}) => Err(serde::ser::Error::custom(
                "Rust-host language sources require project manifest schema version 2",
            )),
            (version, _) => Err(serde::ser::Error::custom(format!(
                "unsupported project manifest schema version {version}; expected 1 or 2"
            ))),
        }
    }
}

impl<'de> Deserialize<'de> for ProjectManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                serde::de::Error::custom("project manifest requires an integer `schema_version`")
            })?;
        match version {
            1 => {
                let manifest: ProjectManifestV1 =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self {
                    schema_version: 1,
                    language_source: ProjectLanguageSource::Declarative {
                        path: manifest.language,
                    },
                    sources: manifest.sources,
                    source_roots: manifest.source_roots,
                    source_extensions: manifest.source_extensions,
                    extensions: manifest.extensions,
                    build: manifest.build,
                })
            }
            2 => {
                let manifest: ProjectManifestV2 =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self {
                    schema_version: 2,
                    language_source: manifest.language_source,
                    sources: manifest.sources,
                    source_roots: manifest.source_roots,
                    source_extensions: manifest.source_extensions,
                    extensions: manifest.extensions,
                    build: manifest.build,
                })
            }
            other => Err(serde::de::Error::custom(format!(
                "unsupported project manifest schema version {other}; expected 1 or 2"
            ))),
        }
    }
}

/// Backend preferences persisted in a project manifest.
///
/// The backend implementation remains registered by the project's Rust host;
/// this data only records that host's selected defaults.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildConfig {
    /// Stable ID of a backend registered by the project host.
    pub backend: String,
    /// Default output format, or `None` when the backend has a single format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Backend-specific target, such as an architecture or preview profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Whether to request generated-to-source mappings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_maps: Option<bool>,
    /// Backend-owned setting values keyed by their declared setting IDs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, String>,
}

impl ProjectBuildConfig {
    /// Creates backend preferences for one host-registered backend.
    #[must_use]
    pub fn new(backend: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            ..Self::default()
        }
    }
}

impl ProjectManifest {
    /// Creates a schema v1 manifest for a declarative language definition.
    #[must_use]
    pub fn new(language: impl Into<PathBuf>) -> Self {
        Self {
            schema_version: 1,
            language_source: ProjectLanguageSource::Declarative {
                path: language.into(),
            },
            sources: Vec::new(),
            source_roots: Vec::new(),
            source_extensions: Vec::new(),
            extensions: Vec::new(),
            build: None,
        }
    }

    /// Creates a schema v2 manifest for a declarative language definition.
    #[must_use]
    pub fn new_declarative(language: impl Into<PathBuf>) -> Self {
        Self {
            schema_version: 2,
            language_source: ProjectLanguageSource::Declarative {
                path: language.into(),
            },
            sources: Vec::new(),
            source_roots: Vec::new(),
            source_extensions: Vec::new(),
            extensions: Vec::new(),
            build: None,
        }
    }

    /// Creates a schema v2 manifest whose language model is supplied by Rust host code.
    #[must_use]
    pub fn new_rust_host() -> Self {
        Self {
            schema_version: 2,
            language_source: ProjectLanguageSource::RustHost {},
            sources: Vec::new(),
            source_roots: Vec::new(),
            source_extensions: Vec::new(),
            extensions: Vec::new(),
            build: None,
        }
    }

    /// Returns the declarative base-definition path, when this manifest has one.
    #[must_use]
    pub fn declarative_language_path(&self) -> Option<&Path> {
        match &self.language_source {
            ProjectLanguageSource::Declarative { path } => Some(path),
            ProjectLanguageSource::RustHost {} => None,
        }
    }

    /// Serializes this manifest as stable, human-readable JSON with a final newline.
    pub fn to_json(&self) -> Result<String, String> {
        if !matches!(self.schema_version, 1 | 2) {
            return Err(format!(
                "unsupported project manifest schema version {}; expected 1 or 2",
                self.schema_version
            ));
        }
        validate_manifest_paths(self)?;
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("cannot serialize project manifest: {error}"))?;
        json.push('\n');
        if json.len() > MAX_MANIFEST_BYTES {
            return Err(format!(
                "project manifest exceeds the {MAX_MANIFEST_BYTES}-byte limit"
            ));
        }
        Ok(json)
    }
}
