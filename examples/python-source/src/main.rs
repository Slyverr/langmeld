use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use langmeld::{core::SourceFile, language::load_project};
use python_source_language::{analyze_source, analyze_sources, has_errors};

fn main() -> ExitCode {
    match Args::parse().input {
        Input::Source { path } => analyze_one(path),
        Input::Project { root } => analyze_project(root),
    }
}

fn analyze_one(path: PathBuf) -> ExitCode {
    let source = match read_source(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let analysis = analyze_source(&source);
    let count = analysis.parsed.program.nodes.first().map_or(0, |module| {
        module
            .children
            .iter()
            .filter(|child| child.construct == "function")
            .count()
    });
    println!(
        "{}: {count} top-level Python declarations indexed",
        source.name
    );
    print_diagnostics(&analysis.diagnostics);
    if has_errors(&analysis) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn analyze_project(root: PathBuf) -> ExitCode {
    let manifest = root.join("langmeld.json");
    let project = match load_project(&manifest) {
        Ok(project) => project,
        Err(error) => {
            eprintln!(
                "cannot load Python project '{}': {error}",
                manifest.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let analysis = analyze_sources(&project.sources);
    println!(
        "{}: {} Python source file(s) indexed",
        root.display(),
        project.sources.len()
    );
    for diagnostic in &analysis.diagnostics {
        eprintln!(
            "{}: {}",
            diagnostic.diagnostic.code, diagnostic.diagnostic.message
        );
    }
    if analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic.severity == langmeld::core::Severity::Error)
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn read_source(path: &PathBuf) -> Result<SourceFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
    Ok(SourceFile::new(path.display().to_string(), text))
}

fn print_diagnostics(diagnostics: &[langmeld::core::Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
}

#[derive(Debug, Parser)]
#[command(about = "Index functions and calls in Python source files.")]
struct Args {
    #[command(subcommand)]
    input: Input,
}

#[derive(Debug, clap::Subcommand)]
enum Input {
    /// Index one Python source file.
    Source { path: PathBuf },
    /// Load and index files selected by a Langmeld project manifest.
    Project { root: PathBuf },
}
