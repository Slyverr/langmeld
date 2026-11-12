use std::{
    error::Error,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};
use dialoguer::{Confirm, Input, MultiSelect, Select, theme::ColorfulTheme};

mod local_crates;
mod scaffold;
mod templates;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum Template {
    #[default]
    Declarative,
    CustomParser,
}

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Directory to create the project in.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Cargo package name. Defaults to the directory name.
    #[arg(long)]
    name: Option<String>,
    /// Display name used by the generated language host.
    #[arg(long)]
    language_name: Option<String>,
    /// Syntax implementation to scaffold. Defaults to `declarative`.
    #[arg(long, value_enum)]
    template: Option<Template>,
    /// Skip prompts and use defaults for omitted names.
    #[arg(long)]
    non_interactive: bool,
    /// Include a project-specific AGENTS.md file.
    #[arg(long)]
    agents_md: bool,
    /// Include a Langmeld authoring skill under `.agents/skills/`.
    #[arg(long)]
    agent_skill: bool,
    /// Use crates from the current Langmeld source checkout for local development.
    #[arg(long)]
    local_crates: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AgentGuidance {
    pub(super) agents_md: bool,
    pub(super) skill: bool,
}

pub(crate) fn run(args: InitArgs) -> Result<(), Box<dyn Error>> {
    let project_name_default = default_project_name(&args.path);
    let interactive =
        !args.non_interactive && io::stdin().is_terminal() && io::stdout().is_terminal();

    let template = match args.template {
        Some(template) => template,
        None if interactive => {
            let choices = [
                "Declarative `.langmeld` grammar with a Rust host",
                "Custom Rust parser with full syntax control",
            ];
            match Select::with_theme(&ColorfulTheme::default())
                .with_prompt("How do you want to define the syntax?")
                .items(choices)
                .default(0)
                .interact()?
            {
                0 => Template::Declarative,
                _ => Template::CustomParser,
            }
        }
        None => Template::default(),
    };

    let package_name = match args.name {
        Some(name) => name,
        None if interactive => Input::<String>::with_theme(&ColorfulTheme::default())
            .with_prompt("Project name")
            .default(project_name_default.clone())
            .validate_with(|name: &String| {
                validate_package_name(name.as_str()).map_err(|error| error.to_string())
            })
            .interact_text()?,
        None => project_name_default,
    };
    validate_package_name(&package_name)?;

    let language_name_default = default_language_name(&package_name);
    let language_name = match args.language_name {
        Some(name) => name,
        None if interactive => Input::<String>::with_theme(&ColorfulTheme::default())
            .with_prompt("Language name")
            .default(language_name_default)
            .validate_with(|name: &String| {
                validate_language_name(name.as_str()).map_err(|error| error.to_string())
            })
            .interact_text()?,
        None => language_name_default,
    };
    validate_language_name(&language_name)?;

    let local_crate_root = select_local_crates(args.local_crates, interactive)?;
    let agent_guidance = agent_guidance(args.agents_md, args.agent_skill, interactive)?;
    scaffold::create_project(
        &args.path,
        &package_name,
        &language_name,
        template,
        agent_guidance,
        local_crate_root.as_deref(),
    )?;
    let template_name = match template {
        Template::Declarative => "declarative grammar",
        Template::CustomParser => "custom Rust parser",
    };
    println!(
        "Created `{package_name}` at `{}` with a {template_name}.",
        args.path.display()
    );

    if let Some(root) = &local_crate_root {
        println!(
            "Using local Langmeld crates from `{}`; the Cargo override is gitignored.",
            root.display()
        );
    }

    println!("Next: run `cargo run` from the project directory.");
    if matches!(template, Template::CustomParser) {
        println!("LSP: run `cargo run --bin language-lsp` from the project directory.");
    } else {
        println!("Editor: run `langmeld lsp` from the project directory for syntax support.");
    }
    if agent_guidance.agents_md {
        println!("Agent instructions: created `AGENTS.md`.");
    }
    if agent_guidance.skill {
        println!("Langmeld authoring skill: created `.agents/skills/langmeld-language/SKILL.md`.");
    }
    Ok(())
}

fn select_local_crates(
    requested: bool,
    interactive: bool,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let workspace_root = local_crates::workspace_root();
    let Some(root) = workspace_root else {
        if requested {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "`--local-crates` requires running Langmeld from its source checkout",
            )
            .into());
        }

        return Ok(None);
    };

    if requested {
        return Ok(Some(root));
    }

    if interactive
        && Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Use local Langmeld crates from `{}` for development?\nThis writes a Git-ignored `.cargo/config.toml` that requires this checkout.",
                root.display()
            ))
            .default(true)
            .interact()?
    {
        return Ok(Some(root));
    }

    Ok(None)
}

fn agent_guidance(
    agents_md: bool,
    skill: bool,
    interactive: bool,
) -> Result<AgentGuidance, Box<dyn Error>> {
    if agents_md || skill {
        return Ok(AgentGuidance { agents_md, skill });
    }

    if !interactive {
        return Ok(AgentGuidance::default());
    }

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Optional agent files (space to select, enter to continue)")
        .items(["AGENTS.md project instructions", "Langmeld authoring skill"])
        .interact()?;

    Ok(AgentGuidance {
        agents_md: selections.contains(&0),
        skill: selections.contains(&1),
    })
}

fn default_project_name(path: &Path) -> String {
    let name = path
        .file_name()
        .filter(|name| *name != "." && *name != "..")
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| {
            std::env::current_dir().ok().and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
        })
        .unwrap_or_else(|| "my-language".to_owned());
    let mut normalized = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            normalized.push(character.to_ascii_lowercase());
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches(|character| matches!(character, '-' | '_'));
    if normalized.is_empty() {
        "my-language".to_owned()
    } else {
        normalized.to_owned()
    }
}

fn default_language_name(package_name: &str) -> String {
    package_name
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn validate_package_name(name: &str) -> io::Result<()> {
    let valid = !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project names must start with a letter or digit and contain only letters, digits, `-`, or `_`",
        ))
    }
}

fn validate_language_name(name: &str) -> io::Result<()> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "language name must not be empty or contain control characters",
        ))
    } else {
        Ok(())
    }
}
