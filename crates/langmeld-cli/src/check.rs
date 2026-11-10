use std::{error::Error, path::PathBuf};

use clap::Args;

use crate::project::load_language_definition;

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn run(args: CheckArgs) -> Result<(), Box<dyn Error>> {
    let language = load_language_definition(&args.manifest_path)?;
    println!("Language `{}` is valid.", language.name());
    Ok(())
}
