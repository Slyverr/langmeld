mod check;
mod cli;
mod configure;
mod extensions;
mod format;
mod generate;
mod host;
mod init;
mod language_tools;
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
        cli::Commands::Lock(arguments) => {
            match langmeld_language::write_project_lockfile(&arguments.project) {
                Ok(path) => {
                    println!("updated {}", path.display());
                    std::process::ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("langmeld: {message}");
                    std::process::ExitCode::from(2)
                }
            }
        }
        cli::Commands::Extension(arguments) => {
            match extensions::manage_extensions(&arguments.project, arguments.command) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("langmeld: {message}");
                    std::process::ExitCode::from(2)
                }
            }
        }
        cli::Commands::Format(arguments) => match format::format_file(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err((code, message)) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(code)
            }
        },
        cli::Commands::Inspect(arguments) => match language_tools::inspect(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err((code, message)) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(code)
            }
        },
        cli::Commands::Constructs(arguments) => {
            match language_tools::constructs(&arguments.language) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err((code, message)) => {
                    eprintln!("langmeld: {message}");
                    std::process::ExitCode::from(code)
                }
            }
        }
        cli::Commands::Describe(arguments) => {
            match language_tools::describe(&arguments.language, &arguments.construct) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err((code, message)) => {
                    eprintln!("langmeld: {message}");
                    std::process::ExitCode::from(code)
                }
            }
        }
        cli::Commands::Language(arguments) => match arguments.action {
            cli::LanguageAction::Inspect(arguments) => match language_tools::inspect(arguments) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err((code, message)) => {
                    eprintln!("langmeld: {message}");
                    std::process::ExitCode::from(code)
                }
            },
            cli::LanguageAction::Constructs(arguments) => {
                match language_tools::constructs(&arguments.language) {
                    Ok(()) => std::process::ExitCode::SUCCESS,
                    Err((code, message)) => {
                        eprintln!("langmeld: {message}");
                        std::process::ExitCode::from(code)
                    }
                }
            }
            cli::LanguageAction::Describe(arguments) => {
                match language_tools::describe(&arguments.language, &arguments.construct) {
                    Ok(()) => std::process::ExitCode::SUCCESS,
                    Err((code, message)) => {
                        eprintln!("langmeld: {message}");
                        std::process::ExitCode::from(code)
                    }
                }
            }
        },
        cli::Commands::Check(arguments) => match check::check_workspace(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err((code, message)) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(code)
            }
        },
        cli::Commands::Host(arguments) => host::run_host(&arguments),
        cli::Commands::Generate(arguments) => match generate::generate(arguments) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("langmeld: {message}");
                std::process::ExitCode::from(2)
            }
        },
    }
}
