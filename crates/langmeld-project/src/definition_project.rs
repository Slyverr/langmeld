use std::{error::Error, fmt, path::Path, path::PathBuf};

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResult, parse_language_definition_project};

use crate::{
    DirectoryDefinitionResolver, ProjectLoadError, ProjectManifest, ProjectManifestError,
    read_project_manifest, read_project_manifest_file,
};

/// A manifest-backed language definition and the source snapshots it loaded.
pub struct LoadedDefinitionProject {
    /// Canonical directory containing the project manifest.
    pub root: PathBuf,
    /// Validated project settings used to locate the root definition.
    pub manifest: ProjectManifest,
    /// Parsed language definition and its source-located diagnostics.
    pub definition: DefinitionResult,
    /// Root and imported definition snapshots in stable path order.
    pub sources: Vec<SourceFile>,
}

impl LoadedDefinitionProject {
    /// Returns whether the definition produced a valid language without diagnostics.
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
    let definition = parse_language_definition_project(root, &resolver);
    let sources = resolver.sources();

    Ok(LoadedDefinitionProject {
        root: resolver.root().to_path_buf(),
        manifest,
        definition,
        sources,
    })
}
