//! `langmeld generate`, which adds to a language project.

use std::io::Write;
use std::path::Path;

use langmeld_cli::{Failure, line, read};
use langmeld_meta::{Loaded, Source, load_language};

use crate::project::{self, Project};

fn is_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

fn snake_case(name: &str) -> String {
    let mut snake = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(c.to_ascii_lowercase());
    }
    snake
}

fn append(path: &Path, text: &str) -> Result<(), Failure> {
    let unable =
        |problem: std::io::Error| Failure::Unable(format!("{}: {problem}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(unable)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(unable)
}

/// The language project at `path`, or in the current folder, and its grammar loaded.
fn open(path: Option<&Path>) -> Result<(Project, Loaded), Failure> {
    let found =
        project::find(path.unwrap_or_else(|| Path::new("."))).map_err(project::Refused::failure)?;
    let loaded = load_language(Source::path(&found.root).generic()).map_err(|error| {
        langmeld_cli::error_line(error.rendered().trim_end());
        Failure::Found
    })?;
    Ok((found, loaded))
}

/// Adds a rule to the grammar, to be changed and referred to.
pub(crate) fn rule(name: &str, path: Option<&Path>) -> Result<(), Failure> {
    if !is_type_name(name) {
        return Err(Failure::Unable(format!(
            "write the rule's name in PascalCase, not `{name}`"
        )));
    }
    let (project, loaded) = open(path)?;
    let schema = loaded.language.schema();
    if schema.node(name).is_some() || schema.union(name).is_some() {
        return Err(Failure::Unable(format!("there is already a `{name}`")));
    }
    let word = snake_case(name);
    let grammar = read(&project.root)?;
    let gap = if grammar.ends_with('\n') { "" } else { "\n" };
    append(
        &project.root,
        &format!("{gap}\nrule {name} = \"{word}\" \";\"\n"),
    )?;
    line(&format!(
        "Added rule `{name}` to {}. Change it, and refer to it from another rule.",
        project.root.display()
    ));
    Ok(())
}

/// Adds a function that handles a node to `src/handlers.rs`.
pub(crate) fn handler(node: &str, path: Option<&Path>) -> Result<(), Failure> {
    let (project, loaded) = open(path)?;
    if loaded.language.schema().node(node).is_none() || !is_type_name(node) {
        return Err(Failure::Unable(format!(
            "there is no node `{node}` in the grammar"
        )));
    }
    let mut function = snake_case(node);
    // Words of Rust cannot name functions.
    if [
        "as", "box", "break", "const", "continue", "do", "else", "enum", "false", "fn", "for",
        "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
        "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
        "while", "yield", "async", "await", "dyn", "crate", "try", "gen",
    ]
    .contains(&function.as_str())
    {
        function.push('_');
    }
    let handlers = project.dir.join("src").join("handlers.rs");
    let existing = if handlers.exists() {
        read(&handlers)?
    } else {
        String::new()
    };
    if existing.contains(&format!("fn {function}(")) {
        return Err(Failure::Unable(format!(
            "{} already has `{function}`",
            handlers.display()
        )));
    }
    let header = if existing.is_empty() {
        "//! Handlers of the nodes of trees.\n\nuse crate::syntax;\n"
    } else {
        ""
    };
    append(
        &handlers,
        &format!(
            "{header}\n/// Handles a `{node}`.\npub fn {function}(node: &syntax::{node}) {{\n    \
             let _ = node;\n}}\n"
        ),
    )?;
    line(&format!(
        "Added `{function}` to {}. Declare the module with `pub mod handlers;` in src/lib.rs.",
        handlers.display()
    ));
    Ok(())
}
