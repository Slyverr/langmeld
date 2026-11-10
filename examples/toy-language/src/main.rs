use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{CapturedField, CapturedValue, parse, parse_language_definition_project};
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
    let root_key = "language/toy.langmeld";
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
        return Err(format_diagnostics(
            std::slice::from_ref(&program_source),
            &parsed.diagnostics,
        ));
    }

    println!("Language: {}", language.name());
    let root = parsed
        .constructs
        .first()
        .ok_or_else(|| "program did not produce a root construct".to_owned())?;
    println!("Root construct: {}", root.name);
    for construct in &root.children {
        let fields = construct
            .fields
            .iter()
            .map(format_field)
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {}: {fields}", construct.name);
    }
    Ok(())
}

fn format_field(field: &CapturedField) -> String {
    let value = match &field.value {
        CapturedValue::Token { text, .. }
        | CapturedValue::Identifier(text)
        | CapturedValue::Integer(text)
        | CapturedValue::String(text) => text,
        CapturedValue::Expression => "<expression>",
    };
    format!("{}={value}", field.name)
}
