use langmeld_language::{FormatOptions, format_source, load_language, load_source_files};

use crate::{check::format_diagnostic, cli::FormatArgs};

pub(crate) fn format_file(arguments: FormatArgs) -> Result<(), (u8, String)> {
    let language = load_language(&arguments.language).map_err(|error| (2, error))?;
    let mut sources =
        load_source_files(std::slice::from_ref(&arguments.source)).map_err(|error| (2, error))?;
    let source = sources
        .pop()
        .ok_or_else(|| (2, "source file did not load".to_owned()))?;
    let formatted =
        format_source(&language, &source, FormatOptions::default()).map_err(|errors| {
            let details = errors
                .iter()
                .map(|diagnostic| format_diagnostic(&source, diagnostic))
                .collect::<Vec<_>>()
                .join("\n");
            (1, details)
        })?;

    if arguments.write {
        std::fs::write(&arguments.source, formatted).map_err(|error| {
            (
                2,
                format!(
                    "cannot write source file '{}': {error}",
                    arguments.source.display()
                ),
            )
        })?;
    } else {
        print!("{formatted}");
    }
    Ok(())
}
