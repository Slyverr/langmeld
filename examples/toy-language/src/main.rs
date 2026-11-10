use langmeld_core::SourceFile;
use langmeld_language::{parse, parse_language_definition};

const LANGUAGE_DEFINITION: &str = include_str!("../language/toy.lm");
const PROGRAM: &str = "program { let title = \"Hello\"; draw title; }";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let definition_source = SourceFile::new("language/toy.lm", LANGUAGE_DEFINITION);
    let definition = parse_language_definition(&definition_source);
    if !definition.is_valid() {
        return Err(format_diagnostics(
            &definition_source,
            &definition.diagnostics,
        ));
    }
    let language = definition
        .language
        .ok_or_else(|| "definition did not produce a language".to_owned())?;

    let program_source = SourceFile::new("main.widget", PROGRAM);
    let parsed = parse(&program_source, &language);
    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics(&program_source, &parsed.diagnostics));
    }

    println!("Language: {}", language.name());
    println!("Root construct: {}", parsed.constructs[0].name);
    for construct in &parsed.constructs[0].children {
        println!("  {} {:?}", construct.name, construct.fields);
    }
    Ok(())
}

fn format_diagnostics(source: &SourceFile, diagnostics: &[langmeld_core::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let (line, column) = diagnostic
                .location
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
