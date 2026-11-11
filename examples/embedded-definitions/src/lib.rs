use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{ParseResult, parse, parse_language_definition_project};
use langmeld_project::{InMemoryDefinitionResolver, OverlayDefinitionResolver};

const ROOT_KEY: &str = "language/main.langmeld";
const PROGRAM: &str = "program { draw circle; }";

/// Loads a project definition with a Rust-embedded module and parses its sample.
pub fn parse_example() -> Result<ParseResult, String> {
    let mut resolver = InMemoryDefinitionResolver::new();
    for &(key, text) in geometry_language_extension_example::DEFINITION_SOURCES {
        resolver
            .insert(key, SourceFile::new(key, text))
            .map_err(|error| error.to_string())?;
    }
    let project_root = env!("CARGO_MANIFEST_DIR");
    let resolver = OverlayDefinitionResolver::new(project_root, resolver)
        .map_err(|error| error.to_string())?;
    let root = resolver.load(ROOT_KEY).map_err(|error| error.to_string())?;
    let definition = parse_language_definition_project(root, &resolver);
    if !definition.is_valid() {
        return Err(format!(
            "invalid language definition:\n{}",
            format_diagnostics(&resolver.sources(), &definition.diagnostics)
        ));
    }
    let language = definition
        .language
        .ok_or_else(|| "language definition produced no language".to_owned())?;

    let program_source = SourceFile::new("main.geometry", PROGRAM);
    let parsed = parse(&program_source, &language);
    if !parsed.diagnostics.is_empty() {
        let formatted =
            format_diagnostics(std::slice::from_ref(&program_source), &parsed.diagnostics);

        if !parsed.is_valid() {
            return Err(format!("invalid example program:\n{formatted}"));
        }

        eprintln!("{formatted}");
    }
    Ok(parsed)
}
