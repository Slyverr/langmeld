use std::{env, fs, path::PathBuf};

use langmeld_core::SourceFile;
use langmeld_toy_language_example::load;

const SAMPLE: &str = "program { let title = \"Hello, Langmeld!\"; say title; draw circle labeled \"main target\"; emit \"first\", \"second\"; compute 20 + 4 * (2 + 1); }";

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
                return Err("usage: langmeld-toy-language-example [SOURCE]".to_owned());
            }

            let path = PathBuf::from(path);
            let source_text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;

            (path.display().to_string(), source_text)
        }
        None => ("example.widget".to_owned(), SAMPLE.to_owned()),
    };

    let source = SourceFile::new(source_name, source_text);
    let language = load()?;
    print!("{}", language.run(&source)?);

    Ok(())
}
