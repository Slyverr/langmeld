use std::{error::Error, path::PathBuf};

use clap::Args;
use langmeld_lsp::DeclarativeBackend;

use crate::project::load_language_definition;

#[derive(Debug, Args)]
pub(crate) struct LspArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn run(args: LspArgs) -> Result<(), Box<dyn Error>> {
    let language = load_language_definition(&args.manifest_path)?;
    langmeld_lsp::run_stdio(DeclarativeBackend::new(language))
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}
