use std::path::PathBuf;

use clap::{Args, Parser};
use langmeld_language::{load_language, load_project};
use langmeld_lsp::serve;

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
    /// Standalone .lm or JSON language definition.
    #[arg(long, group = "language_input")]
    language: Option<PathBuf>,
    /// Project manifest whose effective language should be served.
    #[arg(long, group = "language_input")]
    project: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    let arguments = Cli::parse();
    let language = if let Some(path) = arguments.input.language {
        load_language(&path)
    } else {
        let path = arguments
            .input
            .project
            .as_deref()
            .expect("Clap requires a language definition or project");
        load_project(path).map(|project| project.language)
    };
    let language = match language {
        Ok(language) => language,
        Err(error) => {
            eprintln!("langmeld-lsp: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve(stdin.lock(), stdout.lock(), language) {
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
