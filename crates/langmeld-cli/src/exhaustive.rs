//! `langmeld check --exhaustive`: the nodes that a project's code never handles.
//!
//! A node is handled if the code names its type, its variant in a group, or its
//! visitor method, or calls a method that gives it. This reads the code as words, so
//! it finds what was forgotten rather than proving that all is handled; matching
//! without `_` and the generated `Visitor` let the compiler do that.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use langmeld_cli::{Failure, line, read};
use langmeld_syntax::Language;

/// What the code names.
#[derive(Default)]
struct Words {
    names: HashSet<String>,
    /// Pairs written as `First::Second`.
    paths: HashSet<(String, String)>,
    /// Names written after a `.`, as methods are called.
    called: HashSet<String>,
}

/// Rust code without its comments and quoted text.
fn code_of(source: &str) -> String {
    let mut code = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&c| c != '\n').is_some() {},
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        break;
                    }
                    previous = c;
                }
                code.push(' ');
            }
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => drop(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
                code.push(' ');
            }
            c => code.push(c),
        }
    }
    code
}

impl Words {
    fn add(&mut self, source: &str) {
        let code = code_of(source);
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut previous: Option<(&str, usize)> = None;
        let mut start = None;
        for (at, c) in code.char_indices().chain([(code.len(), ' ')]) {
            if is_word(c) {
                start.get_or_insert(at);
                continue;
            }
            let Some(from) = start.take() else { continue };
            let Some(word) = code.get(from..at) else {
                continue;
            };
            let before = code.get(..from).unwrap_or_default().trim_end();
            if before.ends_with('.') {
                self.called.insert(word.to_owned());
            }
            if let Some((first, end)) = previous
                && code.get(end..from).is_some_and(|gap| gap.trim() == "::")
            {
                self.paths.insert((first.to_owned(), word.to_owned()));
            }
            previous = Some((word, at));
            self.names.insert(word.trim_start_matches("r#").to_owned());
        }
    }
}

fn gather(folder: &Path, words: &mut Words, depth: usize) -> Result<(), Failure> {
    let unable =
        |problem: std::io::Error| Failure::Unable(format!("{}: {problem}", folder.display()));
    for entry in std::fs::read_dir(folder).map_err(unable)? {
        let path = entry.map_err(unable)?.path();
        if path.is_dir() {
            if depth < 64 {
                gather(&path, words, depth + 1)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            words.add(&read(&path)?);
        }
    }
    Ok(())
}

fn snake_case(name: &str) -> String {
    let mut snake = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && index > 0 && !snake.ends_with('_') {
            snake.push('_');
        }
        snake.push(c.to_ascii_lowercase());
    }
    snake
}

/// Lists the nodes of the language that the code in the project's `src` folder does
/// not handle. Fails if there are any.
pub(crate) fn run(language: &Language, project: &Path) -> Result<(), Failure> {
    let folder = project.join("src");
    if !folder.is_dir() {
        return Err(Failure::Unable(format!(
            "there is no src folder in {}",
            project.display()
        )));
    }
    let mut words = Words::default();
    gather(&folder, &mut words, 0)?;
    let schema = language.schema();
    let mut missing = BTreeSet::new();
    for node in schema.nodes() {
        let name = node.name.as_str();
        let groups: Vec<_> = schema
            .unions()
            .filter(|(_, members)| members.iter().any(|member| member == name))
            .map(|(group, _)| group)
            .collect();
        let visited = words.names.contains(&format!("visit_{}", snake_case(name)));
        let handled = if groups.is_empty() {
            // A node reached through a label is handled where the label is read.
            let read = schema.nodes().flat_map(|other| &other.fields).any(|field| {
                field.nodes.iter().any(|n| n == name) && words.called.contains(&field.name)
            });
            read || words.names.contains(name)
        } else {
            groups.iter().any(|group| {
                words
                    .paths
                    .contains(&((*group).to_owned(), name.to_owned()))
            })
        };
        if !handled && !visited {
            let written = match groups.first() {
                Some(group) => format!("{group}::{name}"),
                None => name.to_owned(),
            };
            missing.insert(written);
        }
    }
    for node in &missing {
        line(&format!("`{node}` is not handled in src"));
    }
    if missing.is_empty() {
        line("src handles every node");
        Ok(())
    } else {
        Err(Failure::Found)
    }
}
