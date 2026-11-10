use std::{fs, path::Path};

use langmeld_aster_language_example::{analyze, parse_program};
use langmeld_core::{Diagnostic, SourceFile};

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
    let analysis = analyze(&source, &parsed.tree);
    let diagnostics = parsed
        .diagnostics
        .into_iter()
        .chain(analysis.diagnostics)
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        return Err(format_diagnostics(&source, &diagnostics));
    }

    println!(
        "Aster: {} function(s), {} symbol(s), {} resolved reference(s)",
        parsed.tree.functions.len(),
        analysis.symbols.len(),
        analysis
            .references
            .iter()
            .filter(|reference| reference.target.is_some())
            .count()
    );
    Ok(())
}

fn format_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let (line, column) = diagnostic
                .location
                .as_ref()
                .map(|location| source.line_column(location.span.start))
                .unwrap_or((1, 1));
            format!(
                "{}:{line}:{column}: {}: {}",
                source.name, diagnostic.code, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
