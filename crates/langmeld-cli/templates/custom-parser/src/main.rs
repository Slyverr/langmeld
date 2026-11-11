use std::{fs, path::PathBuf};

use clap::Parser;
use langmeld_core::{SourceFile, format_diagnostics};
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
        return Err(format_diagnostics(
            std::slice::from_ref(&source),
            parsed.diagnostics(),
        ));
    }

    let count = parsed.tree().len();
    for message in parsed.tree() {
        println!("{message}");
    }
    println!("Parsed {count} statement(s) for {LANGUAGE_NAME}.");
    Ok(())
}
