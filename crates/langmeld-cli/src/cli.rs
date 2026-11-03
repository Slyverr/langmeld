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
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Create a Rust + Langmeld language project.
    Init(InitArgs),
    /// Rebuild generated language and editor tooling.
    Generate(GenerateArgs),
}

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Project directory. Defaults to the current directory.
    #[arg(long)]
    pub(crate) path: Option<PathBuf>,
    /// Language name. When omitted, infer it from the project directory.
    #[arg(long)]
    pub(crate) name: Option<String>,
    /// Always ask setup questions, even with redirected input.
    #[arg(long, conflicts_with = "no_interactive")]
    pub(crate) interactive: bool,
    /// Disable all setup questions.
    #[arg(long, conflicts_with = "interactive")]
    pub(crate) no_interactive: bool,
    /// Omit the project-level AGENTS.md file.
    #[arg(long, conflicts_with = "agents")]
    pub(crate) no_agents: bool,
    /// Include the project-level AGENTS.md file.
    #[arg(long, conflicts_with = "no_agents")]
    pub(crate) agents: bool,
    /// Include a local .agents/skills/langmeld skill.
    #[arg(long, conflicts_with = "no_agent_skill")]
    pub(crate) agent_skill: bool,
    /// Omit the local Langmeld agent skill.
    #[arg(long, conflicts_with = "agent_skill")]
    pub(crate) no_agent_skill: bool,
    /// Use a local Langmeld checkout for generated project dependencies.
    #[arg(long)]
    pub(crate) langmeld_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
    /// Project root containing language/language.lm.
    #[arg(long, default_value = ".")]
    pub(crate) project_root: PathBuf,
    /// Verify generated files without writing them.
    #[arg(long, conflicts_with = "install_host")]
    pub(crate) check: bool,
    /// Install or safely refresh generated Rust host modules under src/host.
    #[arg(long)]
    pub(crate) install_host: bool,
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
        let Commands::Init(arguments) = cli.command else {
            panic!("init command parsed as another subcommand");
        };
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
    fn clap_help_includes_init_and_its_interaction_controls() {
        let mut command = Cli::command();
        assert!(command.render_long_help().to_string().contains("init"));
        assert!(command.render_long_help().to_string().contains("generate"));
        let init = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "init")
            .expect("init command is registered");
        let help = init.clone().render_long_help().to_string();
        assert!(help.contains("--interactive"));
        assert!(help.contains("--no-interactive"));
        assert!(help.contains("--agent-skill"));
        let generate = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "generate")
            .expect("generate command is registered");
        let help = generate.clone().render_long_help().to_string();
        assert!(help.contains("--install-host"));
    }

    #[test]
    fn generate_rejects_check_with_host_install() {
        assert!(
            Cli::try_parse_from(["langmeld", "generate", "--check", "--install-host"]).is_err()
        );
    }
}
