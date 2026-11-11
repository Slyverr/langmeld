use std::{fs, path::PathBuf};

use clap::Parser;
use langmeld_core::SourceFile;
use langmeld_language::LanguageParser;
use language::SayParser;

const LANGUAGE_NAME: &str = {{LANGUAGE_NAME}};
const SAMPLE: &str = "say Hello from a custom parser!\n";

#[derive(Debug, Parser)]
#[command(version, about = "Run the generated language parser.")]
struct Cli {
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let (source_name, source_text) = match cli.source {
        Some(path) => {
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
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                let location = diagnostic.location.as_ref();
                let source_name = location
                    .map(|location| location.source_name())
                    .unwrap_or(source.name());
                let (line, column) = location
                    .map(|location| source.line_column(location.span().start))
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

    let count = parsed.tree().len();
    for message in parsed.tree() {
        println!("{message}");
    }
    println!("Parsed {count} statement(s) for {LANGUAGE_NAME}.");
    Ok(())
}
