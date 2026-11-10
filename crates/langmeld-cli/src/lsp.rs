use std::{
    error::Error,
    path::{Path, PathBuf},
};

use clap::Args;
use langmeld_language::parse_language_definition_project;
use langmeld_lsp::DeclarativeBackend;
use langmeld_project::{DirectoryDefinitionResolver, read_project_manifest_file};

use crate::check;

#[derive(Debug, Args)]
pub(crate) struct LspArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn run(args: LspArgs) -> Result<(), Box<dyn Error>> {
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
        return Err(check::format_diagnostics(&resolver.sources(), &definition.diagnostics).into());
    }
    let language = definition
        .language
        .ok_or("validated definition did not produce a language")?;

    langmeld_lsp::run_stdio(DeclarativeBackend::new(language))
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}
