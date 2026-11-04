//! Rust host entry points for the Ledger language.

use langmeld::core::{Diagnostic, Severity, SourceFile, ValidatedLanguageSpec};
use langmeld::expression::{Atom, Expression, ExpressionRuntime, evaluate_expression};
use langmeld::language::{
    AstNode, CapturedValue, ConfiguredLanguageParser, ParseResult, Program, ProjectSnapshot, Value,
    WorkspaceAnalysis, analyze_workspace_with_host_callbacks,
};
use std::collections::BTreeMap;
use std::path::Path;

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

/// Loads the language model compiled by this crate's build script.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language()
}

/// Loads the current project definitions dynamically from disk.
pub fn load_language_runtime() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language_runtime()
}

/// Loads the compiled language model and configured program sources.
pub fn load_project() -> Result<ProjectSnapshot, String> {
    let language = load_language().map_err(format_language_errors)?;
    langmeld::language::load_project_with_compiled_language(Path::new("langmeld.json"), &language)
}

/// Loads the current project definitions and sources dynamically from disk.
pub fn load_project_runtime() -> Result<ProjectSnapshot, String> {
    langmeld::language::load_project(Path::new("langmeld.json"))
}

/// Prepares the declarative parser for the language.
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

/// Interprets the supported statements and returns their output lines.
///
/// Langmeld provides parsing and generic symbol analysis. This host defines
/// statement evaluation and the rule that variables must be set before use.
pub fn evaluate(context: &RuntimeContext<'_>) -> Result<Vec<String>, String> {
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
        .any(|item| item.diagnostic.severity == Severity::Error)
    {
        return Err("cannot execute a program with analysis errors".to_owned());
    }

    let mut variables = BTreeMap::new();
    let mut output = Vec::new();
    for program in context.programs {
        for node in &program.nodes {
            execute_node(node, &mut variables, &mut output)?;
        }
    }
    Ok(output)
}

/// Runs the interpreted program and writes its output to standard output.
pub fn run_sources(context: &RuntimeContext<'_>) -> Result<(), String> {
    for line in evaluate(context)? {
        println!("{line}");
    }
    Ok(())
}

fn execute_node(
    node: &AstNode,
    variables: &mut BTreeMap<String, f64>,
    output: &mut Vec<String>,
) -> Result<(), String> {
    match node.construct.as_str() {
        "program" => {
            for child in &node.children {
                execute_node(child, variables, output)?;
            }
        }
        "set" => {
            let name = identifier(node, "name")?.to_owned();
            let value = number(node, "value")?;
            insert_variable(variables, name, value)?;
        }
        "add" => {
            let left_name = reference(node, "left")?;
            let right_name = reference(node, "right")?;
            let name = identifier(node, "name")?.to_owned();
            let left = lookup_variable(variables, left_name)?;
            let right = lookup_variable(variables, right_name)?;
            let value = left + right;
            if !value.is_finite() {
                return Err(format!("addition for `{name}` produced a non-finite value"));
            }
            insert_variable(variables, name, value)?;
        }
        "calculate" => {
            let name = identifier(node, "name")?.to_owned();
            let value = evaluate_numeric_expression(expression(node, "value")?)?;
            insert_variable(variables, name, value)?;
        }
        "subtract" => {
            let right_name = reference(node, "right")?;
            let left_name = reference(node, "left")?;
            let name = identifier(node, "name")?.to_owned();
            let left = lookup_variable(variables, left_name)?;
            let right = lookup_variable(variables, right_name)?;
            let value = left - right;
            if !value.is_finite() {
                return Err(format!(
                    "subtraction for `{name}` produced a non-finite value"
                ));
            }
            insert_variable(variables, name, value)?;
        }
        "print" => {
            let name = reference(node, "target")?;
            let value = lookup_variable(variables, name)?;
            output.push(format!("{name} = {value}"));
        }
        other => return Err(format!("unsupported construct `{other}`")),
    }
    Ok(())
}

fn field<'a>(node: &'a AstNode, name: &str) -> Result<&'a CapturedValue, String> {
    node.fields
        .get(name)
        .ok_or_else(|| format!("{} construct is missing `{name}`", node.construct))
}

fn identifier<'a>(node: &'a AstNode, name: &str) -> Result<&'a str, String> {
    match &field(node, name)?.value {
        Value::Identifier(value) => Ok(value),
        _ => Err(format!("{}.{} is not an identifier", node.construct, name)),
    }
}

fn reference<'a>(node: &'a AstNode, name: &str) -> Result<&'a str, String> {
    match &field(node, name)?.value {
        Value::Reference(value) => Ok(value),
        _ => Err(format!("{}.{} is not a reference", node.construct, name)),
    }
}

fn number(node: &AstNode, name: &str) -> Result<f64, String> {
    match field(node, name)?.value {
        Value::Number(value) if value.is_finite() => Ok(value),
        Value::Number(_) => Err(format!("{}.{} is not finite", node.construct, name)),
        _ => Err(format!("{}.{} is not a number", node.construct, name)),
    }
}

fn expression<'a>(node: &'a AstNode, name: &str) -> Result<&'a Expression, String> {
    match &field(node, name)?.value {
        Value::Expression(value) => Ok(value),
        _ => Err(format!("{}.{} is not an expression", node.construct, name)),
    }
}

fn evaluate_numeric_expression(expression: &Expression) -> Result<f64, String> {
    evaluate_expression(expression, &mut NumericRuntime).map_err(|diagnostic| diagnostic.message)
}

struct NumericRuntime;

impl ExpressionRuntime for NumericRuntime {
    type Value = f64;

    fn atom(&mut self, atom: &Atom, _span: langmeld::core::Span) -> Result<Self::Value, String> {
        match atom {
            Atom::Number(value) => {
                let value = value
                    .parse::<f64>()
                    .map_err(|_| format!("`{value}` is not a valid number"))?;
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err("numeric literal must be finite".to_owned())
                }
            }
            Atom::Boolean(_) | Atom::Name(_) | Atom::Custom { .. } => {
                Err("this example supports numeric expression atoms only".to_owned())
            }
        }
    }

    fn prefix(
        &mut self,
        operator: &str,
        _operator_span: langmeld::core::Span,
        operand: &Self::Value,
    ) -> Result<Self::Value, String> {
        match operator {
            "-" => Ok(-operand),
            other => Err(format!("operator `{other}` is not supported")),
        }
    }

    fn infix(
        &mut self,
        operator: &str,
        _operator_span: langmeld::core::Span,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, String> {
        let result = match operator {
            "+" => left + right,
            "*" => left * right,
            other => return Err(format!("operator `{other}` is not supported")),
        };
        if result.is_finite() {
            Ok(result)
        } else {
            Err(format!(
                "operator `{operator}` produced a non-finite result"
            ))
        }
    }
}

fn lookup_variable(variables: &BTreeMap<String, f64>, name: &str) -> Result<f64, String> {
    variables
        .get(name)
        .copied()
        .ok_or_else(|| format!("variable `{name}` is used before it is set"))
}

fn insert_variable(
    variables: &mut BTreeMap<String, f64>,
    name: String,
    value: f64,
) -> Result<(), String> {
    if variables.insert(name.clone(), value).is_some() {
        return Err(format!("variable `{name}` is already defined"));
    }
    Ok(())
}

fn format_language_errors(errors: Vec<Diagnostic>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{}: {}", error.code, error.message))
        .collect::<Vec<_>>()
        .join("\n")
}
