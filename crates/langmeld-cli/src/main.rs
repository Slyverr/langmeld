use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "langmeld",
    version,
    about = "Create and work on programming language projects with Rust and Langmeld.",
    arg_required_else_help = true,
    subcommand_required = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a Rust + Langmeld language project.
    Init(InitArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Project directory. Defaults to the current directory.
    #[arg(long)]
    path: Option<PathBuf>,
    /// Language name. When omitted, infer it from the project directory.
    #[arg(long)]
    name: Option<String>,
    /// Always ask setup questions, even with redirected input.
    #[arg(long, conflicts_with = "no_interactive")]
    interactive: bool,
    /// Disable all setup questions.
    #[arg(long, conflicts_with = "interactive")]
    no_interactive: bool,
    /// Omit the project-level AGENTS.md file.
    #[arg(long, conflicts_with = "agents")]
    no_agents: bool,
    /// Include a local .agents/skills/langmeld skill.
    #[arg(long, conflicts_with = "no_agent_skill")]
    agent_skill: bool,
    /// Omit the local Langmeld agent skill.
    #[arg(long, conflicts_with = "agent_skill")]
    no_agent_skill: bool,
    /// Use a local Langmeld checkout for generated project dependencies.
    #[arg(long)]
    langmeld_path: Option<PathBuf>,
    /// Include the project-level AGENTS.md file.
    #[arg(long, conflicts_with = "no_agents")]
    agents: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init(_) => {
            eprintln!("langmeld init: project scaffolding is not available yet");
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_parses_init_options_and_rejects_conflicting_modes() {
        let cli = Cli::try_parse_from([
            "langmeld",
            "init",
            "--name",
            "SampleLanguage",
            "--path",
            "./sample",
            "--no-interactive",
            "--agents",
        ])
        .expect("init options parse");
        let Commands::Init(arguments) = cli.command;
        assert_eq!(arguments.name.as_deref(), Some("SampleLanguage"));
        assert_eq!(
            arguments.path.as_deref(),
            Some(std::path::Path::new("./sample"))
        );
        assert!(arguments.no_interactive);
        assert!(arguments.agents);

        assert!(
            Cli::try_parse_from(["langmeld", "init", "--interactive", "--no-interactive"]).is_err()
        );
    }

    #[test]
    fn clap_provides_help_without_custom_argument_parsing() {
        let mut command = Cli::command();
        assert!(command.render_long_help().to_string().contains("init"));
        let init = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "init")
            .expect("init command is registered");
        assert!(
            init.clone()
                .render_long_help()
                .to_string()
                .contains("--no-interactive")
        );
    }
}
