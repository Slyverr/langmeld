use super::{Template, quote_lm};

pub(super) fn project_files(
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
            include_str!("../../templates/LICENSE-MIT").to_owned(),
        ),
        (
            "LICENSE-APACHE".to_owned(),
            include_str!("../../templates/LICENSE-APACHE").to_owned(),
        ),
    ];

    match template {
        Template::Declarative => {
            files.extend([
                (
                    "README.md".to_owned(),
                    include_str!("../../templates/declarative/README.md")
                        .replace("{{LANGUAGE_NAME}}", language_name),
                ),
                ("langmeld.toml".to_owned(), "format = 1\n".to_owned()),
                (
                    "src/lib.rs".to_owned(),
                    include_str!("../../templates/declarative/src/lib.rs").to_owned(),
                ),
                (
                    "src/main.rs".to_owned(),
                    include_str!("../../templates/declarative/src/main.rs").to_owned(),
                ),
                (
                    "src/runtime.rs".to_owned(),
                    include_str!("../../templates/declarative/src/runtime.rs").to_owned(),
                ),
                (
                    "src/constructs/mod.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/mod.rs").to_owned(),
                ),
                (
                    "src/constructs/let_handler.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/let_handler.rs")
                        .to_owned(),
                ),
                (
                    "src/constructs/print.rs".to_owned(),
                    include_str!("../../templates/declarative/src/constructs/print.rs").to_owned(),
                ),
                (
                    "tests/language.rs".to_owned(),
                    include_str!("../../templates/declarative/tests/language.rs")
                        .replace("{{LANGUAGE_NAME}}", &format!("{language_name:?}")),
                ),
                (
                    "language/main.langmeld".to_owned(),
                    include_str!("../../templates/declarative/language/main.langmeld")
                        .replace("{{LANGUAGE_NAME}}", &quote_lm(language_name)),
                ),
                (
                    "language/constructs/statements.langmeld".to_owned(),
                    include_str!(
                        "../../templates/declarative/language/constructs/statements.langmeld"
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
