use std::{fs, path::Path};

use langmeld_aster_language_example::{AsterAnalyzer, parse_program};
use langmeld_core::{SourceFile, format_diagnostics, has_errors};
use langmeld_language::LanguageAnalyzer;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.aster");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let source = SourceFile::new("src/main.aster", text);

    let parsed = parse_program(&source);
    let analysis = AsterAnalyzer.analyze(&source, &parsed);
    let diagnostics = parsed
        .diagnostics
        .into_iter()
        .chain(analysis.diagnostics)
        .collect::<Vec<_>>();

    if !diagnostics.is_empty() {
        let formatted = format_diagnostics(std::slice::from_ref(&source), &diagnostics);

        if has_errors(&diagnostics) {
            return Err(formatted);
        }

        eprintln!("{formatted}");
    }

    println!(
        "Aster: {} function(s), {} symbol(s), {} resolved reference(s)",
        parsed.tree.functions.len(),
        analysis.result.symbols.len(),
        analysis
            .result
            .references
            .iter()
            .filter(|reference| reference.target.is_some())
            .count()
    );
    Ok(())
}
