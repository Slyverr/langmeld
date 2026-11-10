use std::{error::Error, path::PathBuf};

use clap::Args;
use langmeld_lsp::{DeclarativeBackend, LanguageDefinitionBackend};

use crate::definition_backend::ProjectDefinitionBackend;
use crate::project::load_language_definition;

#[derive(Debug, Args)]
pub(crate) struct LspArgs {
    /// Project manifest path; definition mode discovers ./langmeld.toml when omitted.
    #[arg(long)]
    manifest_path: Option<PathBuf>,
    /// Edit Langmeld `.langmeld` files; uses a project manifest when available.
    #[arg(long)]
    definitions: bool,
}

pub(crate) fn run(args: LspArgs) -> Result<(), Box<dyn Error>> {
    if args.definitions {
        let manifest_path = args.manifest_path;
        if let Some(manifest_path) = manifest_path {
            let backend = ProjectDefinitionBackend::new(manifest_path)?;
            langmeld_lsp::run_stdio(backend)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
        } else if PathBuf::from("langmeld.toml").is_file() {
            let backend = ProjectDefinitionBackend::new("langmeld.toml")?;
            langmeld_lsp::run_stdio(backend)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
        } else {
            langmeld_lsp::run_stdio(LanguageDefinitionBackend)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
        }
    } else {
        let manifest_path = args
            .manifest_path
            .unwrap_or_else(|| PathBuf::from("langmeld.toml"));
        let language = load_language_definition(&manifest_path)?;
        langmeld_lsp::run_stdio(DeclarativeBackend::new(language))
            .map_err(|error| std::io::Error::other(error.to_string()))?;
    }
    Ok(())
}
