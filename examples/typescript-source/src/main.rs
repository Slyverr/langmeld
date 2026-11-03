use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;
use langmeld::core::SourceFile;
use langmeld::language::Value;
use typescript_source_language::{analyze_source, has_errors};

fn main() -> ExitCode {
    let Args { path } = Args::parse();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read `{}`: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let source = SourceFile::new(path.display().to_string(), text);
    let analysis = analyze_source(&source);
    let Some(root) = analysis.parsed.program.nodes.first() else {
        print_diagnostics(&analysis.diagnostics);
        return ExitCode::FAILURE;
    };
    if has_errors(&analysis) {
        print_diagnostics(&analysis.diagnostics);
        return ExitCode::FAILURE;
    }
    println!(
        "{}: {} TypeScript functions indexed",
        source.name,
        root.children.len()
    );
    for function in &root.children {
        let name = match function.fields.get("name").map(|field| &field.value) {
            Some(Value::Identifier(name)) => name.as_str(),
            _ => "<unnamed>",
        };
        println!("  {name} ({} local calls)", function.children.len());
    }
    print_diagnostics(&analysis.diagnostics);
    ExitCode::SUCCESS
}

#[derive(Debug, Parser)]
#[command(about = "Index functions and local calls in a TypeScript file.")]
struct Args {
    /// TypeScript source file.
    path: PathBuf,
}

fn print_diagnostics(diagnostics: &[langmeld::core::Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
}
