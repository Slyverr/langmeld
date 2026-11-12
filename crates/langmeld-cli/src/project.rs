use std::{error::Error, path::Path};

use langmeld_core::{Diagnostic, SourceFile, format_diagnostics};
use langmeld_language::LanguageSpec;
use langmeld_project::{
    ProjectManifest, load_definition_project_from_manifest, read_project_manifest_file,
};

pub(crate) struct DefinitionProjectContext {
    pub(crate) root: std::path::PathBuf,
    pub(crate) manifest: ProjectManifest,
}

impl DefinitionProjectContext {
    pub(crate) fn new(manifest_path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let manifest_path = manifest_path.as_ref();
        let manifest = read_project_manifest_file(manifest_path)?;
        let root = manifest_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        Ok(Self { root, manifest })
    }
}

pub(crate) struct LoadedDefinitionProject {
    pub(crate) root: std::path::PathBuf,
    pub(crate) language: LanguageSpec,
    pub(crate) sources: Vec<SourceFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn load_language_definition(
    manifest_path: impl AsRef<Path>,
) -> Result<LanguageSpec, Box<dyn Error>> {
    Ok(load_definition_project(manifest_path)?.language)
}

pub(crate) fn load_definition_project(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedDefinitionProject, Box<dyn Error>> {
    let project = load_definition_project_from_manifest(manifest_path)?;
    let (root, _, definition, sources) = project.into_parts();

    if !definition.is_valid() {
        return Err(format_diagnostics(&sources, &definition.diagnostics).into());
    }
    let diagnostics = definition.diagnostics;
    let language = definition
        .language
        .ok_or("validated definition did not produce a language")?;

    Ok(LoadedDefinitionProject {
        root,
        language,
        sources,
        diagnostics,
    })
}
