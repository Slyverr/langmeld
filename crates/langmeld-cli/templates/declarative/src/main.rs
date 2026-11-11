use std::{fs, path::PathBuf};

use clap::Parser;
use langmeld_core::SourceFile;
use language::load;

const SAMPLE: &str = "program { let message = \"Hello, Langmeld!\"; print message; }";

#[derive(Debug, Parser)]
#[command(version, about = "Run the generated language.")]
struct Cli {
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let cli = Cli::parse();
    let (source_name, source_text) = match cli.source {
        Some(path) => {
            let source_text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;

            (path.display().to_string(), source_text)
        }
        None => ("example.widget".to_owned(), SAMPLE.to_owned()),
    };

    let source = SourceFile::new(source_name, source_text);
    let language = load()?;
    let output = language.run(&source)?;

    print!("{output}");

    Ok(())
}
