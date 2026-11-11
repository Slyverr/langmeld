use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Write},
    path::{Component, Path, PathBuf},
};

use clap::{Args, ValueEnum};
use dialoguer::{Input, Select, theme::ColorfulTheme};

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

    create_project(&args.path, &package_name, &language_name, template)?;
    let template_name = match template {
        Template::Declarative => "declarative grammar",
        Template::CustomParser => "custom Rust parser",
    };
    println!(
        "Created `{package_name}` at `{}` with a {template_name}.",
        args.path.display()
    );
    println!("Next: run `cargo run` from the project directory.");
    if matches!(template, Template::CustomParser) {
        println!("LSP: run `cargo run --bin language-lsp` from the project directory.");
    } else {
        println!("Editor: run `langmeld lsp` from the project directory for syntax support.");
    }
    Ok(())
}

fn create_project(
    path: &Path,
    package_name: &str,
    language_name: &str,
    template: Template,
) -> io::Result<()> {
    let files = project_files(package_name, language_name, template);
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

fn project_files(
    package_name: &str,
    language_name: &str,
    template: Template,
) -> Vec<(String, String)> {
    let version = env!("CARGO_PKG_VERSION");
    let dependencies = match template {
        Template::Declarative => format!(
            "langmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-project = \"{version}\"\n"
        ),
        Template::CustomParser => {
            format!(
                "langmeld-core = \"{version}\"\nlangmeld-language = \"{version}\"\nlangmeld-lsp = \"{version}\"\n"
            )
        }
    };
    let library = "\n[lib]\nname = \"language\"\n";
    let manifest = format!(
        "[package]\nname = {package_name:?}\ndefault-run = {package_name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.88\"\nlicense = \"MIT OR Apache-2.0\"\n{library}\n[dependencies]\n{dependencies}\n[workspace]\n"
    );
    let mut files = vec![
        ("Cargo.toml".to_owned(), manifest),
        (".gitignore".to_owned(), "/target/\n".to_owned()),
        (
            "LICENSE-MIT".to_owned(),
            include_str!("../templates/LICENSE-MIT").to_owned(),
        ),
        (
            "LICENSE-APACHE".to_owned(),
            include_str!("../templates/LICENSE-APACHE").to_owned(),
        ),
    ];

    match template {
        Template::Declarative => {
            files.extend([
                (
                    "README.md".to_owned(),
                    include_str!("../templates/declarative/README.md")
                        .replace("{{LANGUAGE_NAME}}", language_name),
                ),
                ("langmeld.toml".to_owned(), "format = 1\n".to_owned()),
                (
                    "src/lib.rs".to_owned(),
                    include_str!("../templates/declarative/src/lib.rs").to_owned(),
                ),
                (
                    "src/main.rs".to_owned(),
                    include_str!("../templates/declarative/src/main.rs").to_owned(),
                ),
                (
                    "src/runtime.rs".to_owned(),
                    include_str!("../templates/declarative/src/runtime.rs").to_owned(),
                ),
                (
                    "src/constructs/mod.rs".to_owned(),
                    include_str!("../templates/declarative/src/constructs/mod.rs").to_owned(),
                ),
                (
                    "src/constructs/let_handler.rs".to_owned(),
                    include_str!("../templates/declarative/src/constructs/let_handler.rs")
                        .to_owned(),
                ),
                (
                    "src/constructs/print.rs".to_owned(),
                    include_str!("../templates/declarative/src/constructs/print.rs").to_owned(),
                ),
                (
                    "tests/language.rs".to_owned(),
                    include_str!("../templates/declarative/tests/language.rs")
                        .replace("{{LANGUAGE_NAME}}", &format!("{language_name:?}")),
                ),
                (
                    "language/main.langmeld".to_owned(),
                    include_str!("../templates/declarative/language/main.langmeld")
                        .replace("{{LANGUAGE_NAME}}", &quote_lm(language_name)),
                ),
                (
                    "language/constructs/statements.langmeld".to_owned(),
                    include_str!(
                        "../templates/declarative/language/constructs/statements.langmeld"
                    )
                    .to_owned(),
                ),
            ]);
        }
        Template::CustomParser => {
            files.extend([
                ("README.md".to_owned(), custom_parser_readme(language_name)),
                (
                    "src/lib.rs".to_owned(),
                    custom_parser_library_template().to_owned(),
                ),
                (
                    "src/main.rs".to_owned(),
                    custom_parser_main_template(language_name),
                ),
                (
                    "src/bin/language-lsp.rs".to_owned(),
                    custom_parser_lsp_template().to_owned(),
                ),
                (
                    "tests/parser.rs".to_owned(),
                    custom_parser_test_template().to_owned(),
                ),
            ]);
        }
    }

    files
}

fn custom_parser_readme(language_name: &str) -> String {
    format!(
        r#"# {language_name}

A Rust-defined language parser built with Langmeld.

## Run

```sh
cargo run
cargo run -- path/to/program.say
cargo test
```

Without a path, the CLI parses a small built-in sample. Pass a source file to
parse your own program. The parser and AST live in `src/lib.rs`; the CLI entry
point is `src/main.rs`, and public behavior tests belong in `tests/`. This
template uses Langmeld's `LanguageParser` contract and leaves grammar design
to your Rust code.

## Language server

Start the stdio LSP server with:

```sh
cargo run --bin language-lsp
```

It publishes parser diagnostics from the host parser. Add host analysis,
completion, and navigation by implementing Langmeld's corresponding traits
and attaching them to `HostLanguageBackend` in `src/bin/language-lsp.rs`.

## License

Licensed under either the MIT License or Apache License 2.0; choose either
license. Replace the copyright placeholder in `LICENSE-MIT` before distributing.
"#
    )
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

fn custom_parser_library_template() -> &'static str {
    r#"use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

pub struct SayParser;

impl LanguageParser for SayParser {
    type Tree = Vec<String>;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let mut statements = Vec::new();
        let mut diagnostics = Vec::new();
        let mut line_start = 0;

        for line_with_ending in source.text.split_inclusive('\n') {
            let line = line_with_ending
                .strip_suffix('\n')
                .unwrap_or(line_with_ending);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let leading = line.len() - line.trim_start().len();
            let content = line.trim();
            if !content.is_empty() {
                if let Some(message) = content.strip_prefix("say ") {
                    if message.trim().is_empty() {
                        diagnostics.push(Diagnostic::error(
                            "EXAMPLE-001",
                            "expected text after `say`",
                            source,
                            Span::new(line_start + leading, line_start + line.len()),
                        ));
                    } else {
                        statements.push(message.trim().to_owned());
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        "EXAMPLE-001",
                        "expected `say <text>`",
                        source,
                        Span::new(line_start + leading, line_start + line.len()),
                    ));
                }
            }
            line_start += line_with_ending.len();
        }

        ParsedDocument::new(source, statements, diagnostics)
    }
}
"#
}

fn custom_parser_main_template(language_name: &str) -> String {
    r#"use std::{env, fs, path::PathBuf};

use langmeld_core::SourceFile;
use langmeld_language::LanguageParser;
use language::SayParser;

const LANGUAGE_NAME: &str = $LANGUAGE_NAME;
const SAMPLE: &str = "say Hello from a custom parser!\n";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let (source_name, source_text) = match arguments.next() {
        Some(path) => {
            if arguments.next().is_some() {
                return Err("usage: language [SOURCE]".to_owned());
            }
            let path = PathBuf::from(path);
            let source_text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
            (path.display().to_string(), source_text)
        }
        None => ("example.say".to_owned(), SAMPLE.to_owned()),
    };

    let source = SourceFile::new(source_name, source_text);
    let parsed = SayParser.parse(&source);

    if !parsed.is_valid() {
        let diagnostics = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let location = diagnostic.location.as_ref();
                let source_name = location
                    .map(|location| location.source_name.as_str())
                    .unwrap_or(&source.name);
                let (line, column) = location
                    .map(|location| source.line_column(location.span.start))
                    .unwrap_or((1, 1));
                format!(
                    "{source_name}:{line}:{column}: {}: {}",
                    diagnostic.code, diagnostic.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(diagnostics);
    }

    let count = parsed.tree.len();
    for message in parsed.tree {
        println!("{message}");
    }
    println!("Parsed {count} statement(s) for {LANGUAGE_NAME}.");
    Ok(())
}
"#
    .replace("$LANGUAGE_NAME", &format!("{language_name:?}"))
}

fn custom_parser_lsp_template() -> &'static str {
    r#"use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, run_stdio};
use language::SayParser;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    run_stdio(HostLanguageBackend::new(SayParser))
}
"#
}

fn custom_parser_test_template() -> &'static str {
    r#"use langmeld_core::{SourceFile, Span};
use langmeld_language::LanguageParser;
use language::SayParser;

#[test]
fn parses_a_valid_say_statement() {
    let source = SourceFile::new("valid.say", "say hello\n");
    let parsed = SayParser.parse(&source);

    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.tree, ["hello"]);
}

#[test]
fn reports_byte_correct_diagnostics_after_crlf_lines() {
    let source = SourceFile::new("example.say", "say ok\r\ninvalid\r\n");
    let parsed = SayParser.parse(&source);
    let diagnostic = parsed
        .diagnostics
        .first()
        .expect("invalid line should be diagnosed");
    let location = diagnostic
        .location
        .as_ref()
        .expect("diagnostic should have a source span");

    assert_eq!(location.span, Span::new(8, 15));
    assert_eq!(source.line_column(location.span.start), (2, 1));
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
