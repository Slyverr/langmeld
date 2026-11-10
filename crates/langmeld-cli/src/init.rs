use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Write},
    path::{Component, Path, PathBuf},
};

use clap::Args;
use dialoguer::{Input, theme::ColorfulTheme};

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Directory to create the project in.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Cargo package name. Defaults to the directory name.
    #[arg(long)]
    name: Option<String>,
    /// Display name used in the language definition.
    #[arg(long)]
    language_name: Option<String>,
    /// Skip prompts and use defaults for omitted names.
    #[arg(long)]
    non_interactive: bool,
}

pub(crate) fn run(args: InitArgs) -> Result<(), Box<dyn Error>> {
    let project_name_default = default_project_name(&args.path);
    let interactive =
        !args.non_interactive && io::stdin().is_terminal() && io::stdout().is_terminal();

    let package_name = match args.name {
        Some(name) => name,
        None if interactive => Input::<String>::with_theme(&ColorfulTheme::default())
            .with_prompt("Project name")
            .default(project_name_default.clone())
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
            .interact_text()?,
        None => language_name_default,
    };
    validate_language_name(&language_name)?;

    create_project(&args.path, &package_name, &language_name)?;
    println!("Created Langmeld language project `{package_name}`.");
    println!(
        "Next: cargo run --manifest-path {}/Cargo.toml",
        args.path.display()
    );
    Ok(())
}

fn create_project(path: &Path, package_name: &str, language_name: &str) -> io::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let files = [
        (
            "Cargo.toml",
            format!(
                "[package]\nname = {package_name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.88\"\nlicense = \"MIT OR Apache-2.0\"\n\n[dependencies]\nlangmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-project = \"{version}\"\n\n[workspace]\n"
            ),
        ),
        (
            ".gitignore",
            "/target/\n".to_owned(),
        ),
        (
            "README.md",
            format!(
                "# {language_name}\n\nA Rust-hosted language project built with Langmeld.\n\n## Run\n\n```sh\ncargo run\n```\n\nEdit `language/language.lm` and files in `language/constructs/` to change\nthe syntax. Put language-specific analysis and execution in `src/main.rs` or\nmove them into Rust modules as the project grows.\n"
            ),
        ),
        (
            "langmeld.toml",
            "format = 1\ndefinition = \"language/language.lm\"\n".to_owned(),
        ),
        (
            "src/main.rs",
            main_template().to_owned(),
        ),
        (
            "language/language.lm",
            format!(
                "language {} {{\n    import \"constructs/statements.lm\";\n\n    construct \"program\" {{\n        syntax {{\n            literal \"program\";\n        }}\n        block \"{{\" \"}}\" {{\n            allow \"let\";\n            allow \"print\";\n        }}\n    }}\n}}\n",
                quote_lm(language_name)
            ),
        ),
        (
            "language/constructs/statements.lm",
            "construct \"let\" {\n    syntax {\n        literal \"let\";\n        identifier \"name\";\n        literal \"=\";\n        string \"value\";\n        literal \";\";\n    }\n}\n\nconstruct \"print\" {\n    syntax {\n        literal \"print\";\n        identifier \"name\";\n        literal \";\";\n    }\n}\n"
                .to_owned(),
        ),
    ];

    if path.exists() && !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("project path `{}` is not a directory", path.display()),
        ));
    }
    if path.exists() {
        for (relative, _) in &files {
            if path.join(relative).exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "`{}` already exists; no files were changed",
                        path.join(relative).display()
                    ),
                ));
            }
        }
    }

    fs::create_dir_all(path)?;
    let root = path.canonicalize()?;
    let mut created_files = Vec::new();
    let mut created_directories = Vec::new();
    let write_result = (|| {
        for (relative, contents) in files {
            let destination = root.join(relative);
            let parent = destination.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "generated file has no parent")
            })?;
            ensure_parent_directories(&root, parent, &mut created_directories)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            if let Err(error) = file.write_all(contents.as_bytes()) {
                drop(file);
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
            created_files.push(destination);
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        for file in created_files.iter().rev() {
            let _ = fs::remove_file(file);
        }
        for directory in created_directories.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
        return Err(error);
    }
    Ok(())
}

fn ensure_parent_directories(
    root: &Path,
    parent: &Path,
    created: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let relative = parent.strip_prefix(root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "generated path escaped the project root",
        )
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "generated path contains an invalid directory component",
            ));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing to write through symlink `{}`", current.display()),
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("`{}` exists and is not a directory", current.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                created.push(current.clone());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn main_template() -> &'static str {
    r#"use std::path::Path;

use langmeld_core::SourceFile;
use langmeld_language::{parse, parse_language_definition_project};
use langmeld_project::{DirectoryDefinitionResolver, read_project_manifest};

const SAMPLE: &str = "program { let message = \"Hello, Langmeld!\"; print message; }";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolver = DirectoryDefinitionResolver::new(project_root)
        .map_err(|error| error.to_string())?;
    let manifest = read_project_manifest(project_root).map_err(|error| error.to_string())?;
    let root = resolver
        .load(&manifest.definition)
        .map_err(|error| error.to_string())?;
    let definition = parse_language_definition_project(root, &resolver);
    if !definition.is_valid() {
        return Err(format_diagnostics(&resolver.sources(), &definition.diagnostics));
    }
    let language = definition
        .language
        .ok_or_else(|| "validated definition did not produce a language".to_owned())?;

    let sample = SourceFile::new("example.widget", SAMPLE);
    let parsed = parse(&sample, &language);
    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics(&[sample], &parsed.diagnostics));
    }

    println!("Parsed {} top-level construct(s) for {}.", parsed.constructs.len(), language.name());
    Ok(())
}

fn format_diagnostics(
    sources: &[SourceFile],
    diagnostics: &[langmeld_core::Diagnostic],
) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let location = diagnostic.location.as_ref();
            let source = location.and_then(|location| {
                sources.iter().find(|source| source.name == location.source_name)
            });
            let (line, column) = match (source, location) {
                (Some(source), Some(location)) => source.line_column(location.span.start),
                _ => (1, 1),
            };
            let source_name = location
                .map(|location| location.source_name.as_str())
                .unwrap_or("<unknown>");
            format!("{source_name}:{line}:{column}: {}: {}", diagnostic.code, diagnostic.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
"#
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
    let normalized = normalized.trim_matches('-');
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

fn quote_lm(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}
