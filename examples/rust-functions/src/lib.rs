//! Rust host entry points for the Functions language.

use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{
    AstNode, ConfiguredLanguageParser, ParseResult, Program, ProjectSnapshot, Value,
    WorkspaceAnalysis, analyze_workspace_with_host_callbacks,
};
use std::{collections::BTreeSet, path::Path};

pub mod host;
mod language;

/// Parsed and analyzed inputs supplied to this language's execution hook.
pub struct RuntimeContext<'a> {
    /// Validated language model used to parse and analyze the inputs.
    pub language: &'a ValidatedLanguageSpec,
    /// Source snapshots in the order selected by the project.
    pub sources: &'a [SourceFile],
    /// Parsed programs corresponding to `sources` by index.
    pub programs: &'a [Program],
    /// Generic and host-provided analysis results for those programs.
    pub analysis: &'a WorkspaceAnalysis,
}

/// Loads the effective language model compiled by this crate's build script.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language()
}

/// Loads the Rust base model and declarative extensions dynamically from disk.
pub fn load_language_runtime() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language_runtime()
}

/// Loads configured sources using the effective language model compiled at build time.
pub fn load_project() -> Result<ProjectSnapshot, String> {
    let language = load_language().map_err(format_language_errors)?;
    langmeld::language::load_project_with_compiled_language(Path::new("langmeld.json"), &language)
}

/// Loads the Rust base model and declared `.langmeld` extensions dynamically from disk.
pub fn load_project_runtime() -> Result<ProjectSnapshot, String> {
    let language = load_language_runtime().map_err(format_language_errors)?;
    langmeld::language::load_project_with_compiled_language(Path::new("langmeld.json"), &language)
}

/// Prepares the declarative parser for the base language model.
pub fn parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    parser_for(load_language()?)
}

/// Prepares a validated language model with this project's custom callbacks.
pub fn parser_for(
    language: ValidatedLanguageSpec,
) -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    host::runtime::configured_parser(language)
        .map_err(|error| vec![Diagnostic::spec_error("LMSTARTER001", error)])
}

/// Parses one source snapshot and returns its syntax tree and diagnostics.
pub fn parse_source(source: &SourceFile) -> Result<ParseResult, Vec<Diagnostic>> {
    Ok(parser()?.parse(source))
}

/// Runs the project's registered analysis callbacks over parsed source files.
pub fn analyze_sources(
    language: &ValidatedLanguageSpec,
    sources: &[SourceFile],
    programs: &[&Program],
) -> Result<WorkspaceAnalysis, String> {
    if sources.len() != programs.len() {
        return Err(format!(
            "source and program counts differ: {} sources, {} programs",
            sources.len(),
            programs.len()
        ));
    }
    let files: Vec<_> = sources.iter().zip(programs.iter().copied()).collect();
    let semantic_hooks =
        host::analysis::semantic_behavior_hooks(language).map_err(|error| error.to_string())?;
    let resolver_hooks =
        host::analysis::resolver_behavior_hooks(language).map_err(|error| error.to_string())?;
    let type_hooks =
        host::analysis::workspace_behavior_hooks(language).map_err(|error| error.to_string())?;
    Ok(analyze_workspace_with_host_callbacks(
        language,
        &files,
        &[],
        &semantic_hooks,
        &resolver_hooks,
        &type_hooks,
    ))
}

/// Runs validated source files using this language's runtime implementation.
pub fn run_sources(_context: &RuntimeContext<'_>) -> Result<(), String> {
    Err("runtime behavior is not implemented yet; add it in src/lib.rs".to_owned())
}

/// Lowers supported function declarations into deterministic Rust source.
pub fn generate_rust(context: &RuntimeContext<'_>) -> Result<String, String> {
    if context.sources.len() != context.programs.len() {
        return Err(format!(
            "source and program counts differ: {} sources, {} programs",
            context.sources.len(),
            context.programs.len()
        ));
    }
    if context.analysis.resource_limited {
        return Err("workspace analysis stopped at a configured resource limit".to_owned());
    }
    if context
        .analysis
        .diagnostics
        .iter()
        .any(|item| item.diagnostic.severity == langmeld::core::Severity::Error)
    {
        return Err("cannot generate Rust from a program with analysis errors".to_owned());
    }

    let mut output = String::new();
    for program in context.programs {
        for module in &program.nodes {
            if module.construct != "module" {
                return Err(format!(
                    "expected module root, found `{}`",
                    module.construct
                ));
            }
            for function in &module.children {
                emit_function(function, &mut output)?;
            }
        }
    }
    Ok(output)
}

fn emit_function(function: &AstNode, output: &mut String) -> Result<(), String> {
    if function.construct != "function" {
        return Err(format!(
            "unsupported function construct `{}`",
            function.construct
        ));
    }
    let name = identifier(function, "name")?;
    ensure_rust_identifier(name)?;
    ensure_i32(function, "return_type")?;
    let parameters = parameters(function)?;
    let modifier = match function.fields.get("modifier").map(|value| &value.value) {
        None => "",
        Some(Value::Enum(value)) if value == "unsafe" => "unsafe ",
        Some(_) => return Err("function modifier is not supported".to_owned()),
    };
    let mut names = BTreeSet::new();
    for (parameter_name, parameter_type) in &parameters {
        ensure_rust_identifier(parameter_name)?;
        ensure_i32_type(parameter_type, "function parameter")?;
        if !names.insert(*parameter_name) {
            return Err("function parameters must have distinct names".to_owned());
        }
    }

    let signature = parameters
        .iter()
        .map(|(name, _)| format!("{name}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    let body = if names.is_empty() {
        "0".to_owned()
    } else {
        parameters
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(" + ")
    };
    output.push_str(&format!(
        "pub {modifier}fn {name}({signature}) -> i32 {{\n    {body}\n}}\n\n"
    ));
    Ok(())
}

fn parameters(function: &AstNode) -> Result<Vec<(&str, &str)>, String> {
    let Some(captured) = function.fields.get("parameters") else {
        return Ok(Vec::new());
    };
    let Value::List(parameters) = &captured.value else {
        return Err("function.parameters is not a list".to_owned());
    };
    parameters
        .iter()
        .map(|parameter| {
            let Value::Properties(fields) = &parameter.value else {
                return Err("function parameter is not a record".to_owned());
            };
            let name = fields
                .get("name")
                .and_then(|value| match &value.value {
                    Value::Identifier(name) => Some(name.as_str()),
                    _ => None,
                })
                .ok_or_else(|| "function parameter name is not an identifier".to_owned())?;
            let parameter_type = fields
                .get("type")
                .and_then(|value| match &value.value {
                    Value::Identifier(value) => Some(value.as_str()),
                    _ => None,
                })
                .ok_or_else(|| "function parameter type is not an identifier".to_owned())?;
            Ok((name, parameter_type))
        })
        .collect()
}

fn identifier<'a>(node: &'a AstNode, field: &str) -> Result<&'a str, String> {
    match node.fields.get(field).map(|captured| &captured.value) {
        Some(Value::Identifier(value)) => Ok(value),
        _ => Err(format!("{}.{} is not an identifier", node.construct, field)),
    }
}

fn ensure_i32(node: &AstNode, field: &str) -> Result<(), String> {
    ensure_i32_type(identifier(node, field)?, field)
}

fn ensure_i32_type(value: &str, label: &str) -> Result<(), String> {
    match value {
        "i32" => Ok(()),
        other => Err(format!("unsupported type `{other}` in {label}")),
    }
}

fn ensure_rust_identifier(value: &str) -> Result<(), String> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("Rust identifier cannot be empty".to_owned());
    };
    if !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
        || is_rust_keyword(value)
    {
        return Err(format!("`{value}` is not a supported Rust identifier"));
    }
    Ok(())
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
            | "gen"
    )
}

fn format_language_errors(errors: Vec<Diagnostic>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{}: {}", error.code, error.message))
        .collect::<Vec<_>>()
        .join("\n")
}
