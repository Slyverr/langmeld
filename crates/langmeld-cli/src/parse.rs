//! `langmeld parse`.

use std::path::Path;
use std::process::ExitCode;

use langmeld_meta::corpus::render_parse;
use langmeld_meta::{Source, load_language};
use langmeld_syntax::{FileId, Severity, SourceFile, parse};

use crate::project::shown;

pub(crate) fn run(grammar: &Path, file: &Path) -> ExitCode {
    let loaded = match load_language(Source::path(grammar).generic()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprint!("{}", error.rendered());
            eprintln!(
                "error: {} has problems, so it cannot parse files",
                shown(grammar)
            );
            return ExitCode::FAILURE;
        }
    };
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read `{}`: {error}", shown(file));
            return ExitCode::from(2);
        }
    };
    let source = match SourceFile::new(FileId::new(0), shown(file), text) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: `{}`: {error}", shown(file));
            return ExitCode::from(2);
        }
    };
    let parsed = parse(&loaded.language, &source);
    println!("{}", render_parse(&loaded.language, &parsed));
    if parsed
        .diagnostics
        .iter()
        .any(|found| found.severity == Severity::Error)
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
