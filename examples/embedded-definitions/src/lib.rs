use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{
    DefinitionResolver, DefinitionSource, ParseResult, parse, parse_language_definition_project,
};

const ROOT_KEY: &str = "language/main.langmeld";
const DRAWING_KEY: &str = "vendor/geometry-drawing/drawing.langmeld";
const DRAWING_DEFINITION: &str = include_str!("../language/extensions/drawing.langmeld");
const PROGRAM: &str = "program { draw circle; }";

struct BundledDefinitions;

impl DefinitionResolver for BundledDefinitions {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        match (from_key, import) {
            (ROOT_KEY, "extensions/drawing.langmeld") => Ok(DefinitionSource::new(
                DRAWING_KEY,
                SourceFile::new(DRAWING_KEY, DRAWING_DEFINITION),
            )),
            _ => Err(format!(
                "unknown embedded definition `{import}` from `{from_key}`"
            )),
        }
    }
}

/// Loads a project definition with a Rust-embedded module and parses its sample.
pub fn parse_example() -> Result<ParseResult, String> {
    let root_source = SourceFile::new(ROOT_KEY, include_str!("../language/main.langmeld"));
    let sources = [
        root_source.clone(),
        SourceFile::new(DRAWING_KEY, DRAWING_DEFINITION),
    ];
    let root = DefinitionSource::new(ROOT_KEY, root_source);
    let definition = parse_language_definition_project(root, &BundledDefinitions);
    if !definition.is_valid() {
        return Err(format!(
            "invalid language definition:\n{}",
            format_diagnostics(&sources, &definition.diagnostics)
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
