use std::path::PathBuf;

use clap::{Args, Parser};
use langmeld_language::load_language;
use langmeld_lsp::{serve, serve_definitions, serve_project};

#[derive(Debug, Parser)]
#[command(
    name = "langmeld-lsp",
    about = "Serve Langmeld diagnostics over the Language Server Protocol.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(flatten)]
    input: LanguageInput,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("language_input").required(true).multiple(false)))]
struct LanguageInput {
    /// Serve the built-in grammar for editing Langmeld `.lm` definitions.
    #[arg(long, group = "language_input")]
    definitions: bool,
    /// Standalone .lm or JSON language definition.
    #[arg(long, group = "language_input")]
    language: Option<PathBuf>,
    /// Project manifest whose effective language should be served.
    #[arg(long, group = "language_input")]
    project: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    let arguments = Cli::parse();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let result = if arguments.input.definitions {
        serve_definitions(stdin.lock(), stdout.lock())
    } else if let Some(path) = arguments.input.language {
        match load_language(&path) {
            Ok(language) => serve(stdin.lock(), stdout.lock(), language),
            Err(error) => Err(error),
        }
    } else {
        let path = arguments
            .input
            .project
            .as_deref()
            .expect("Clap requires a language definition or project");
        serve_project(stdin.lock(), stdout.lock(), path)
    };
    match result {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("langmeld-lsp: client exited without shutdown");
            std::process::ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("langmeld-lsp: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
