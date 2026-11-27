//! Loading a language from `.langmeld` files: reading them and what they import,
//! checking them, and building the language.

use std::collections::{BTreeMap, HashSet};

use langmeld_syntax::{
    Diagnostic, Externals, FileId, Language, LexCursor, Parser, Severity, SourceFile, TextRange,
    TextSize, parse,
};

use crate::declared::{Bindings, Editor};
use crate::lower::{Options, Unit, lower};
use crate::problem::problem;
use crate::tree::Node;
use crate::{meta, render};

/// Where files come from: the disk, or memory for programs that have no disk.
pub trait Files {
    /// The text of the file at `path`, or why it cannot be read.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// Files kept in memory, by path.
#[derive(Clone, Debug, Default)]
pub struct MemoryFiles(BTreeMap<String, String>);

impl MemoryFiles {
    /// No files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file.
    #[must_use]
    pub fn with(mut self, path: impl Into<String>, text: impl Into<String>) -> Self {
        self.0.insert(path.into(), text.into());
        self
    }
}

impl Files for MemoryFiles {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| "there is no such file".to_owned())
    }
}

/// Files on the disk.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskFiles;

#[cfg(not(target_arch = "wasm32"))]
impl Files for DiskFiles {
    fn read(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|error| {
            let reason = error.to_string();
            let reason = reason.split(" (os error").next().unwrap_or(&reason);
            reason.to_lowercase()
        })
    }
}

enum Origin {
    #[cfg(not(target_arch = "wasm32"))]
    Path(String),
    Text {
        name: String,
        text: String,
    },
    Embedded(Vec<u8>),
}

/// Where a language is loaded from, and the Rust code it needs.
///
/// ```
/// use langmeld_meta::{Source, load_language};
///
/// let loaded = load_language(Source::text(
///     "bang.langmeld",
///     "langmeld 1\nlanguage Bang { extensions \"bang\" }\nrule File = \"!\"*\n",
/// ))
/// .expect("the file is valid");
/// assert_eq!(loaded.extensions, ["bang"]);
/// ```
pub struct Source {
    origin: Origin,
    files: Option<MemoryFiles>,
    externals: Externals,
    generic: bool,
}

impl Source {
    /// The root file at `path`; the files it imports are found relative to it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn path(path: impl AsRef<std::path::Path>) -> Self {
        let shown = path.as_ref().to_string_lossy().replace('\\', "/");
        Self::new(Origin::Path(shown))
    }

    /// A root file given as text. `name` is how diagnostics call it, and imports are
    /// relative to it, found among the files added with [`Source::file`].
    pub fn text(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(Origin::Text {
            name: name.into(),
            text: text.into(),
        })
    }

    /// A language compiled ahead of time by [`build::embed`](crate::build::embed), as a
    /// program includes it. Its custom rules and tokens still need their code.
    pub fn embedded(bytes: &[u8]) -> Self {
        Self::new(Origin::Embedded(bytes.to_vec()))
    }

    fn new(origin: Origin) -> Self {
        Self {
            origin,
            files: None,
            externals: Externals::new(),
            generic: false,
        }
    }

    /// A file that imports can name. Imports are then found in memory, not on the disk.
    #[must_use]
    pub fn file(mut self, path: impl Into<String>, text: impl Into<String>) -> Self {
        self.files = Some(self.files.take().unwrap_or_default().with(path, text));
        self
    }

    /// The code of the custom rule called `name`.
    #[must_use]
    pub fn extern_rule(
        mut self,
        name: impl Into<String>,
        parse: impl Fn(&mut Parser<'_>) + Send + Sync + 'static,
    ) -> Self {
        self.externals = self.externals.rule(name, parse);
        self
    }

    /// The code of the external token called `name`.
    #[must_use]
    pub fn extern_token(
        mut self,
        name: impl Into<String>,
        lex: impl Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync + 'static,
    ) -> Self {
        self.externals = self.externals.token(name, lex);
        self
    }

    /// Reads custom rules as one error each, because no Rust code is given for them.
    #[must_use]
    pub fn generic(mut self) -> Self {
        self.generic = true;
        self
    }
}

/// A language that was loaded, with what its files said besides the grammar.
#[derive(Debug)]
pub struct Loaded {
    /// The language, checked and ready to parse.
    pub language: Language,
    /// The files read, root first. Their numbers are what diagnostics name them by.
    pub files: Vec<SourceFile>,
    /// The file extensions of the language, without dots.
    pub extensions: Vec<String>,
    /// What the `bind` blocks declare.
    pub bindings: Bindings,
    /// What the `editor` blocks declare.
    pub editor: Editor,
}

/// Why a language could not be loaded: every mistake found, each with its place.
#[derive(Debug)]
pub struct LoadError {
    /// The files read, root first.
    pub files: Vec<SourceFile>,
    /// The mistakes, in the order of their files and places.
    pub diagnostics: Vec<Diagnostic>,
}

impl LoadError {
    /// The mistakes as text, each with the line it points at.
    pub fn rendered(&self) -> String {
        render::render(&self.files, &self.diagnostics)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.rendered().trim_end())
    }
}

impl std::error::Error for LoadError {}

/// Gives a mistake the note that tells how to solve it, if it has none.
pub(crate) fn explain(mut found: Diagnostic) -> Diagnostic {
    if !found.notes.is_empty() {
        return found;
    }
    let fix = match found.code {
        "LM-P001" => match found
            .message
            .strip_prefix("expected ")
            .and_then(|rest| rest.split_once(", found "))
        {
            Some((wanted, got)) => format!("write {wanted} where {got} is, or remove {got}"),
            None => "write what is expected, or remove what is in the way".to_owned(),
        },
        "LM-L001" => "remove it, or put it in quotes if it is text".to_owned(),
        "LM-L002" => "close the quotes before the end of the line".to_owned(),
        _ => "correct the line, following the format reference".to_owned(),
    };
    found.notes.push(format!("To fix this, {fix}."));
    found
}

/// `dir/` of a path, empty for a bare name.
fn directory(path: &str) -> &str {
    path.rfind('/').map_or("", |at| &path[..=at])
}

/// The path an import names, relative to the file that imports it.
fn resolve(from: &str, import: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in directory(from).split('/').chain(import.split('/')) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if from.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

const MOST_IMPORT_DEPTH: usize = 64;

struct Collector<'a> {
    files: &'a dyn Files,
    sources: Vec<SourceFile>,
    units: Vec<Unit>,
    problems: Vec<Diagnostic>,
    loaded: HashSet<String>,
    stack: Vec<String>,
}

impl Collector<'_> {
    fn visit(&mut self, name: String, text: String, importer: Option<(FileId, TextRange)>) {
        let id = FileId::new(u32::try_from(self.sources.len()).unwrap_or(u32::MAX));
        let file = match SourceFile::new(id, name.clone(), text) {
            Ok(file) => file,
            Err(error) => {
                let (file, range) = importer.unwrap_or((id, TextRange::empty(TextSize::from(0))));
                self.problems.push(problem(
                    "LM-F007",
                    file,
                    range,
                    format!("cannot read `{name}`: {error}"),
                    "split the file into smaller ones",
                ));
                return;
            }
        };
        self.loaded.insert(name.clone());
        let parsed = parse(meta::language(), &file);
        self.problems
            .extend(parsed.diagnostics.iter().cloned().map(explain));
        let root = Node(parsed.syntax());
        let imports: Vec<_> = root
            .children()
            .filter(|item| item.is("Import"))
            .filter_map(|item| item.all("string").next())
            .collect();
        self.sources.push(file.clone());
        self.units.push(Unit { file, root });
        self.stack.push(name.clone());
        for written in imports {
            let path = written.text().trim_matches('"');
            let range = written.text_range();
            let target = resolve(&name, path);
            if self.stack.contains(&target) || self.stack.len() >= MOST_IMPORT_DEPTH {
                let mut chain = self.stack.clone();
                chain.push(target.clone());
                self.problems.push(problem(
                    "LM-F008",
                    id,
                    range,
                    format!(
                        "importing `{target}` here would go in a circle: {}",
                        chain.join(" -> ")
                    ),
                    "remove this import, or move what both files need into a third file",
                ));
            } else if !self.loaded.contains(&target) {
                match self.files.read(&target) {
                    Ok(text) => self.visit(target, text, Some((id, range))),
                    Err(reason) => self.problems.push(problem(
                        "LM-F007",
                        id,
                        range,
                        format!("cannot read `{target}`: {reason}"),
                        "check the path, which is relative to this file",
                    )),
                }
            }
        }
        self.stack.pop();
    }
}

fn stem(name: &str) -> String {
    let file = name.rsplit('/').next().unwrap_or(name);
    file.split('.').next().unwrap_or(file).to_owned()
}

/// Reads the language that `source` names.
pub fn load_language(source: Source) -> Result<Loaded, LoadError> {
    let Source {
        origin,
        files,
        externals,
        generic,
    } = source;
    let (name, root) = match origin {
        #[cfg(not(target_arch = "wasm32"))]
        Origin::Path(path) => {
            let read = DiskFiles.read(&path);
            (path, read)
        }
        Origin::Text { name, text } => (name, Ok::<_, String>(text)),
        Origin::Embedded(bytes) => return crate::build::read(&bytes, &externals),
    };
    let text = match root {
        Ok(text) => text,
        Err(reason) => {
            let found = problem(
                "LM-F007",
                FileId::new(0),
                TextRange::empty(TextSize::from(0)),
                format!("cannot read `{name}`: {reason}"),
                "check the path of the language file",
            );
            return Err(LoadError {
                files: Vec::new(),
                diagnostics: vec![found],
            });
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    let reader: &dyn Files = match &files {
        Some(memory) => memory,
        None => &DiskFiles,
    };
    #[cfg(target_arch = "wasm32")]
    let memory = files.unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    let reader: &dyn Files = &memory;
    let mut collector = Collector {
        files: reader,
        sources: Vec::new(),
        units: Vec::new(),
        problems: Vec::new(),
        loaded: HashSet::new(),
        stack: Vec::new(),
    };
    collector.visit(name.clone(), text, None);
    let Collector {
        sources,
        units,
        mut problems,
        ..
    } = collector;
    let fail = |sources: Vec<SourceFile>, mut diagnostics: Vec<Diagnostic>| {
        diagnostics.sort_by_key(|found| (found.file, found.range.start()));
        LoadError {
            files: sources,
            diagnostics,
        }
    };
    if !problems.is_empty() {
        return Err(fail(sources, problems));
    }
    let options = Options {
        externals: &externals,
        generic,
    };
    let lowered = lower(&units, &options, &stem(&name));
    problems.extend(lowered.problems);
    // A name against the convention is still a name, so the grammar is checked as well
    // and its problems are shown together with them.
    let only_names = problems
        .iter()
        .all(|found| matches!(found.code, "LM-F003" | "LM-F004"));
    if !only_names {
        return Err(fail(sources, problems));
    }
    match lowered.builder.build() {
        Ok(language) if problems.is_empty() => Ok(Loaded {
            language,
            files: sources,
            extensions: lowered.extensions,
            bindings: lowered.bindings,
            editor: lowered.editor,
        }),
        Ok(_) => Err(fail(sources, problems)),
        Err(found) => {
            problems.extend(found);
            Err(fail(sources, problems))
        }
    }
}

/// The warnings of a loaded language as text, each with the line it points at.
pub fn render_warnings(loaded: &Loaded) -> String {
    let warnings: Vec<_> = loaded
        .language
        .warnings()
        .iter()
        .filter(|found| found.severity == Severity::Warning)
        .cloned()
        .collect();
    render::render(&loaded.files, &warnings)
}
