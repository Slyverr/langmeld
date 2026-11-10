use std::{error::Error, path::Path};

use langmeld_core::SourceFile;
use langmeld_language::{LanguageSpec, parse_language_definition_project};
use langmeld_project::{DirectoryDefinitionResolver, ProjectManifest, read_project_manifest_file};

pub(crate) struct DefinitionProjectContext {
    pub(crate) root: std::path::PathBuf,
    pub(crate) manifest: ProjectManifest,
    pub(crate) resolver: DirectoryDefinitionResolver,
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
        let resolver = DirectoryDefinitionResolver::new(&root)?;
        Ok(Self {
            root,
            manifest,
            resolver,
        })
    }
}

pub(crate) struct LoadedDefinitionProject {
    pub(crate) root: std::path::PathBuf,
    pub(crate) language: LanguageSpec,
    pub(crate) sources: Vec<SourceFile>,
}

pub(crate) fn load_language_definition(
    manifest_path: impl AsRef<Path>,
) -> Result<LanguageSpec, Box<dyn Error>> {
    Ok(load_definition_project(manifest_path)?.language)
}

pub(crate) fn load_definition_project(
    manifest_path: impl AsRef<Path>,
) -> Result<LoadedDefinitionProject, Box<dyn Error>> {
    let project = DefinitionProjectContext::new(manifest_path)?;
    let root = project.resolver.load(&project.manifest.definition)?;
    let definition = parse_language_definition_project(root, &project.resolver);
    let sources = project.resolver.sources();
    if !definition.is_valid() {
        return Err(format_diagnostics(&sources, &definition.diagnostics).into());
    }
    let language = definition
        .language
        .ok_or("validated definition did not produce a language")?;
    Ok(LoadedDefinitionProject {
        root: project.root,
        language,
        sources,
    })
}

pub(crate) fn format_diagnostics(
    sources: &[SourceFile],
    diagnostics: &[langmeld_core::Diagnostic],
) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let location = diagnostic.location.as_ref();
            let source = location.and_then(|location| {
                sources
                    .iter()
                    .find(|source| source.name == location.source_name)
            });
            let (line, column) = match (source, location) {
                (Some(source), Some(location)) => source.line_column(location.span.start),
                _ => (1, 1),
            };
            let source_name = location
                .map(|location| location.source_name.as_str())
                .unwrap_or("<project>");
            format!(
                "{source_name}:{line}:{column}: {}: {}",
                diagnostic.code, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
