//! `langmeld.toml`: what a language project says besides its `.langmeld` files.

use std::ops::Range;

use langmeld_syntax::{Diagnostic, FileId, SourceFile, TextRange, TextSize};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::problem::problem;

/// The name of the manifest file.
pub const FILE_NAME: &str = "langmeld.toml";

/// Where the root `.langmeld` file is when the manifest does not say.
pub const DEFAULT_ROOT: &str = "language/main.langmeld";

/// Fields of the `language` block, which the manifest must not repeat.
const LANGUAGE_FIELDS: [&str; 6] = [
    "language",
    "extensions",
    "comments",
    "terminators",
    "continue_inside",
    "tokens",
];

/// How the binaries of a language are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Layout {
    /// One binary with every command.
    #[default]
    Single,
    /// A binary for the compiler, one for the language server and one for the rest.
    Split,
    /// One binary that acts as the others when called by their names.
    Multicall,
}

/// The `[bins]` table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bins {
    /// How the binaries are laid out.
    pub layout: Layout,
    /// The command editors run to start the language server.
    pub server: Vec<String>,
}

/// The `[project]` table: what marks the root of a user's project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// The name of the project file, such as `mylang.toml`.
    pub manifest: String,
}

/// The `[package.vscode]` table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VsCode {
    /// The name shown in the editor.
    pub display_name: Option<String>,
    /// The publisher's identifier.
    pub publisher: Option<String>,
    /// The icon file.
    pub icon: Option<String>,
    /// The folder the extension is written to.
    pub out: Option<String>,
}

/// The `[package]` table: editor packages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    /// Visual Studio Code.
    pub vscode: Option<VsCode>,
}

/// A read `langmeld.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The root `.langmeld` file, relative to the manifest.
    pub root: String,
    /// The `[bins]` table.
    pub bins: Bins,
    /// The `[project]` table.
    pub project: Option<Project>,
    /// The `[package]` table.
    pub package: Package,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            root: DEFAULT_ROOT.to_owned(),
            bins: Bins::default(),
            project: None,
            package: Package::default(),
        }
    }
}

struct Reading<'a> {
    file: &'a SourceFile,
    problems: Vec<Diagnostic>,
}

fn quoted(keys: &[&str]) -> String {
    let keys: Vec<_> = keys.iter().map(|key| format!("`{key}`")).collect();
    match keys.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => keys.concat(),
    }
}

impl Reading<'_> {
    fn range(&self, span: Range<usize>) -> TextRange {
        let end = self.file.text().len();
        let at = |offset: usize| TextSize::from(u32::try_from(offset.min(end)).unwrap_or(u32::MAX));
        TextRange::new(at(span.start), at(span.end.max(span.start)))
    }

    fn report(&mut self, code: &'static str, span: Range<usize>, headline: String, fix: &str) {
        let range = self.range(span);
        self.problems
            .push(problem(code, self.file.id(), range, headline, fix));
    }

    /// Reports the keys of `table` that `known` does not list.
    fn keys(&mut self, table: &DeTable<'_>, known: &[&str], inside: &str) {
        for (key, _) in table {
            let name: &str = key.get_ref().as_ref();
            if known.contains(&name) {
                continue;
            }
            if inside == "the top level" && LANGUAGE_FIELDS.contains(&name) {
                self.report(
                    "LM-M003",
                    key.span(),
                    format!("`{name}` is already described by the `language` block of the grammar"),
                    "remove it from this file, because the `language` block is the only place \
                     for it",
                );
            } else {
                self.report(
                    "LM-M002",
                    key.span(),
                    format!("`{name}` is not a setting of {inside}"),
                    &format!("remove it, or use {}", quoted(known)),
                );
            }
        }
    }

    fn string(&mut self, value: &Spanned<DeValue<'_>>, key: &str) -> Option<String> {
        match value.get_ref().as_str() {
            Some(text) => Some(text.to_owned()),
            None => {
                self.report(
                    "LM-M004",
                    value.span(),
                    format!("`{key}` must be text in quotes"),
                    &format!("write it as `{key} = \"...\"`"),
                );
                None
            }
        }
    }

    fn table<'v, 'i>(
        &mut self,
        value: &'v Spanned<DeValue<'i>>,
        key: &str,
    ) -> Option<&'v DeTable<'i>> {
        let table = value.get_ref().as_table();
        if table.is_none() {
            self.report(
                "LM-M004",
                value.span(),
                format!("`{key}` must be a table"),
                &format!("write it as `[{key}]` on a line of its own, with its settings below"),
            );
        }
        table
    }

    fn bins(&mut self, table: &DeTable<'_>) -> Bins {
        self.keys(table, &["layout", "server"], "`[bins]`");
        let mut bins = Bins::default();
        if let Some(value) = table.get("layout")
            && let Some(layout) = self.string(value, "layout")
        {
            match layout.as_str() {
                "single" => bins.layout = Layout::Single,
                "split" => bins.layout = Layout::Split,
                "multicall" => bins.layout = Layout::Multicall,
                other => self.report(
                    "LM-M004",
                    value.span(),
                    format!("`{other}` is not a layout of binaries"),
                    "use `single`, `split` or `multicall`",
                ),
            }
        }
        if let Some(value) = table.get("server") {
            match value.get_ref().as_array() {
                Some(words) => {
                    bins.server = words
                        .iter()
                        .filter_map(|word| self.string(word, "server"))
                        .collect();
                }
                None => self.report(
                    "LM-M004",
                    value.span(),
                    "`server` must be a list of words".to_owned(),
                    "write it as `server = [\"mylang\", \"lsp\"]`",
                ),
            }
        }
        bins
    }

    fn vscode(&mut self, table: &DeTable<'_>) -> VsCode {
        self.keys(
            table,
            &["display-name", "publisher", "icon", "out"],
            "`[package.vscode]`",
        );
        let mut text = |key: &str| table.get(key).and_then(|value| self.string(value, key));
        VsCode {
            display_name: text("display-name"),
            publisher: text("publisher"),
            icon: text("icon"),
            out: text("out"),
        }
    }
}

impl Manifest {
    /// Reads a manifest. Each mistake is a diagnostic at its place in the file.
    pub fn parse(file: &SourceFile) -> Result<Manifest, Vec<Diagnostic>> {
        let mut reading = Reading {
            file,
            problems: Vec::new(),
        };
        let table = match DeTable::parse(file.text()) {
            Ok(table) => table,
            Err(error) => {
                let span = error.span().unwrap_or(0..0);
                let headline = format!("the file is not valid TOML: {}", error.message().trim());
                reading.report(
                    "LM-M001",
                    span,
                    headline,
                    "correct the line, as TOML writes `key = value` and `[table]`",
                );
                return Err(reading.problems);
            }
        };
        let table = table.get_ref();
        reading.keys(
            table,
            &["root", "bins", "project", "package"],
            "the top level",
        );
        let mut manifest = Manifest::default();
        if let Some(value) = table.get("root")
            && let Some(root) = reading.string(value, "root")
        {
            manifest.root = root;
        }
        if let Some(value) = table.get("bins")
            && let Some(bins) = reading.table(value, "bins")
        {
            manifest.bins = reading.bins(bins);
        }
        if let Some(value) = table.get("project")
            && let Some(project) = reading.table(value, "project")
        {
            reading.keys(project, &["manifest"], "`[project]`");
            if let Some(name) = project
                .get("manifest")
                .and_then(|value| reading.string(value, "manifest"))
            {
                manifest.project = Some(Project { manifest: name });
            }
        }
        if let Some(value) = table.get("package")
            && let Some(package) = reading.table(value, "package")
        {
            reading.keys(package, &["vscode"], "`[package]`");
            if let Some(value) = package.get("vscode")
                && let Some(vscode) = reading.table(value, "package.vscode")
            {
                manifest.package.vscode = Some(reading.vscode(vscode));
            }
        }
        if reading.problems.is_empty() {
            Ok(manifest)
        } else {
            Err(reading.problems)
        }
    }

    /// Reads a manifest from text, naming the file `name` in diagnostics.
    pub fn parse_text(name: &str, text: &str) -> Result<Manifest, Vec<Diagnostic>> {
        let file = SourceFile::new(FileId::new(0), name, text).map_err(|error| {
            vec![problem(
                "LM-M001",
                FileId::new(0),
                TextRange::empty(TextSize::from(0)),
                format!("the file cannot be read: {error}"),
                "make the file smaller",
            )]
        })?;
        Self::parse(&file)
    }
}
