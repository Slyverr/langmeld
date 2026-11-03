use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use console::style;
use inquire::{Confirm, Text};

use crate::{
    cli::InitArgs,
    scaffold::{ProjectOptions, create_project, valid_language_name},
};

pub(crate) fn initialize(arguments: InitArgs) -> Result<(), String> {
    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    let interactive = arguments.interactive || (!arguments.no_interactive && terminal);
    if terminal && interactive {
        print_heading();
    }

    let path = if let Some(path) = arguments.path {
        path
    } else if interactive {
        PathBuf::from(prompt_text("Project directory", ".")?)
    } else {
        PathBuf::from(".")
    };
    let default_name = infer_language_name(&path)?;
    let name = if let Some(name) = arguments.name {
        name
    } else if interactive {
        prompt_text("Language name", &default_name)?
    } else {
        default_name
    };
    if !valid_language_name(&name) {
        return Err("language name must start with an ASCII letter and contain only ASCII letters or digits".to_owned());
    }

    let library_only = arguments.library_only
        || (interactive && !prompt_confirm("Include a command-line host?", true)?);

    let agents = if arguments.no_agents {
        false
    } else if arguments.agents || !interactive {
        true
    } else {
        prompt_confirm("Create a project AGENTS.md?", true)?
    };
    let agent_skill = if arguments.no_agent_skill {
        false
    } else if arguments.agent_skill {
        true
    } else if interactive {
        prompt_confirm("Add the local Langmeld agent skill?", false)?
    } else {
        false
    };

    let options = ProjectOptions {
        name: name.clone(),
        path: path.clone(),
        agents,
        agent_skill,
        langmeld_path: arguments.langmeld_path,
        library_only,
    };
    print_preview(&options);
    if interactive && !prompt_confirm("Create this project?", true)? {
        println!("Project creation cancelled; no files were written.");
        return Ok(());
    }

    create_project(options)
}

fn print_heading() {
    println!(
        "\n  {}  {}\n  {}\n",
        style("◈").cyan().bold(),
        style("LANGMELD").bold(),
        style("Build a language project with a Rust host and modular .lm definitions.").dim()
    );
}

fn print_preview(options: &ProjectOptions) {
    println!("\n{}", style("PROJECT PREVIEW").bold().cyan());
    println!(
        "  {}  {}",
        style("Language").dim(),
        style(&options.name).bold()
    );
    println!(
        "  {}  {}",
        style("Location").dim(),
        style(options.path.display()).cyan()
    );
    println!("  {}", style("Structure").dim());
    let mut entries = vec![
        "Cargo.toml",
        "language/language.lm",
        "language/constructs/program.lm",
        "src/lib.rs",
        "src/host/",
        "examples/hello.<extension>",
        "tests/language.rs",
        ".langmeld/generated/",
    ];
    if !options.library_only {
        entries.insert(4, "src/main.rs (language CLI)");
    }
    if options.agents {
        entries.push("AGENTS.md");
    }
    if options.agent_skill {
        entries.push(".agents/skills/langmeld/SKILL.md");
    }
    for (index, entry) in entries.iter().enumerate() {
        let branch = if index + 1 == entries.len() {
            "└─"
        } else {
            "├─"
        };
        println!("    {} {entry}", style(branch).dim());
    }
    println!(
        "  {}",
        style("The Rust host and language definition are ready to edit.").dim()
    );
}

pub(crate) fn prompt_text(message: &str, default: &str) -> Result<String, String> {
    if io::stdin().is_terminal() {
        return Text::new(message)
            .with_default(default)
            .prompt()
            .map_err(|error| format!("{message} prompt failed: {error}"));
    }
    print!("{message} [{default}]: ");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("cannot read {message}: {error}"))?;
    let input = input.trim();
    Ok(if input.is_empty() { default } else { input }.to_owned())
}

fn prompt_confirm(message: &str, default: bool) -> Result<bool, String> {
    if io::stdin().is_terminal() {
        return Confirm::new(message)
            .with_default(default)
            .prompt()
            .map_err(|error| format!("{message} prompt failed: {error}"));
    }
    let choice = if default { "Y/n" } else { "y/N" };
    print!("{message} [{choice}]: ");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("cannot read confirmation: {error}"))?;
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        "" => Ok(default),
        _ => Err("answer with y or n".to_owned()),
    }
}

fn infer_language_name(path: &Path) -> Result<String, String> {
    let path = if path.file_name().is_none() {
        std::env::current_dir()
            .map_err(|error| format!("cannot read current directory: {error}"))?
    } else {
        path.to_path_buf()
    };
    let name = path
        .file_name()
        .ok_or_else(|| "cannot infer a language name from the project directory".to_owned())?
        .to_string_lossy();
    let mut result = String::new();
    for part in name.split(|character: char| !character.is_ascii_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut characters = part.chars();
        if let Some(first) = characters.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(characters.map(|character| character.to_ascii_lowercase()));
        }
    }
    if result.is_empty() {
        return Err("cannot infer a language name from the project directory".to_owned());
    }
    Ok(result)
}
