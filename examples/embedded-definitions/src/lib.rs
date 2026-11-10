use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{ParseResult, parse, parse_language_definition_project};
use langmeld_project::InMemoryDefinitionResolver;

const ROOT_KEY: &str = "language/main.langmeld";
const DRAWING_KEY: &str = "language/extensions/drawing.langmeld";
const DRAWING_DEFINITION: &str = include_str!("../language/extensions/drawing.langmeld");
const PROGRAM: &str = "program { draw circle; }";

/// Loads a project definition with a Rust-embedded module and parses its sample.
pub fn parse_example() -> Result<ParseResult, String> {
    let root_source = SourceFile::new(ROOT_KEY, include_str!("../language/main.langmeld"));
    let mut resolver = InMemoryDefinitionResolver::new();
    resolver
        .insert(ROOT_KEY, root_source)
        .map_err(|error| error.to_string())?;
    resolver
        .insert(
            DRAWING_KEY,
            SourceFile::new(DRAWING_KEY, DRAWING_DEFINITION),
        )
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
        return Err(format!(
            "invalid example program:\n{}",
            format_diagnostics(std::slice::from_ref(&program_source), &parsed.diagnostics)
        ));
    }
    Ok(parsed)
}
