use std::{error::Error, path::PathBuf};

use clap::Args;
use langmeld_core::{Diagnostic, Severity, SourceFile, format_diagnostics};

use crate::project::load_definition_project;

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn run(args: CheckArgs) -> Result<(), Box<dyn Error>> {
    let project = load_definition_project(&args.manifest_path)?;
    let diagnostics = format_non_error_diagnostics(&project.sources, &project.diagnostics);
    if !diagnostics.is_empty() {
        eprintln!("{diagnostics}");
    }

    println!("Language `{}` is valid.", project.language.name());
    Ok(())
}

fn format_non_error_diagnostics(sources: &[SourceFile], diagnostics: &[Diagnostic]) -> String {
    let findings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity != Severity::Error)
        .cloned()
        .collect::<Vec<_>>();

    format_diagnostics(sources, &findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use langmeld_core::Span;

    #[test]
    fn formats_non_error_definition_diagnostics_with_source_locations() {
        let source = SourceFile::new("language/main.langmeld", "language \"Example\" {}");
        let diagnostics = [
            Diagnostic::warning(
                "LM-DEF-W001",
                "deprecated definition form",
                &source,
                Span::new(0, 8),
            ),
            Diagnostic::info(
                "LM-DEF-I001",
                "additional definition guidance",
                &source,
                Span::new(0, 8),
            ),
            Diagnostic::error(
                "LM-DEF-E001",
                "invalid definition form",
                &source,
                Span::new(9, 17),
            ),
        ];

        assert_eq!(
            format_non_error_diagnostics(&[source], &diagnostics),
            "language/main.langmeld:1:1: warning[LM-DEF-W001]: deprecated definition form\nlanguage/main.langmeld:1:1: info[LM-DEF-I001]: additional definition guidance"
        );
    }
}
