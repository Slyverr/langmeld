//! `langmeld test`.

use std::path::Path;
use std::process::ExitCode;

use langmeld_cli::{Failure, error_line, run_corpus};
use langmeld_meta::{Source, load_language};

use crate::project::{self, shown};

/// The name of the folder of corpus files, inside the project.
const CORPUS: &str = "corpus";

pub(crate) fn run(path: &Path, update: bool) -> ExitCode {
    let found = match project::find(path) {
        Ok(found) => found,
        Err(refused) => {
            eprintln!("{}", refused.0.trim_end());
            return ExitCode::from(2);
        }
    };
    let loaded = match load_language(Source::path(&found.root).generic()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprint!("{}", error.rendered());
            eprintln!(
                "error: {} has problems, so its corpus cannot be run",
                shown(&found.root)
            );
            return ExitCode::FAILURE;
        }
    };
    match run_corpus(&loaded.language, &found.dir.join(CORPUS), update) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if let Failure::Unable(reason) = &failure {
                error_line(&format!("error: {reason}"));
            }
            ExitCode::from(failure.code())
        }
    }
}
