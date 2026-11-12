use std::collections::HashSet;

use heck::{ToSnakeCase, ToUpperCamelCase};
use langmeld_language::LanguageSpec;

use crate::BuildError;

mod emit;
mod helpers;
mod model;

/// Generates typed Rust construct views for a validated language grammar.
///
/// Each generated construct type exposes its declared name through `NAME`.
/// Construct and capture names become Rust type and method names. All
/// optional and repeated groups must have explicit names. Constructs with
/// multiple syntax variants must name every variant with `syntax "name"`.
///
/// # Errors
///
/// Returns an error if names cannot map unambiguously to Rust identifiers or
/// the grammar contains unnamed groups or syntax variants.
/// A single-variant construct also cannot have a top-level capture or group
/// that maps to `from_construct`, which is the generated view constructor.
pub fn generate_bindings(specification: &LanguageSpec) -> Result<String, BuildError> {
    let constructs = model::build(specification)?;

    emit::bindings(&constructs)
}

/// Returns the Rust type name used for a declared construct's generated view.
///
/// This uses the same conversion rules as [`generate_bindings`], so tools that
/// generate typed handler code can refer to the exact emitted type.
///
/// # Errors
///
/// Returns an error if the construct name cannot be represented as a portable
/// Rust type name.
pub fn construct_type_name(name: &str) -> Result<String, BuildError> {
    rust_type_name(name)
}

struct GeneratedConstruct {
    name: String,
    type_name: String,
    variants: Vec<GeneratedVariant>,
}

struct GeneratedVariant {
    index: usize,
    enum_variant: Option<String>,
    view_type: String,
    shape: ScopeShape,
}

struct ScopeShape {
    fields: Vec<FieldShape>,
    groups: Vec<GroupShape>,
}

struct FieldShape {
    name: String,
    rust_name: String,
    expected: ExpectedCapture,
}

struct GroupShape {
    name: String,
    rust_name: String,
    kind: GroupKind,
    shape: ScopeShape,
}

#[derive(Clone, Copy)]
enum ExpectedCapture {
    Identifier,
    Token,
    Integer,
    String,
    Expression,
}

#[derive(Clone, Copy)]
enum GroupKind {
    Optional,
    Repeated,
}

fn group_type_name(parent: &str, name: &str) -> Result<String, BuildError> {
    rust_type_name(&format!("{parent} {name}"))
}

fn rust_type_name(name: &str) -> Result<String, BuildError> {
    let mut result = name.to_upper_camel_case();
    if result.is_empty() {
        return Err(unsupported(format!(
            "`{name}` does not produce a valid Rust type name"
        )));
    }
    if !result
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !result
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        return Err(unsupported(format!(
            "`{name}` cannot be converted to a portable Rust type name"
        )));
    }
    if result == "_" || is_keyword(&result) {
        result.push_str("Construct");
    }

    Ok(result)
}

fn rust_field_name(name: &str) -> Result<String, BuildError> {
    let mut result = name.to_snake_case();
    if result.is_empty()
        || !result
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !result
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        return Err(unsupported(format!(
            "`{name}` cannot be converted to a portable Rust field name"
        )));
    }
    if result == "_" || is_keyword(&result) {
        result.push('_');
    }

    Ok(result)
}

fn register_type(names: &mut HashSet<String>, name: &str) -> Result<(), BuildError> {
    if names.insert(name.to_owned()) {
        Ok(())
    } else {
        Err(unsupported(format!(
            "multiple language constructs or groups map to the Rust type `{name}`; rename them or use the dynamic API"
        )))
    }
}

fn register_method(
    names: &mut HashSet<String>,
    rust_name: &str,
    source_name: &str,
    construct: &str,
) -> Result<(), BuildError> {
    if names.insert(rust_name.to_owned()) {
        Ok(())
    } else {
        Err(unsupported(format!(
            "capture or group `{source_name}` in construct `{construct}` conflicts with another generated method `{rust_name}`"
        )))
    }
}

fn unsupported(message: String) -> BuildError {
    BuildError::UnsupportedGrammar(message)
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
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
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}
