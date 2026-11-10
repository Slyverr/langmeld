use langmeld_core::SourceFile;
use langmeld_language::{parse, parse_language_definition_project};
use langmeld_project::DirectoryDefinitionResolver;
use std::path::Path;

const PROGRAM: &str = "program { let title = \"Hello\"; draw title; }";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root_key = "language/toy.lm";
    let resolver =
        DirectoryDefinitionResolver::new(project_root).map_err(|error| error.to_string())?;
    let root = resolver.load(root_key).map_err(|error| error.to_string())?;
    let definition = parse_language_definition_project(root, &resolver);
    if !definition.is_valid() {
        return Err(format_diagnostics(
            &resolver.sources(),
            &definition.diagnostics,
        ));
    }
    let language = definition
        .language
        .ok_or_else(|| "definition did not produce a language".to_owned())?;

    let program_source = SourceFile::new("main.widget", PROGRAM);
    let parsed = parse(&program_source, &language);
    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics(&[program_source], &parsed.diagnostics));
    }

    println!("Language: {}", language.name());
    println!("Root construct: {}", parsed.constructs[0].name);
    for construct in &parsed.constructs[0].children {
        println!("  {} {:?}", construct.name, construct.fields);
    }
    Ok(())
}

fn format_diagnostics(sources: &[SourceFile], diagnostics: &[langmeld_core::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let location = diagnostic.location.as_ref();
            let source = location.and_then(|location| {
                sources
                    .iter()
                    .find(|source| source.name == location.source_name)
            });
            let (line, column) = match (source, location) {
                (Some(source), Some(location)) => source.line_column(location.span.start),
                _ => (1, 1),
            };
            let source_name = location
                .map(|location| location.source_name.as_str())
                .unwrap_or("<unknown>");
            format!(
                "{}:{line}:{column}: {}: {}",
                source_name, diagnostic.code, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
