//! `langmeld check`.

use std::path::Path;
use std::process::ExitCode;

use langmeld_meta::{Loaded, Source, load_language, render_warnings};
use langmeld_syntax::Severity;

use crate::project::{self, shown};

/// What the grammar looks ahead for, as text.
fn explanation(loaded: &Loaded) -> String {
    let ahead = loaded.language.lookahead();
    if ahead.is_empty() {
        return "Lookahead: the next token settles every choice.\n".to_owned();
    }
    let mut out = format!(
        "Lookahead: the most tokens looked at to choose is {}. These choices need more than \
         one:\n",
        loaded.language.max_lookahead()
    );
    for place in &ahead {
        out.push_str(&format!(
            "  in `{}`, a choice that starts with `{}` looks {} tokens ahead\n",
            place.rule, place.starts, place.tokens
        ));
    }
    out
}

pub(crate) fn run(path: &Path, explain: bool) -> ExitCode {
    let found = match project::find(path) {
        Ok(found) => found,
        Err(refused) => {
            eprintln!("{}", refused.0.trim_end());
            return ExitCode::from(2);
        }
    };
    let root = shown(&found.root);
    match load_language(Source::path(&found.root).generic()) {
        Ok(loaded) => {
            let warnings = render_warnings(&loaded);
            eprint!("{warnings}");
            let count = loaded.language.warnings().len();
            println!(
                "{root}: language {} is valid ({} nodes, {count} warnings)",
                loaded.language.name(),
                loaded.language.schema().nodes().count()
            );
            if explain {
                print!("{}", explanation(&loaded));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprint!("{}", error.rendered());
            let count = error
                .diagnostics
                .iter()
                .filter(|found| found.severity == Severity::Error)
                .count();
            let noun = if count == 1 { "problem" } else { "problems" };
            eprintln!("error: {root} has {count} {noun}, so the language cannot be used");
            ExitCode::FAILURE
        }
    }
}
