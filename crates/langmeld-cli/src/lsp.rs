use std::{error::Error, path::PathBuf};

use clap::Args;
use langmeld_lsp::{DeclarativeBackend, LanguageDefinitionBackend};

use crate::project::load_language_definition;

#[derive(Debug, Args)]
pub(crate) struct LspArgs {
    /// Path to langmeld.toml. Defaults to ./langmeld.toml.
    #[arg(long, conflicts_with = "definitions")]
    manifest_path: Option<PathBuf>,
    /// Use the built-in grammar to edit Langmeld `.lm` definition files.
    #[arg(long, conflicts_with = "manifest_path")]
    definitions: bool,
}

pub(crate) fn run(args: LspArgs) -> Result<(), Box<dyn Error>> {
    if args.definitions {
        langmeld_lsp::run_stdio(LanguageDefinitionBackend)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
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
