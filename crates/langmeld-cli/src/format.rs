use std::{error::Error, fs, path::PathBuf};

use clap::Args;
use langmeld_core::SourceFile;
use langmeld_language::{DefinitionFormatOptions, format_language_definition};

use crate::project::{format_diagnostics, load_definition_project};

#[derive(Debug, Args)]
pub(crate) struct FormatArgs {
    /// Format one `.langmeld` file instead of the project import graph.
    file: Option<PathBuf>,
    /// Project manifest to use when formatting the complete definition graph.
    #[arg(long, conflicts_with = "file")]
    manifest_path: Option<PathBuf>,
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
    let options = DefinitionFormatOptions {
        tab_size: args.tab_width.unwrap_or(4),
        insert_spaces: !args.tabs,
    };
    let files = if let Some(file) = args.file {
        let text = fs::read_to_string(&file)?;
        let source = SourceFile::new(file.display().to_string(), text);
        vec![(file, source)]
    } else {
        let manifest_path = args
            .manifest_path
            .unwrap_or_else(|| PathBuf::from("langmeld.toml"));
        let project = load_definition_project(&manifest_path)?;
        project
            .sources
            .into_iter()
            .map(|source| (project.root.join(&source.name), source))
            .collect()
    };
    let mut outputs = Vec::with_capacity(files.len());
    for (path, source) in files {
        let formatted = format_language_definition(&source, options).map_err(|diagnostics| {
            format_diagnostics(std::slice::from_ref(&source), &diagnostics)
        })?;
        outputs.push((path, source.text, formatted));
    }

    if args.check {
        let unformatted = outputs
            .iter()
            .filter(|(_, original, formatted)| original != formatted)
            .map(|(path, _, _)| path.display().to_string())
            .collect::<Vec<_>>();
        if !unformatted.is_empty() {
            return Err(format!("files need formatting:\n{}", unformatted.join("\n")).into());
        }
        println!("All {} definition file(s) are formatted", outputs.len());
        return Ok(());
    }

    for (path, original, formatted) in outputs {
        if formatted != original {
            fs::write(&path, formatted)?;
            println!("Formatted {}", path.display());
        }
    }
    Ok(())
}
