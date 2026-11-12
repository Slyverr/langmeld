use std::{error::Error, fmt, path::Path, path::PathBuf};

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{DefinitionResult, LanguageSpec, parse_language_definition_project};

use crate::{
    DirectoryDefinitionResolver, ProjectLoadError, ProjectManifest, ProjectManifestError,
    read_project_manifest, read_project_manifest_file,
};

/// A manifest-backed language definition and the source snapshots it loaded.
pub struct LoadedDefinitionProject {
    /// Canonical directory containing the project manifest.
    root: PathBuf,
    /// Validated project settings used to locate the root definition.
    manifest: ProjectManifest,
    /// Parsed language definition and its source-located diagnostics.
    definition: DefinitionResult,
    /// Root and imported definition snapshots in stable path order.
    sources: Vec<SourceFile>,
}

impl LoadedDefinitionProject {
    /// Returns the canonical directory containing the project manifest.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the validated project settings used to locate the root definition.
    #[must_use]
    pub const fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    /// Returns the parsed language definition and its diagnostics.
    #[must_use]
    pub const fn definition(&self) -> &DefinitionResult {
        &self.definition
    }

    /// Returns the root and imported definition snapshots in stable path order.
    #[must_use]
    pub fn sources(&self) -> &[SourceFile] {
        &self.sources
    }

    /// Consumes the loaded project and returns its root, manifest, parsed
    /// definition, and source snapshots in that order.
    #[must_use]
    pub fn into_parts(self) -> (PathBuf, ProjectManifest, DefinitionResult, Vec<SourceFile>) {
        (self.root, self.manifest, self.definition, self.sources)
    }

    /// Returns whether the definition produced a language without errors.
    ///
    /// Warning and informational diagnostics do not make the project invalid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.definition.is_valid()
    }
}

/// Filesystem or manifest error encountered while opening a definition project.
#[derive(Debug)]
pub enum DefinitionProjectError {
    /// The project manifest could not be read or validated.
    Manifest(ProjectManifestError),
    /// The manifest's root definition could not be loaded.
    Definition(ProjectLoadError),
}

impl fmt::Display for DefinitionProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => error.fmt(formatter),
            Self::Definition(error) => error.fmt(formatter),
        }
    }
}

impl Error for DefinitionProjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error),
            Self::Definition(error) => Some(error),
        }
    }
}

impl From<ProjectManifestError> for DefinitionProjectError {
    fn from(error: ProjectManifestError) -> Self {
        Self::Manifest(error)
    }
}

impl From<ProjectLoadError> for DefinitionProjectError {
    fn from(error: ProjectLoadError) -> Self {
        Self::Definition(error)
    }
}

/// Error returned when loading a project as a validated language specification.
#[derive(Debug)]
pub enum LanguageSpecLoadError {
    /// The project manifest or definition files could not be loaded.
    Project(DefinitionProjectError),
    /// The parsed definition did not produce a valid language specification.
    InvalidDefinition {
        /// Diagnostics tied to the loaded definition source snapshots.
        diagnostics: Vec<Diagnostic>,
        /// Source snapshots used to locate definition diagnostics.
        sources: Vec<SourceFile>,
    },
}

impl LanguageSpecLoadError {
    /// Returns definition diagnostics, or an empty slice for project I/O errors.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Project(_) => &[],
            Self::InvalidDefinition { diagnostics, .. } => diagnostics,
        }
    }

    /// Returns source snapshots used to locate definition diagnostics.
    #[must_use]
    pub fn sources(&self) -> &[SourceFile] {
        match self {
            Self::Project(_) => &[],
            Self::InvalidDefinition { sources, .. } => sources,
        }
    }
}

impl fmt::Display for LanguageSpecLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => error.fmt(formatter),
            Self::InvalidDefinition { .. } => formatter.write_str("language definition is invalid"),
        }
    }
}

impl Error for LanguageSpecLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(error) => Some(error),
            Self::InvalidDefinition { .. } => None,
        }
    }
}

impl From<DefinitionProjectError> for LanguageSpecLoadError {
    fn from(error: DefinitionProjectError) -> Self {
        Self::Project(error)
    }
}

/// Loads the conventional `langmeld.toml` project and parses its imports.
///
/// File-system and manifest failures are returned as [`DefinitionProjectError`].
/// Definition syntax and validation failures remain in the returned
/// [`LoadedDefinitionProject::definition`] diagnostics, alongside their source
/// snapshots, so callers can render them for their environment.
pub fn load_definition_project(
    project_root: impl AsRef<Path>,
) -> Result<LoadedDefinitionProject, DefinitionProjectError> {
    let project_root = project_root.as_ref();
    let manifest = read_project_manifest(project_root)?;
    load_with_manifest(project_root, manifest)
}

/// Loads a manifest-backed project and returns its validated language grammar.
///
/// Use [`load_definition_project`] when the caller needs to inspect an invalid
/// definition without treating its diagnostics as a load failure. This helper
/// retains source snapshots on validation errors for diagnostic rendering.
pub fn load_language_spec(
    project_root: impl AsRef<Path>,
) -> Result<LanguageSpec, LanguageSpecLoadError> {
    let project = load_definition_project(project_root)?;
    let (_, _, definition, sources) = project.into_parts();

    if !definition.is_valid() {
        return Err(LanguageSpecLoadError::InvalidDefinition {
            diagnostics: definition.diagnostics,
            sources,
        });
    }

    let Some(specification) = definition.language else {
        return Err(LanguageSpecLoadError::InvalidDefinition {
            diagnostics: definition.diagnostics,
            sources,
        });
    };

    Ok(specification)
}

/// Loads a definition project from a specified `langmeld.toml` path.
pub fn load_definition_project_from_manifest(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedDefinitionProject, DefinitionProjectError> {
    let manifest_path = manifest_path.as_ref();
    let manifest = read_project_manifest_file(manifest_path)?;
    let root = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    load_with_manifest(root, manifest)
}

fn load_with_manifest(
    project_root: &Path,
    manifest: ProjectManifest,
) -> Result<LoadedDefinitionProject, DefinitionProjectError> {
    let resolver = DirectoryDefinitionResolver::new(project_root)?;
    let root = resolver.load(&manifest.definition)?;
    let legacy_root_warning = manifest.definition.ends_with(".lm").then(|| {
        Diagnostic::warning(
            "LM-DEF-W001",
            "legacy `.lm` root definition; rename the file to `.langmeld` and update `definition` in `langmeld.toml`",
            root.source(),
            Span::new(0, 0),
        )
    });
    let mut definition = parse_language_definition_project(root, &resolver);

    if let Some(warning) = legacy_root_warning {
        definition.diagnostics.push(warning);
    }

    let sources = resolver.sources();

    Ok(LoadedDefinitionProject {
        root: resolver.root().to_path_buf(),
        manifest,
        definition,
        sources,
    })
}
