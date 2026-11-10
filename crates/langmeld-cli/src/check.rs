use std::{
    error::Error,
    path::{Path, PathBuf},
};

use clap::Args;
use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition_project;
use langmeld_project::{DirectoryDefinitionResolver, read_project_manifest_file};

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn run(args: CheckArgs) -> Result<(), Box<dyn Error>> {
    let manifest = read_project_manifest_file(&args.manifest_path)?;
    let project_root = args
        .manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let resolver = DirectoryDefinitionResolver::new(project_root)?;
    let root = resolver.load(&manifest.definition)?;
    let definition = parse_language_definition_project(root, &resolver);
    if !definition.is_valid() {
        return Err(format_diagnostics(&resolver.sources(), &definition.diagnostics).into());
    }
    let language = definition
        .language
        .ok_or("validated definition did not produce a language")?;

    println!("Language `{}` is valid.", language.name());
    Ok(())
}

fn format_diagnostics(sources: &[SourceFile], diagnostics: &[langmeld_core::Diagnostic]) -> String {
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
