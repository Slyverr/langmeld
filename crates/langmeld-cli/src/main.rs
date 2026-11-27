//! The `langmeld` binary: checks and tests languages written in `.langmeld` files, and
//! parses files with them.
//!
//! Exit codes: 0 when everything is fine, 1 when the language or the files it is run
//! on have problems, 2 when the command cannot run, such as an unreadable path.

mod check;
mod parse;
mod project;
mod test;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => {
            let _ = Cli::command().print_long_help();
            ExitCode::SUCCESS
        }
        Some(Command::Check { path, explain }) => check::run(&path, explain),
        Some(Command::Test { path, update }) => test::run(&path, update),
        Some(Command::Parse { grammar, file }) => parse::run(&grammar, &file),
    }
}
