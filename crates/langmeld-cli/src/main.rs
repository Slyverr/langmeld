//! The `langmeld` binary: starts, checks and tests languages written in `.langmeld`
//! files, and parses files with them.
//!
//! Exit codes: 0 when everything is fine, 1 when the language or the files it is run
//! on have problems, 2 when the command cannot run, such as an unreadable path.

mod check;
mod exhaustive;
mod parse;
mod project;
mod test;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use langmeld_cli::{Failure, error_line, line};

/// Tools for languages written in `.langmeld` files.
#[derive(Parser)]
#[command(name = "langmeld", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check a language: the grammar is read, validated and explained when it is wrong.
    Check {
        /// A `.langmeld` file, or the folder of a project with a `langmeld.toml`.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Also show where the grammar looks more than one token ahead to choose.
        #[arg(long)]
        explain: bool,
        /// Also list the nodes that the code in the project's `src` folder does not handle.
        #[arg(long)]
        exhaustive: bool,
    },
    /// Run the corpus files of a language project, and show what differs.
    Test {
        /// The folder of a project, or its `.langmeld` file.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Rewrite what the cases expect to what the language gives.
        #[arg(long)]
        update: bool,
    },
    /// Parse a file with a language written in a `.langmeld` file, with no Rust code.
    Parse {
        /// The `.langmeld` file of the language.
        #[arg(long)]
        grammar: PathBuf,
        /// The file to parse.
        file: PathBuf,
    },
    /// Say what a diagnostic code means and how to solve it.
    Explain {
        /// The code, such as `LM-P001`.
        code: String,
    },
}

/// The exit code of a command that gives a failure, after saying why if it could not run.
fn finish(result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if let Failure::Unable(reason) = &failure {
                error_line(&format!("error: {reason}"));
            }
            ExitCode::from(failure.code())
        }
    }
}

fn explain(code: &str) -> Result<(), Failure> {
    let text = langmeld_syntax::explain(code).ok_or_else(|| {
        Failure::Unable(format!(
            "there is no diagnostic code `{code}`; codes look like `LM-P001`"
        ))
    })?;
    line(&format!("{code}\n\n{text}"));
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => {
            let _ = Cli::command().print_long_help();
            ExitCode::SUCCESS
        }
        Some(Command::Check {
            path,
            explain,
            exhaustive,
        }) => check::run(&path, explain, exhaustive),
        Some(Command::Test { path, update }) => test::run(&path, update),
        Some(Command::Parse { grammar, file }) => parse::run(&grammar, &file),
        Some(Command::Explain { code }) => finish(explain(&code)),
    }
}
