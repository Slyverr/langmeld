use std::{ffi::OsString, path::PathBuf};

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
    /// Update project source discovery settings.
    Configure(ConfigureArgs),
    /// Refresh the integrity lock for local language definitions.
    Lock(LockArgs),
    /// Manage the project's local language extensions.
    Extension(ExtensionArgs),
    /// Format one source file using a language definition.
    Format(FormatArgs),
    /// Inspect a language definition or project.
    Inspect(InspectionArgs),
    /// List constructs declared by a language.
    Constructs(LanguageArgs),
    /// Describe one construct declared by a language.
    Describe(DescribeArgs),
    /// Language-oriented aliases for inspection commands.
    Language(LanguageCommand),
    /// Parse source files and run generic workspace analysis.
    #[command(alias = "validate")]
    Check(ValidationArgs),
    /// Run the initialized project's Rust host CLI.
    Host(HostArgs),
    /// Rebuild generated language and editor tooling.
    Generate(GenerateArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ConfigureArgs {
    /// Project manifest to edit.
    #[arg(long, default_value = "langmeld.json")]
    pub(crate) project: PathBuf,
    /// Always ask setup questions, even with redirected input.
    #[arg(long, conflicts_with = "no_interactive")]
    pub(crate) interactive: bool,
    /// Disable all setup questions.
    #[arg(long, conflicts_with = "interactive")]
    pub(crate) no_interactive: bool,
    /// Recursively scan this source root. Repeat to replace the root list.
    #[arg(long = "source-root", conflicts_with = "clear_source_roots")]
    pub(crate) source_roots: Vec<PathBuf>,
    /// Remove all configured source roots.
    #[arg(long, conflicts_with = "source_roots")]
    pub(crate) clear_source_roots: bool,
    /// Include this explicit source file. Repeat to replace the source list.
    #[arg(long = "source", conflicts_with = "clear_sources")]
    pub(crate) sources: Vec<PathBuf>,
    /// Remove all configured explicit source files.
    #[arg(long, conflicts_with = "sources")]
    pub(crate) clear_sources: bool,
    /// Override accepted source extensions. Repeat or comma-separate values.
    #[arg(
        long = "source-extension",
        value_delimiter = ',',
        conflicts_with = "inherit_source_extensions"
    )]
    pub(crate) source_extensions: Vec<String>,
    /// Inherit source extensions from the language definition.
    #[arg(long, conflicts_with = "source_extensions")]
    pub(crate) inherit_source_extensions: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LockArgs {
    /// Project manifest whose language definitions should be pinned.
    #[arg(long, default_value = "langmeld.json")]
    pub(crate) project: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct ExtensionArgs {
    /// Project manifest to update.
    #[arg(long, default_value = "langmeld.json")]
    pub(crate) project: PathBuf,
    #[command(subcommand)]
    pub(crate) command: ExtensionCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ExtensionCommand {
    /// List declarations in manifest order; this does not validate resolution.
    List,
    /// Add a versioned package or an unversioned local definition path.
    Add(ExtensionAddArgs),
    /// Update a versioned package by ID.
    Update(ExtensionUpdateArgs),
    /// Remove a versioned package by ID or a path declaration by path.
    Remove(ExtensionRemoveArgs),
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("extension_kind").required(true).multiple(false)))]
pub(crate) struct ExtensionAddArgs {
    /// Add an unversioned local definition path.
    #[arg(long, group = "extension_kind")]
    pub(crate) path_only: Option<PathBuf>,
    /// Package ID (requires --version, --path, and --language-version).
    #[arg(long, group = "extension_kind")]
    pub(crate) id: Option<String>,
    /// Exact package semantic version.
    #[arg(long, requires = "id")]
    pub(crate) version: Option<String>,
    /// Package definition path relative to the manifest.
    #[arg(long, requires = "id")]
    pub(crate) path: Option<PathBuf>,
    /// Compatible target language version range.
    #[arg(long, requires = "id")]
    pub(crate) language_version: Option<String>,
    /// Dependency in ID@VERSION-REQUIREMENT form; repeat for multiple.
    #[arg(long = "depends", requires = "id")]
    pub(crate) dependencies: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ExtensionUpdateArgs {
    /// Existing package ID.
    pub(crate) id: String,
    /// Replace the exact package semantic version.
    #[arg(long)]
    pub(crate) version: Option<String>,
    /// Replace the package definition path.
    #[arg(long)]
    pub(crate) path: Option<PathBuf>,
    /// Replace the compatible target language version range.
    #[arg(long)]
    pub(crate) language_version: Option<String>,
    /// Replace dependencies; repeat in ID@VERSION-REQUIREMENT form.
    #[arg(long = "depends", conflicts_with = "clear_dependencies")]
    pub(crate) dependencies: Vec<String>,
    /// Remove all dependencies from this package.
    #[arg(long, conflicts_with = "dependencies")]
    pub(crate) clear_dependencies: bool,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("extension_target").required(true).multiple(false)))]
pub(crate) struct ExtensionRemoveArgs {
    /// Versioned package ID.
    #[arg(group = "extension_target")]
    pub(crate) id: Option<String>,
    /// Exact unversioned local definition path.
    #[arg(long, group = "extension_target")]
    pub(crate) path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct FormatArgs {
    /// Language definition to use.
    #[arg(long)]
    pub(crate) language: PathBuf,
    /// Source file to format.
    #[arg(long)]
    pub(crate) source: PathBuf,
    /// Write the formatted source back to the file.
    #[arg(long)]
    pub(crate) write: bool,
}

#[derive(Clone, Debug, Args)]
#[command(group(clap::ArgGroup::new("language_input").required(true).multiple(false)))]
pub(crate) struct InspectionArgs {
    /// Language definition to inspect.
    #[arg(long, group = "language_input")]
    pub(crate) language: Option<PathBuf>,
    /// Project manifest to inspect.
    #[arg(long, group = "language_input")]
    pub(crate) project: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct LanguageArgs {
    /// Language definition.
    #[arg(long)]
    pub(crate) language: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct DescribeArgs {
    /// Language definition.
    #[arg(long)]
    pub(crate) language: PathBuf,
    /// Construct identifier.
    #[arg(long)]
    pub(crate) construct: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LanguageAction {
    /// Inspect a language definition or project.
    Inspect(InspectionArgs),
    /// List declared constructs.
    Constructs(LanguageArgs),
    /// Describe a construct.
    Describe(DescribeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct LanguageCommand {
    #[command(subcommand)]
    pub(crate) action: LanguageAction,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("language_input").required(true).multiple(false)))]
pub(crate) struct ValidationArgs {
    /// Project manifest; loads its language definition and configured sources.
    #[arg(long, group = "language_input", conflicts_with = "sources")]
    pub(crate) project: Option<PathBuf>,
    /// Language definition to validate against when checking explicit source files.
    #[arg(long, group = "language_input")]
    pub(crate) language: Option<PathBuf>,
    /// Source file to check. Repeat to analyze a workspace.
    #[arg(long = "source", required_unless_present = "project")]
    pub(crate) sources: Vec<PathBuf>,
    /// Diagnostic output format.
    #[arg(long, value_enum, default_value = "text")]
    pub(crate) format: OutputFormat,
}

#[derive(Debug, Args)]
pub(crate) struct HostArgs {
    /// Project manifest beside the Rust host's Cargo.toml.
    #[arg(long, default_value = "langmeld.json")]
    pub(crate) project: PathBuf,
    /// Arguments forwarded to the project's CLI; place them after `--`.
    #[arg(last = true, allow_hyphen_values = true)]
    pub(crate) arguments: Vec<OsString>,
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
    /// Create a library-only package without the command-line host.
    #[arg(long)]
    pub(crate) library_only: bool,
    /// Include the optional browser/WASM parser and editor adapter.
    #[arg(long)]
    pub(crate) wasm: bool,
    /// Select the language-definition authoring frontend.
    #[arg(long, value_enum)]
    pub(crate) authoring: Option<AuthoringMode>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub(crate) enum AuthoringMode {
    /// Author the language model in modular `.langmeld` files.
    #[default]
    Declarative,
    /// Author the canonical language model in Rust.
    Rust,
    /// Author a Rust base model and extend it with local `.langmeld` files.
    Combined,
}

#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
    /// Project root containing language/language.langmeld.
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
            "--wasm",
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
        assert!(arguments.wasm);
        assert!(
            Cli::try_parse_from(["langmeld", "init", "--interactive", "--no-interactive"]).is_err()
        );
    }

    #[test]
    fn clap_help_includes_init_and_its_interaction_controls() {
        let mut command = Cli::command();
        assert!(command.render_long_help().to_string().contains("init"));
        assert!(command.render_long_help().to_string().contains("configure"));
        assert!(command.render_long_help().to_string().contains("lock"));
        assert!(command.render_long_help().to_string().contains("format"));
        assert!(command.render_long_help().to_string().contains("inspect"));
        assert!(
            command
                .render_long_help()
                .to_string()
                .contains("constructs")
        );
        assert!(command.render_long_help().to_string().contains("language"));
        assert!(command.render_long_help().to_string().contains("check"));
        assert!(command.render_long_help().to_string().contains("generate"));
        let init = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "init")
            .expect("init command is registered");
        let help = init.clone().render_long_help().to_string();
        assert!(help.contains("--interactive"));
        assert!(help.contains("--no-interactive"));
        assert!(help.contains("--agent-skill"));
        assert!(help.contains("--wasm"));
        let configure = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "configure")
            .expect("configure command is registered");
        let help = configure.clone().render_long_help().to_string();
        assert!(help.contains("--source-root"));
        assert!(help.contains("--source-extension"));
        assert!(help.contains("--clear-sources"));
        let check = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "check")
            .expect("check command is registered");
        assert!(
            check
                .clone()
                .render_long_help()
                .to_string()
                .contains("--format")
        );
        let generate = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "generate")
            .expect("generate command is registered");
        let help = generate.clone().render_long_help().to_string();
        assert!(help.contains("--install-host"));
    }

    #[test]
    fn clap_parses_lock_project_option() {
        let cli = Cli::try_parse_from(["langmeld", "lock", "--project", "workspace.json"])
            .expect("lock command options parse");
        let Commands::Lock(arguments) = cli.command else {
            panic!("lock command parsed as another subcommand");
        };
        assert_eq!(arguments.project, PathBuf::from("workspace.json"));
    }

    #[test]
    fn generate_rejects_check_with_host_install() {
        assert!(
            Cli::try_parse_from(["langmeld", "generate", "--check", "--install-host"]).is_err()
        );
    }

    #[test]
    fn configure_rejects_conflicting_source_options() {
        assert!(
            Cli::try_parse_from([
                "langmeld",
                "configure",
                "--source-root",
                "examples",
                "--clear-source-roots"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "langmeld",
                "configure",
                "--source-extension",
                "calc",
                "--inherit-source-extensions"
            ])
            .is_err()
        );
    }

    #[test]
    fn check_alias_accepts_validate_and_requires_one_input_mode() {
        assert!(
            Cli::try_parse_from([
                "langmeld",
                "validate",
                "--language",
                "language.langmeld",
                "--source",
                "input.calc"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from([
                "langmeld",
                "check",
                "--project",
                "langmeld.json",
                "--source",
                "input.calc"
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["langmeld", "check"]).is_err());
    }
}
