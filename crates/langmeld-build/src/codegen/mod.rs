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

/// Rust names used for a construct's generated handler source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructHandlerNames {
    /// Rust module identifier and source filename stem.
    pub module_name: String,
    /// Rust handler struct identifier.
    pub handler_name: String,
}

/// Returns the module and handler identifiers used by generated construct
/// handlers.
///
/// Construct names may contain ASCII letters, digits, hyphens, and underscores.
/// Reserved Rust module keywords receive a `_handler` suffix, and leading
/// digits receive a `construct_` prefix for module names.
///
/// # Errors
///
/// Returns an error if the construct name cannot map to a Rust module name.
pub fn construct_handler_names(name: &str) -> Result<ConstructHandlerNames, BuildError> {
    let mut words = Vec::new();
    let mut word = String::new();

    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            word.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_') {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            return Err(unsupported(format!(
                "construct `{name}` cannot map to a Rust module name; use ASCII letters, digits, `-`, or `_`"
            )));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.is_empty() {
        return Err(unsupported(
            "construct name must contain an ASCII letter or digit".to_owned(),
        ));
    }

    let mut module_name = words.join("_");
    if module_name.starts_with(|character: char| character.is_ascii_digit()) {
        module_name.insert_str(0, "construct_");
    }
    if is_keyword(&module_name) {
        module_name.push_str("_handler");
    }

    let handler_type_name = match rust_type_name(name) {
        Ok(type_name) => type_name,
        Err(BuildError::UnsupportedGrammar(_)) => module_name.to_upper_camel_case(),
        Err(error) => return Err(error),
    };
    let handler_name = format!("{handler_type_name}Handler");

    Ok(ConstructHandlerNames {
        module_name,
        handler_name,
    })
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
