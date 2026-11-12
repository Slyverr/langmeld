use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::Parser;
use langmeld_brainfuck_example::run_with_reader;
use langmeld_core::SourceFile;

const SAMPLE: &str = include_str!("../hello.bf");

#[derive(Debug, Parser)]
#[command(version, about = "Run a Brainfuck program.")]
struct Cli {
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<(), String> {
    let cli = Cli::parse();
    let (source_name, code) = match cli.source {
        Some(path) => {
            let code = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;

            (path.display().to_string(), code)
        }
        None => ("hello.bf".to_owned(), SAMPLE.to_owned()),
    };
    let source = SourceFile::new(source_name, code);
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let output = run_with_reader(&source, &mut input)?;

    io::stdout()
        .write_all(&output)
        .map_err(|error| format!("cannot write program output: {error}"))
}
