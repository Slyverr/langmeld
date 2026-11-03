//! Contracts for lowering validated programs into generated artifacts.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::Program;

/// One artifact format advertised by a backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendFormat {
    /// Stable format identifier used in compile options.
    pub id: String,
    /// MIME type assigned to emitted artifacts.
    pub media_type: String,
    /// Suggested file extension without a leading dot.
    pub extension: String,
}

/// A backend-owned setting exposed by project configuration and build commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendSetting {
    /// Stable key used in project manifests and `--set KEY=VALUE` arguments.
    pub key: String,
    /// Short label shown in interactive project configuration.
    pub label: String,
    /// Optional explanatory text shown beside the setting.
    pub description: Option<String>,
    /// Value passed to the backend when the caller does not configure this key.
    pub default: Option<String>,
    /// Allowed values; an empty list accepts any string.
    pub choices: Vec<String>,
}

impl BackendSetting {
    /// Declares a free-text backend setting.
    #[must_use]
    pub fn text(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: None,
            default: None,
            choices: Vec::new(),
        }
    }

    /// Declares a backend setting restricted to the supplied choices.
    #[must_use]
    pub fn choice(
        key: impl Into<String>,
        label: impl Into<String>,
        choices: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: None,
            default: None,
            choices: choices.into_iter().map(Into::into).collect(),
        }
    }

    /// Adds explanatory text for interactive project configuration.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the backend's default value for this setting.
    #[must_use]
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }
}

/// Capabilities and input requirements of one artifact backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendDescriptor {
    /// Stable backend identifier.
    pub id: String,
    /// Formats that the backend can produce.
    pub formats: Vec<BackendFormat>,
    /// Named IR or analysis artifacts required by this backend.
    ///
    /// The built-in `langmeld.program` artifact is provided by [`BackendInputs::program`].
    pub required_ir: Vec<String>,
    /// Whether the backend can map generated spans to source spans.
    pub supports_source_maps: bool,
}

/// A defect in a backend descriptor, found before backend code is invoked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendDescriptorIssue {
    /// The backend ID is empty or whitespace-only.
    EmptyBackendId,
    /// A backend must advertise at least one output format.
    NoFormats,
    /// A format ID is empty or whitespace-only.
    EmptyFormatId,
    /// Multiple advertised formats use the same ID.
    DuplicateFormatId(String),
    /// A format's media type or extension is missing or malformed.
    InvalidFormatMetadata { format: String, field: String },
    /// A required IR artifact ID is empty or whitespace-only.
    EmptyRequiredIr,
    /// A required IR artifact ID appears more than once.
    DuplicateRequiredIr(String),
}

impl fmt::Display for BackendDescriptorIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBackendId => formatter.write_str("backend ID is empty"),
            Self::NoFormats => formatter.write_str("backend advertises no output formats"),
            Self::EmptyFormatId => formatter.write_str("format ID is empty"),
            Self::DuplicateFormatId(id) => write!(formatter, "format ID `{id}` is duplicated"),
            Self::InvalidFormatMetadata { format, field } => {
                write!(formatter, "format `{format}` has invalid {field}")
            }
            Self::EmptyRequiredIr => formatter.write_str("required IR ID is empty"),
            Self::DuplicateRequiredIr(id) => {
                write!(formatter, "required IR ID `{id}` is duplicated")
            }
        }
    }
}

/// Options selected by the host application for one compilation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendOptions {
    /// Requested format from [`BackendDescriptor::formats`].
    pub format: String,
    /// Backend-specific target such as an architecture or preview profile.
    pub target: Option<String>,
    /// Whether generated-to-source mappings should be returned.
    pub source_maps: bool,
    /// Explicit backend-owned values. Declared defaults are added at compile time.
    pub settings: BTreeMap<String, String>,
}

impl BackendOptions {
    /// Requests one output format with default target and source-map settings.
    #[must_use]
    pub fn new(format: impl Into<String>) -> Self {
        Self {
            format: format.into(),
            ..Self::default()
        }
    }

    /// Sets one explicit backend-owned option for this compilation.
    #[must_use]
    pub fn with_setting(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }
}

/// Maps one generated byte range to the source range that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// Byte range in the generated artifact.
    pub generated: Span,
    /// Byte range in the original source file.
    pub source: Span,
    /// Index in the workspace input. `None` identifies a single source.
    pub source_index: Option<usize>,
}

impl SourceMapEntry {
    /// Creates a source mapping for a single-source compilation.
    #[must_use]
    pub const fn new(generated: Span, source: Span) -> Self {
        Self {
            generated,
            source,
            source_index: None,
        }
    }

    /// Creates a source mapping for one file in a workspace compilation.
    #[must_use]
    pub const fn in_source(generated: Span, source: Span, source_index: usize) -> Self {
        Self {
            generated,
            source,
            source_index: Some(source_index),
        }
    }
}

/// Output produced by a compiler or transpiler backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationArtifact {
    /// Format identifier selected from the backend descriptor.
    pub format: String,
    /// MIME type for the emitted bytes.
    pub media_type: String,
    /// Generated content. Backends do not write files themselves.
    pub bytes: Vec<u8>,
    /// Optional mappings from generated byte ranges to input source ranges.
    pub source_map: Vec<SourceMapEntry>,
}

/// Inputs produced by parsing, analysis, and earlier lowering passes.
///
/// The generic program is always available directly. Hosts can pass additional named
/// intermediate representations as bytes without coupling Langmeld to one IR type.
#[derive(Clone, Debug, PartialEq)]
pub struct BackendInputs<'a> {
    /// Generic Langmeld syntax tree for the source program.
    pub program: &'a Program,
    /// Original source snapshot when compilation began from source text.
    pub source: Option<&'a SourceFile>,
    /// Host-defined intermediate representations keyed by stable artifact IDs.
    pub artifacts: BTreeMap<String, Vec<u8>>,
}

impl<'a> BackendInputs<'a> {
    /// Creates backend inputs from a generic parsed program.
    #[must_use]
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            source: None,
            artifacts: BTreeMap::new(),
        }
    }

    /// Attaches the original source snapshot for source-preserving backends.
    #[must_use]
    pub fn with_source(mut self, source: &'a SourceFile) -> Self {
        self.source = Some(source);
        self
    }

    /// Adds a named host IR artifact for backends that declare it as a requirement.
    #[must_use]
    pub fn with_artifact(mut self, id: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.artifacts.insert(id.into(), bytes.into());
        self
    }
}

/// Host-implemented backend that lowers a Langmeld program to an artifact.
pub trait ArtifactBackend {
    /// Returns the backend's formats and required input capabilities.
    fn descriptor(&self) -> BackendDescriptor;

    /// Returns settings understood by this backend. Unlisted settings are rejected.
    fn settings(&self) -> Vec<BackendSetting> {
        Vec::new()
    }

    /// Lowers the supplied program using the selected options and resolved defaults.
    ///
    /// The caller owns parse and semantic orchestration and is responsible for deciding whether
    /// this program is valid for the requested output.
    ///
    /// # Errors
    /// Returns source diagnostics when the backend cannot lower the program.
    fn compile(
        &self,
        inputs: &BackendInputs<'_>,
        options: &BackendOptions,
    ) -> Result<CompilationArtifact, Vec<Diagnostic>>;
}

/// A defect in compile options, backend output, or backend execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    /// The backend descriptor is invalid.
    InvalidDescriptor {
        /// Backend whose descriptor failed validation.
        backend: String,
        /// Specific descriptor defect.
        issue: BackendDescriptorIssue,
    },
    /// The requested format was not advertised by the backend.
    UnsupportedFormat { backend: String, format: String },
    /// Source maps were requested from a backend that does not support them.
    UnsupportedSourceMaps { backend: String },
    /// The backend returned source maps when the caller did not request them.
    UnexpectedSourceMaps { backend: String },
    /// The backend declared an invalid or duplicate setting key/schema.
    InvalidSettingSchema { backend: String, key: String },
    /// The caller supplied a setting that the backend did not declare.
    UnsupportedSetting { backend: String, key: String },
    /// A setting value is not in the backend's advertised choices.
    InvalidSettingValue {
        backend: String,
        key: String,
        value: String,
    },
    /// Source maps were requested without an original source snapshot.
    MissingSourceSnapshotForMaps { backend: String },
    /// A required intermediate representation was not supplied.
    MissingRequiredIr { backend: String, artifact: String },
    /// The backend returned an artifact inconsistent with its advertised format.
    InvalidArtifact {
        backend: String,
        expected_format: String,
        actual_format: String,
        expected_media_type: String,
        actual_media_type: String,
    },
    /// A generated source-map range falls outside the returned artifact.
    InvalidSourceMapRange {
        backend: String,
        entry_index: usize,
        start: usize,
        end: usize,
        artifact_len: usize,
    },
    /// A single-file source map refers to a nonexistent source file.
    InvalidSourceMapSourceIndex {
        backend: String,
        entry_index: usize,
        source_index: usize,
        source_count: usize,
    },
    /// A source-map range is outside its input or splits a UTF-8 code point.
    InvalidSourceMapSourceRange {
        backend: String,
        entry_index: usize,
        source_index: usize,
        start: usize,
        end: usize,
        source_len: usize,
    },
    /// The backend rejected the program with source diagnostics.
    Diagnostics(Vec<Diagnostic>),
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDescriptor { backend, issue } => {
                write!(
                    formatter,
                    "backend `{backend}` has an invalid descriptor: {issue}"
                )
            }
            Self::UnsupportedFormat { backend, format } => {
                write!(formatter, "backend `{backend}` does not support `{format}`")
            }
            Self::UnsupportedSourceMaps { backend } => {
                write!(
                    formatter,
                    "backend `{backend}` does not support source maps"
                )
            }
            Self::UnexpectedSourceMaps { backend } => write!(
                formatter,
                "backend `{backend}` returned source maps that were not requested"
            ),
            Self::InvalidSettingSchema { backend, key } => write!(
                formatter,
                "backend `{backend}` declares an invalid or duplicate setting key `{key}`"
            ),
            Self::UnsupportedSetting { backend, key } => {
                write!(
                    formatter,
                    "backend `{backend}` does not declare setting `{key}`"
                )
            }
            Self::InvalidSettingValue {
                backend,
                key,
                value,
            } => write!(
                formatter,
                "backend `{backend}` does not accept `{value}` for setting `{key}`"
            ),
            Self::MissingSourceSnapshotForMaps { backend } => write!(
                formatter,
                "backend `{backend}` needs the original source snapshot to produce source maps"
            ),
            Self::MissingRequiredIr { backend, artifact } => write!(
                formatter,
                "backend `{backend}` requires missing IR artifact `{artifact}`"
            ),
            Self::InvalidArtifact {
                backend,
                expected_format,
                actual_format,
                expected_media_type,
                actual_media_type,
            } => write!(
                formatter,
                "backend `{backend}` advertised `{expected_format}` ({expected_media_type}) but returned `{actual_format}` ({actual_media_type})"
            ),
            Self::InvalidSourceMapRange {
                backend,
                entry_index,
                start,
                end,
                artifact_len,
            } => write!(
                formatter,
                "backend `{backend}` returned invalid generated range {start}..{end} for source-map entry {entry_index} in an artifact of {artifact_len} bytes"
            ),
            Self::InvalidSourceMapSourceIndex {
                backend,
                entry_index,
                source_index,
                source_count,
            } => write!(
                formatter,
                "backend `{backend}` returned source index {source_index} for source-map entry {entry_index}, but the compilation has {source_count} source file(s)"
            ),
            Self::InvalidSourceMapSourceRange {
                backend,
                entry_index,
                source_index,
                start,
                end,
                source_len,
            } => write!(
                formatter,
                "backend `{backend}` returned invalid source range {start}..{end} for source-map entry {entry_index} in source {source_index} of {source_len} bytes"
            ),
            Self::Diagnostics(diagnostics) => write!(
                formatter,
                "backend reported {} diagnostic(s)",
                diagnostics.len()
            ),
        }
    }
}

impl Error for BackendError {}

/// Validates a backend's advertised ID, formats, and required intermediate artifacts.
///
/// # Errors
/// Returns a typed descriptor issue when metadata is incomplete or ambiguous.
pub fn validate_backend_descriptor(descriptor: &BackendDescriptor) -> Result<(), BackendError> {
    let backend = descriptor.id.clone();
    let issue = if descriptor.id.trim().is_empty() {
        Some(BackendDescriptorIssue::EmptyBackendId)
    } else if descriptor.formats.is_empty() {
        Some(BackendDescriptorIssue::NoFormats)
    } else {
        let mut format_ids = BTreeSet::new();
        let mut issue = None;
        for format in &descriptor.formats {
            if format.id.trim().is_empty() {
                issue = Some(BackendDescriptorIssue::EmptyFormatId);
                break;
            }
            if !format_ids.insert(format.id.as_str()) {
                issue = Some(BackendDescriptorIssue::DuplicateFormatId(format.id.clone()));
                break;
            }
            if format.media_type.trim().is_empty() {
                issue = Some(BackendDescriptorIssue::InvalidFormatMetadata {
                    format: format.id.clone(),
                    field: "media type".to_owned(),
                });
                break;
            }
            if format.extension.trim().is_empty()
                || format.extension.starts_with('.')
                || format.extension.ends_with('.')
                || format.extension.chars().any(char::is_whitespace)
                || format.extension.contains('/')
                || format.extension.contains('\\')
            {
                issue = Some(BackendDescriptorIssue::InvalidFormatMetadata {
                    format: format.id.clone(),
                    field: "extension".to_owned(),
                });
                break;
            }
        }
        issue
    };
    if let Some(issue) = issue {
        return Err(BackendError::InvalidDescriptor { backend, issue });
    }

    let mut required_ir = BTreeSet::new();
    for id in &descriptor.required_ir {
        if id.trim().is_empty() {
            return Err(BackendError::InvalidDescriptor {
                backend,
                issue: BackendDescriptorIssue::EmptyRequiredIr,
            });
        }
        if !required_ir.insert(id.as_str()) {
            return Err(BackendError::InvalidDescriptor {
                backend,
                issue: BackendDescriptorIssue::DuplicateRequiredIr(id.clone()),
            });
        }
    }
    Ok(())
}

/// Resolves explicit values and declared defaults against a backend setting schema.
///
/// Explicit values override defaults. Settings without an explicit value or a default are
/// omitted from the returned map.
///
/// # Errors
/// Returns an error for malformed schemas, unknown keys, or values outside declared choices.
pub fn resolve_backend_settings(
    backend: &str,
    schema: &[BackendSetting],
    values: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, BackendError> {
    let mut settings = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for setting in schema {
        if setting.key.trim().is_empty()
            || setting.key.contains('=')
            || !keys.insert(setting.key.as_str())
            || setting.choices.iter().collect::<BTreeSet<_>>().len() != setting.choices.len()
            || setting.default.as_ref().is_some_and(|default| {
                !setting.choices.is_empty() && !setting.choices.contains(default)
            })
        {
            return Err(BackendError::InvalidSettingSchema {
                backend: backend.to_owned(),
                key: setting.key.clone(),
            });
        }
        settings.insert(setting.key.as_str(), setting);
    }

    let mut resolved = BTreeMap::new();
    for (key, value) in values {
        let Some(setting) = settings.get(key.as_str()) else {
            return Err(BackendError::UnsupportedSetting {
                backend: backend.to_owned(),
                key: key.clone(),
            });
        };
        if !setting.choices.is_empty() && !setting.choices.contains(value) {
            return Err(BackendError::InvalidSettingValue {
                backend: backend.to_owned(),
                key: key.clone(),
                value: value.clone(),
            });
        }
        resolved.insert(key.clone(), value.clone());
    }
    for setting in schema {
        if let Some(default) = &setting.default {
            resolved
                .entry(setting.key.clone())
                .or_insert_with(|| default.clone());
        }
    }
    Ok(resolved)
}

/// Validates values against a backend's setting schema.
///
/// # Errors
/// Returns a schema error for duplicate or malformed declarations, an unsupported-setting error
/// for unknown keys, or an invalid-value error for values outside an advertised choice list.
pub fn validate_backend_settings(
    backend: &str,
    schema: &[BackendSetting],
    values: &BTreeMap<String, String>,
) -> Result<(), BackendError> {
    resolve_backend_settings(backend, schema, values).map(drop)
}

/// Validates inputs, options, and output metadata around one lowering call.
///
/// # Errors
/// Rejects unsupported options and missing IR before invoking the backend, preserves backend
/// diagnostics, and validates returned format metadata and source-map ranges.
pub fn compile_with_backend(
    backend: &impl ArtifactBackend,
    inputs: &BackendInputs<'_>,
    options: &BackendOptions,
) -> Result<CompilationArtifact, BackendError> {
    let descriptor = backend.descriptor();
    validate_backend_descriptor(&descriptor)?;
    let resolved_settings =
        resolve_backend_settings(&descriptor.id, &backend.settings(), &options.settings)?;
    let Some(format) = descriptor
        .formats
        .iter()
        .find(|format| format.id == options.format)
    else {
        return Err(BackendError::UnsupportedFormat {
            backend: descriptor.id,
            format: options.format.clone(),
        });
    };
    if let Some(artifact) = descriptor.required_ir.iter().find(|required| {
        required.as_str() != "langmeld.program" && !inputs.artifacts.contains_key(*required)
    }) {
        return Err(BackendError::MissingRequiredIr {
            backend: descriptor.id,
            artifact: artifact.clone(),
        });
    }
    if options.source_maps && !descriptor.supports_source_maps {
        return Err(BackendError::UnsupportedSourceMaps {
            backend: descriptor.id,
        });
    }
    if options.source_maps && inputs.source.is_none() {
        return Err(BackendError::MissingSourceSnapshotForMaps {
            backend: descriptor.id,
        });
    }

    let mut resolved_options = options.clone();
    resolved_options.settings = resolved_settings;
    let artifact = backend
        .compile(inputs, &resolved_options)
        .map_err(BackendError::Diagnostics)?;
    let sources = inputs.source.into_iter().collect::<Vec<_>>();
    validate_artifact(&descriptor, format, artifact, &sources, options.source_maps)
}

fn validate_artifact(
    descriptor: &BackendDescriptor,
    format: &BackendFormat,
    artifact: CompilationArtifact,
    sources: &[&SourceFile],
    source_maps_requested: bool,
) -> Result<CompilationArtifact, BackendError> {
    if artifact.format != format.id || artifact.media_type != format.media_type {
        return Err(BackendError::InvalidArtifact {
            backend: descriptor.id.clone(),
            expected_format: format.id.clone(),
            actual_format: artifact.format,
            expected_media_type: format.media_type.clone(),
            actual_media_type: artifact.media_type,
        });
    }
    if !artifact.source_map.is_empty() {
        if !descriptor.supports_source_maps {
            return Err(BackendError::UnsupportedSourceMaps {
                backend: descriptor.id.clone(),
            });
        }
        if !source_maps_requested {
            return Err(BackendError::UnexpectedSourceMaps {
                backend: descriptor.id.clone(),
            });
        }
        if sources.is_empty() {
            return Err(BackendError::MissingSourceSnapshotForMaps {
                backend: descriptor.id.clone(),
            });
        }
    }
    if let Some((entry_index, entry)) = artifact.source_map.iter().enumerate().find(|(_, entry)| {
        entry.generated.start > entry.generated.end || entry.generated.end > artifact.bytes.len()
    }) {
        return Err(BackendError::InvalidSourceMapRange {
            backend: descriptor.id.clone(),
            entry_index,
            start: entry.generated.start,
            end: entry.generated.end,
            artifact_len: artifact.bytes.len(),
        });
    }
    if let Some(source) = sources.first() {
        for (entry_index, entry) in artifact.source_map.iter().enumerate() {
            let source_index = entry.source_index.unwrap_or(0);
            if source_index != 0 {
                return Err(BackendError::InvalidSourceMapSourceIndex {
                    backend: descriptor.id.clone(),
                    entry_index,
                    source_index,
                    source_count: 1,
                });
            }
            let span = entry.source;
            if span.start > span.end
                || span.end > source.text.len()
                || !source.text.is_char_boundary(span.start)
                || !source.text.is_char_boundary(span.end)
            {
                return Err(BackendError::InvalidSourceMapSourceRange {
                    backend: descriptor.id.clone(),
                    entry_index,
                    source_index,
                    start: span.start,
                    end: span.end,
                    source_len: source.text.len(),
                });
            }
        }
    }
    Ok(artifact)
}
