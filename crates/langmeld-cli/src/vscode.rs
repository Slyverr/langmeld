//! `langmeld package vscode`, which writes an extension for Visual Studio Code from the
//! grammar and `langmeld.toml`.

use std::path::Path;

use langmeld_cli::{Failure, line, read};
use langmeld_meta::{
    Layout, MANIFEST_NAME, Manifest, Source, grammar_language, load_language, render,
};
use langmeld_syntax::{FileId, Language, Severity, SourceFile};
use serde_json::{Map, Value, json};

use crate::project;

/// The versions of the packages an extension uses.
const CLIENT: &str = "^9.0.1";
const TYPESCRIPT: &str = "^5.6.0";
const VSCODE_TYPES: &str = "^1.90.0";
const NODE_TYPES: &str = "^22.0.0";

const BRACKETS: [(&str, &str); 3] = [("(", ")"), ("[", "]"), ("{", "}")];

/// A language name as extensions and scopes write it: `Shape_Script` is `shape-script`.
fn identifier(name: &str) -> String {
    let mut identifier = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            identifier.push(c.to_ascii_lowercase());
        } else if !identifier.ends_with('-') {
            identifier.push('-');
        }
    }
    let identifier = identifier.trim_matches('-');
    if identifier.is_empty() {
        "language".to_owned()
    } else {
        identifier.to_owned()
    }
}

/// `text` with every character that means something in a pattern escaped.
fn escaped(text: &str) -> String {
    let mut pattern = String::new();
    for c in text.chars() {
        if r"\^$.|?*+()[]{}-/".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern
}

struct Extension<'l> {
    language: &'l Language,
    id: String,
    name: String,
    publisher: String,
    icon: Option<String>,
    /// The program that serves the language, and its arguments.
    server: Vec<String>,
    /// The name of the project file of the language's users.
    project: Option<String>,
}

impl Extension<'_> {
    fn brackets(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        BRACKETS.into_iter().filter(|(open, close)| {
            self.language.token_kind(open).is_some() && self.language.token_kind(close).is_some()
        })
    }

    fn manifest(&self) -> Value {
        let id = &self.id;
        let extensions: Vec<_> = self
            .language
            .extensions()
            .map(|extension| format!(".{extension}"))
            .collect();
        let mut package = json!({
            "name": format!("{id}-language"),
            "displayName": self.name,
            "description": format!("Editor support for {}", self.language.name()),
            "publisher": self.publisher,
            "version": "0.1.0",
            "engines": { "vscode": VSCODE_TYPES },
            "categories": ["Programming Languages"],
            "main": "./out/extension.js",
            "contributes": {
                "languages": [{
                    "id": id,
                    "aliases": [self.language.name()],
                    "extensions": extensions,
                    "configuration": "./language-configuration.json",
                }],
                "grammars": [{
                    "language": id,
                    "scopeName": format!("source.{id}"),
                    "path": format!("./syntaxes/{id}.tmLanguage.json"),
                }],
                "configuration": {
                    "title": self.name,
                    "properties": {
                        format!("{id}.server.path"): {
                            "type": "string",
                            "default": "",
                            "description": "The language server to start, instead of the one on the path.",
                        },
                    },
                },
            },
            "scripts": {
                "compile": "tsc -p .",
                "vscode:prepublish": "npm run compile",
            },
            "dependencies": { "vscode-languageclient": CLIENT },
            "devDependencies": {
                "@types/node": NODE_TYPES,
                "@types/vscode": VSCODE_TYPES,
                "typescript": TYPESCRIPT,
            },
        });
        if let (Some(icon), Some(fields)) = (&self.icon, package.as_object_mut()) {
            fields.insert("icon".to_owned(), Value::String(icon.clone()));
        }
        // A folder with the project file starts the server before a file is opened.
        if let (Some(file), Some(fields)) = (&self.project, package.as_object_mut()) {
            let event = format!("workspaceContains:**/{file}");
            fields.insert("activationEvents".to_owned(), json!([event]));
        }
        package
    }

    /// What marks a word for double clicks and `Ctrl`+arrows: a number or a name of the
    /// language.
    fn word_pattern(&self) -> String {
        match self.language.identifier_chars() {
            Some((start, rest)) => format!(r"(-?\d+(\.\d+)?)|({start}{rest}*)"),
            None => r"[\p{L}\p{N}_]+".to_owned(),
        }
    }

    fn configuration(&self) -> Value {
        let mut comments = Map::new();
        if let Some(open) = self.language.line_comments().next() {
            comments.insert("lineComment".to_owned(), json!(open));
        }
        if let Some((open, close)) = self.language.block_comments().next() {
            comments.insert("blockComment".to_owned(), json!([open, close]));
        }
        let brackets: Vec<_> = self.brackets().collect();
        let quotes: Vec<String> = self.language.quotes().map(String::from).collect();
        let pairs: Vec<Value> = brackets
            .iter()
            .map(|&(open, close)| (open, close))
            .chain(quotes.iter().map(|q| (q.as_str(), q.as_str())))
            .map(|(open, close)| json!({ "open": open, "close": close }))
            .collect();
        json!({
            "comments": comments,
            "brackets": brackets.iter().map(|&(a, b)| json!([a, b])).collect::<Vec<_>>(),
            "autoClosingPairs": pairs,
            "surroundingPairs": pairs,
            "wordPattern": self.word_pattern(),
        })
    }

    /// Colours from patterns, for the moments before the server starts. Once it runs, it
    /// colours from the tree.
    fn textmate(&self) -> Value {
        let id = &self.id;
        let mut patterns = Vec::new();
        for (open, close) in self.language.block_comments() {
            patterns.push(json!({
                "name": format!("comment.block.{id}"),
                "begin": escaped(open),
                "end": escaped(close),
            }));
        }
        for open in self.language.line_comments() {
            patterns.push(json!({
                "name": format!("comment.line.{id}"),
                "match": format!("{}.*$", escaped(open)),
            }));
        }
        for quote in self.language.quotes() {
            let kind = if quote == '"' { "double" } else { "single" };
            patterns.push(json!({
                "name": format!("string.quoted.{kind}.{id}"),
                "begin": quote.to_string(),
                "end": format!("{quote}|$"),
                "patterns": [{ "name": format!("constant.character.escape.{id}"), "match": r"\\." }],
            }));
        }
        let mut words: Vec<&str> = self.language.words().collect();
        words.sort_unstable();
        words.dedup();
        if !words.is_empty() {
            let listed: Vec<_> = words.into_iter().map(escaped).collect();
            patterns.push(json!({
                "name": format!("keyword.control.{id}"),
                "match": format!(r"\b({})\b", listed.join("|")),
            }));
        }
        patterns.push(json!({
            "name": format!("constant.numeric.{id}"),
            "match": r"\b(0[xX][0-9a-fA-F_]+|[0-9][0-9_]*(\.[0-9_]+)?([eE][+-]?[0-9]+)?)\b",
        }));
        json!({
            "name": self.language.name(),
            "scopeName": format!("source.{id}"),
            "patterns": patterns,
        })
    }

    fn code(&self) -> String {
        let id = &self.id;
        let command = self.server.first().map_or(id.as_str(), String::as_str);
        let arguments = json!(self.server.get(1..).unwrap_or_default());
        let command = json!(command);
        let name = json!(self.name);
        let mut selector = format!(r#"{{ language: "{id}" }}"#);
        if let Some(file) = &self.project {
            let pattern = json!(format!("**/{file}"));
            selector.push_str(&format!(", {{ pattern: {pattern} }}"));
        }
        format!(
            r#"// Written by `langmeld package vscode`. It starts the language server.
import * as vscode from "vscode";
import {{ LanguageClient }} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {{
  const settings = vscode.workspace.getConfiguration("{id}");
  const command = settings.get<string>("server.path") || {command};
  const server = {{ command, args: {arguments} }};
  client = new LanguageClient("{id}", {name}, server, {{
    documentSelector: [{selector}],
  }});
  context.subscriptions.push({{ dispose: () => client?.stop() }});
  await client.start();
}}

export function deactivate() {{
  return client?.stop();
}}
"#
        )
    }

    fn files(&self) -> Vec<(String, String)> {
        let pretty = |value: Value| format!("{value:#}\n");
        let command = self.server.join(" ");
        vec![
            ("package.json".to_owned(), pretty(self.manifest())),
            (
                "language-configuration.json".to_owned(),
                pretty(self.configuration()),
            ),
            (
                format!("syntaxes/{}.tmLanguage.json", self.id),
                pretty(self.textmate()),
            ),
            ("src/extension.ts".to_owned(), self.code()),
            (
                "tsconfig.json".to_owned(),
                pretty(json!({
                    "compilerOptions": {
                        "module": "node16",
                        "target": "es2022",
                        "outDir": "out",
                        "rootDir": "src",
                        "strict": true,
                        "skipLibCheck": true,
                    },
                    "include": ["src"],
                })),
            ),
            (
                ".vscodeignore".to_owned(),
                "src/\ntsconfig.json\n.gitignore\n".to_owned(),
            ),
            (
                ".gitignore".to_owned(),
                "node_modules/\nout/\n*.vsix\n".to_owned(),
            ),
            (
                "README.md".to_owned(),
                format!(
                    "# {}\n\nEditor support for {}. It starts `{command}`, which has to be on the \
                     path, or set\n`{}.server.path`.\n\nTo try it: `npm install`, `npm run \
                     compile`, open this folder in Visual Studio Code\nand press F5. To make a \
                     package: `npx @vscode/vsce package`.\n",
                    self.name,
                    self.language.name(),
                    self.id
                ),
            ),
        ]
    }
}

fn write(folder: &Path, path: &str, text: &[u8]) -> Result<(), Failure> {
    let path = folder.join(path);
    let unable =
        |problem: std::io::Error| Failure::Unable(format!("{}: {problem}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(unable)?;
    }
    std::fs::write(&path, text).map_err(unable)
}

/// The manifest of a project, or the defaults.
fn settings(folder: &Path) -> Result<Manifest, Failure> {
    let path = folder.join(MANIFEST_NAME);
    if !path.is_file() {
        return Ok(Manifest::default());
    }
    let file = SourceFile::new(FileId::new(0), path.display().to_string(), read(&path)?)
        .map_err(|problem| Failure::Unable(problem.to_string()))?;
    Manifest::parse(&file).map_err(|problems| {
        langmeld_cli::error_line(render(std::slice::from_ref(&file), &problems).trim_end());
        Failure::Found
    })
}

/// Writes the extension of the project at `path`, or of `.langmeld` files themselves.
pub(crate) fn run(path: &Path, out: Option<&Path>, grammar: bool) -> Result<(), Failure> {
    let loaded;
    let (language, folder, manifest) = if grammar {
        (
            grammar_language(),
            Path::new(".").to_path_buf(),
            Manifest::default(),
        )
    } else {
        let found = project::find(path).map_err(project::Refused::failure)?;
        loaded = load_language(Source::path(&found.root).generic()).map_err(|error| {
            langmeld_cli::error_line(error.rendered().trim_end());
            let errors = error
                .diagnostics
                .iter()
                .filter(|found| found.severity == Severity::Error)
                .count();
            langmeld_cli::error_line(&format!(
                "error: the grammar has {errors} problem(s), so there is nothing to make an \
                 extension from"
            ));
            Failure::Found
        })?;
        let manifest = settings(&found.dir)?;
        (&loaded.language, found.dir, manifest)
    };
    if language.extensions().next().is_none() {
        line("The language declares no `extensions`, so no files will open with it.");
    }
    let vscode = manifest.package.vscode.unwrap_or_default();
    let server = Some(manifest.bins.server)
        .filter(|server| !server.is_empty())
        .unwrap_or_else(|| {
            let program = if grammar {
                "langmeld".to_owned()
            } else {
                // The name `langmeld::cli::App` gives a program; the tools of a split
                // project are in the program with `c` after it.
                let name = language.name().to_lowercase();
                match manifest.bins.layout {
                    Layout::Split => format!("{name}c"),
                    _ => name,
                }
            };
            vec![program, "lsp".to_owned()]
        });
    let out = out.map_or_else(
        || folder.join(vscode.out.as_deref().unwrap_or("editors/vscode")),
        Path::to_owned,
    );
    let icon = match &vscode.icon {
        Some(icon) => {
            let source = folder.join(icon);
            let image = std::fs::read(&source).map_err(|problem| {
                Failure::Unable(format!("cannot read {}: {problem}", source.display()))
            })?;
            let name = source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| Failure::Unable(format!("`{icon}` is not a file")))?;
            write(&out, &name, &image)?;
            Some(name)
        }
        None => None,
    };
    let extension = Extension {
        language,
        id: identifier(language.name()),
        name: vscode
            .display_name
            .unwrap_or_else(|| language.name().to_owned()),
        publisher: vscode.publisher.unwrap_or_else(|| "local".to_owned()),
        icon,
        server,
        project: manifest.project.map(|project| project.manifest),
    };
    for (path, text) in extension.files() {
        write(&out, &path, text.as_bytes())?;
    }
    line(&format!(
        "Wrote the extension to {0}. To try it:\n\n    cd {0}\n    npm install\n    \
         npm run compile\n\nThen open that folder in Visual Studio Code and press F5.",
        out.display()
    ));
    Ok(())
}
