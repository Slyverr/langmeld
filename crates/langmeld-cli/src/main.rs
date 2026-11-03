mod check;
mod cli;
mod configure;
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
        cli::Commands::Configure(arguments) => match configure::configure_project(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(2)
            }
        },
        cli::Commands::Check(arguments) => match check::check_workspace(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err((code, message)) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(code)
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
