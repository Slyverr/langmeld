use std::{error::Error, fs, path::PathBuf};

use clap::Args;
use langmeld_core::SourceFile;
use langmeld_language::{DefinitionFormatOptions, format_language_definition};

use crate::project::format_diagnostics;

#[derive(Debug, Args)]
pub(crate) struct FormatArgs {
    /// `.lm` definition file to format.
    file: PathBuf,
    /// Check formatting without modifying the file.
    #[arg(long)]
    check: bool,
    /// Spaces per indentation level (1 to 16).
    #[arg(long, conflicts_with = "tabs", value_parser = clap::value_parser!(u32).range(1..=16))]
    tab_width: Option<u32>,
    /// Indent with tab characters instead of spaces.
    #[arg(long)]
    tabs: bool,
}

pub(crate) fn run(args: FormatArgs) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(&args.file)?;
    let source = SourceFile::new(args.file.display().to_string(), text);
    let options = DefinitionFormatOptions {
        tab_size: args.tab_width.unwrap_or(4),
        insert_spaces: !args.tabs,
    };
    let formatted = format_language_definition(&source, options)
        .map_err(|diagnostics| format_diagnostics(std::slice::from_ref(&source), &diagnostics))?;

    if args.check {
        if formatted != source.text {
            return Err(format!("{} needs formatting", args.file.display()).into());
        }
        println!("{} is formatted", args.file.display());
        return Ok(());
    }

    if formatted != source.text {
        fs::write(&args.file, formatted)?;
        println!("Formatted {}", args.file.display());
    }
    Ok(())
}
