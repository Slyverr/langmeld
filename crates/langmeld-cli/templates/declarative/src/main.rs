use std::{fs, path::PathBuf};

use clap::Parser;
use langmeld_core::{SourceFile, format_diagnostics};
use language::{LanguageLoadError, load};

const SAMPLE: &str = "program { let message = \"Hello, Langmeld!\"; print message; }";

#[derive(Debug, Parser)]
#[command(version, about = "Run the generated language.")]
struct Cli {
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let cli = Cli::parse();
    let (source_name, source_text) = match cli.source {
        Some(path) => {
            let source_text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;

            (path.display().to_string(), source_text)
        }
        None => ("example.widget".to_owned(), SAMPLE.to_owned()),
    };

    let source = SourceFile::new(source_name, source_text);
    let language = load().map_err(format_language_load_error)?;
    let report = language.run(&source).map_err(|failure| {
        format_diagnostics(std::slice::from_ref(&source), &failure.diagnostics)
    })?;

    if !report.diagnostics.is_empty() {
        eprintln!(
            "{}",
            format_diagnostics(std::slice::from_ref(&source), &report.diagnostics)
        );
    }

    print!("{}", report.output);

    Ok(())
}

fn format_language_load_error(error: LanguageLoadError) -> String {
    if error.diagnostics().is_empty() {
        return error.to_string();
    }

    format_diagnostics(error.sources(), error.diagnostics())
}
