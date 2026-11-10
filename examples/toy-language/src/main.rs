use langmeld_core::SourceFile;
use langmeld_language::{
    DefinitionResolver, DefinitionSource, parse, parse_language_definition_project,
};
use std::collections::HashMap;
use std::path::{Component, Path};

const LANGUAGE_DEFINITION: &str = include_str!("../language/toy.lm");
const STATEMENTS: &str = include_str!("../language/constructs/statements.lm");
const VALUES: &str = include_str!("../language/constructs/values.lm");
const PROGRAM: &str = "program { let title = \"Hello\"; draw title; }";

struct ExampleResolver(HashMap<String, &'static str>);

impl ExampleResolver {
    fn new() -> Self {
        Self(HashMap::from([
            ("language/constructs/statements.lm".to_owned(), STATEMENTS),
            ("language/constructs/values.lm".to_owned(), VALUES),
        ]))
    }
}

impl DefinitionResolver for ExampleResolver {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let import_path = Path::new(import);
        if import_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("example imports must use relative paths without `.` or `..`".into());
        }

        let path = Path::new(from_key)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(import_path);
        let key = path.to_string_lossy().replace('\\', "/");
        let text = self
            .0
            .get(&key)
            .ok_or_else(|| format!("no embedded definition source for `{key}`"))?;

        Ok(DefinitionSource::new(
            key.clone(),
            SourceFile::new(key, *text),
        ))
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root_source = SourceFile::new("language/toy.lm", LANGUAGE_DEFINITION);
    let definition = parse_language_definition_project(
        DefinitionSource::new("language/toy.lm", root_source.clone()),
        &ExampleResolver::new(),
    );
    if !definition.is_valid() {
        return Err(format_diagnostics(
            &[
                root_source,
                SourceFile::new("language/constructs/statements.lm", STATEMENTS),
                SourceFile::new("language/constructs/values.lm", VALUES),
            ],
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
