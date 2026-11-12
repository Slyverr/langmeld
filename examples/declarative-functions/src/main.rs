use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;
use langmeld_core::{SourceFile, format_diagnostics, has_errors};
use langmeld_declarative_functions_example::{analyze, diagnostics};

const SAMPLE: &str = include_str!("../program.functionflow");

#[derive(Debug, Parser)]
#[command(version, about = "Analyze a FunctionFlow program.")]
struct Cli {
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let (name, text) = match cli.source {
        Some(path) => {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;

            (path.display().to_string(), text)
        }
        None => ("program.functionflow".to_owned(), SAMPLE.to_owned()),
    };
    let source = SourceFile::new(name, text);
    let analysis = analyze(&source)?;
    let diagnostics = diagnostics(&analysis);

    if !diagnostics.is_empty() {
        let formatted = format_diagnostics(std::slice::from_ref(&source), &diagnostics);

        if has_errors(&diagnostics) {
            return Err(formatted);
        }

        eprintln!("{formatted}");
    }

    println!(
        "Valid FunctionFlow program: {} function(s) declared.",
        analysis.parsed.tree().functions.len()
    );

    Ok(())
}
