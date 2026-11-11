use std::{env, fs, path::PathBuf};

use langmeld_core::SourceFile;
use language::run;

const SAMPLE: &str = "program { let message = \"Hello, Langmeld!\"; print message; }";

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
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
        None => ("example.widget".to_owned(), SAMPLE.to_owned()),
    };

    let source = SourceFile::new(source_name, source_text);
    let output = run(&source)?;

    print!("{output}");

    Ok(())
}
