mod check;
mod init;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Build languages with Rust and Langmeld")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a starter language project.
    Init(init::InitArgs),
    /// Validate the project manifest and language definition.
    Check(check::CheckArgs),
}

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => match run(cli) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            let _ = error.print();
            ExitCode::from(error.exit_code() as u8)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Init(args) => init::run(args)?,
        Command::Check(args) => check::run(args)?,
    }
    Ok(())
}
