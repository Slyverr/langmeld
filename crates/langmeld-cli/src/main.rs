mod cli;
mod generate;
mod init;
mod scaffold;

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Commands::Init(arguments) => match init::initialize(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(2)
            }
        },
        cli::Commands::Generate(arguments) => match generate::generate(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(2)
            }
        },
    }
}
