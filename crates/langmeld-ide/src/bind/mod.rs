//! Names across the files of a project: where they are introduced, what uses them,
//! and renaming them safely.
//!
//! A language says how names bind with the [`bind` block](langmeld_meta::BindRules) of
//! its grammar. A [`Project`] holds the parsed files, and [`Project::bind`] works out
//! [`Bindings`].
//!
//! ```
//! use langmeld_ide::bind::Project;
//! use langmeld_ide::project::MemoryFiles;
//! use langmeld_meta::{Source, load_language};
//!
//! let loaded = load_language(Source::text(
//!     "lets.langmeld",
//!     "langmeld 1\nlanguage Lets { extensions \"lets\" }\n\
//!      tokens { skip ws = /[ \\n]+/\n word = preset ident(ascii) }\n\
//!      rule File = Let*\nrule Let = \"let\" name:Name \"=\" value:Use\n\
//!      rule Name = word\nrule Use = word\n\
//!      bind { scope File\n Let.name defines value\n Use refers value }\n",
//! ))?;
//! let mut files = MemoryFiles::default();
//! files.insert("main", "let a = b");
//! let mut project = Project::new(&loaded);
//! project.open("main", &files);
//! let found = project.bind();
//! assert_eq!(found.diagnostics()[0].code, "LM-B001");
//! assert_eq!(found.diagnostics()[0].message, "`b` is not defined");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod rename;
mod resolve;

use std::collections::{HashMap, HashSet};

use langmeld_meta::BindRules;
use langmeld_syntax::{
    Diagnostic, FileId, Language, Parse, SourceFile, SyntaxElement, SyntaxNode, TextEdit,
    TextRange, TextSize,
};

use crate::Served;
use crate::project::{FileProvider, resolve as relative};

pub use rename::RenameError;

/// The most bytes of text a file of a project may have, unless set otherwise.
pub const DEFAULT_FILE_LIMIT: usize = 16 << 20;

/// The most files a project holds, unless set otherwise.
pub const DEFAULT_FILES_LIMIT: usize = 10_000;

/// One file of a project.
#[derive(Debug)]
pub struct ProjectFile {
    source: SourceFile,
    parse: Parse,
}

impl ProjectFile {
    /// Its text and path.
    pub fn source(&self) -> &SourceFile {
        &self.source
    }

    /// Its tree and the problems of its syntax.
    pub fn parse(&self) -> &Parse {
        &self.parse
    }
}

/// The files of one language that see each other's names.
#[derive(Debug)]
pub struct Project<'l> {
    language: &'l Language,
    rules: &'l BindRules,
    /// By file number; `None` once removed.
    files: Vec<Option<ProjectFile>>,
    paths: HashMap<String, usize>,
    /// The numbers of removed files, which a new file takes.
    free: Vec<usize>,
    largest: usize,
    most: usize,
}

impl<'l> Project<'l> {
    /// A project with no files, for a language and the rules it binds names by. A
    /// language read from a file brings its rules; see [`Served`].
    pub fn new(language: impl Into<Served<'l>>) -> Self {
        let served = language.into();
        Self {
            language: served.language(),
            rules: served.bind_rules(),
            files: Vec::new(),
            paths: HashMap::new(),
            free: Vec::new(),
            largest: DEFAULT_FILE_LIMIT,
            most: DEFAULT_FILES_LIMIT,
        }
    }

    /// Sets the most bytes of text that a file may have. A larger one is not added,
    /// and an import of it is reported as one that cannot be read.
    #[must_use]
    pub fn largest_file(mut self, bytes: usize) -> Self {
        self.largest = bytes;
        self
    }

    /// Sets the most files the project holds. Files beyond it are not added, and
    /// imports of them are reported as ones that cannot be read.
    #[must_use]
    pub fn most_files(mut self, files: usize) -> Self {
        self.most = files;
        self
    }

    /// The language of the files.
    pub fn language(&self) -> &'l Language {
        self.language
    }

    /// The rules the names are bound by.
    pub fn rules(&self) -> &'l BindRules {
        self.rules
    }

    /// Whether the project holds as many files as it may.
    pub fn is_full(&self) -> bool {
        self.paths.len() >= self.most
    }

    /// Adds or replaces the file at `path` with `text`, and gives its number. The
    /// number stays the same while the file stays. Returns `None` if the text is too
    /// large (see [`largest_file`](Self::largest_file)), or the project is full.
    pub fn set(&mut self, path: &str, text: impl Into<String>) -> Option<FileId> {
        let text = text.into();
        if text.len() > self.largest {
            return None;
        }
        let index = match self.paths.get(path) {
            Some(&index) => index,
            None if self.is_full() => return None,
            None => self.free.pop().unwrap_or(self.files.len()),
        };
        let id = FileId::new(u32::try_from(index).ok()?);
        let source = SourceFile::new(id, path, text).ok()?;
        let parse = langmeld_syntax::parse(self.language, &source);
        let file = Some(ProjectFile { source, parse });
        match self.files.get_mut(index) {
            Some(place) => *place = file,
            None => self.files.push(file),
        }
        self.paths.insert(path.to_owned(), index);
        Some(id)
    }

    /// Reads the file at `path` and the files it imports, directly or not, that the
    /// project does not have yet. Returns the number of the file, if it can be read.
    pub fn open(&mut self, path: &str, files: &dyn FileProvider) -> Option<FileId> {
        let id = match self.paths.get(path) {
            Some(&index) => FileId::new(u32::try_from(index).ok()?),
            None => self.set(path, files.read(path)?)?,
        };
        // Read in the order the imports are written, each file's own imports first.
        let mut pending: Vec<String> = self.imports(id).into_iter().rev().collect();
        while let Some(path) = pending.pop() {
            if self.paths.contains_key(&path) {
                continue;
            }
            if let Some(imported) = files.read(&path).and_then(|text| self.set(&path, text)) {
                pending.extend(self.imports(imported).into_iter().rev());
            }
        }
        Some(id)
    }

    /// Forgets the file at `path`.
    pub fn remove(&mut self, path: &str) {
        if let Some(index) = self.paths.remove(path)
            && let Some(place) = self.files.get_mut(index)
        {
            *place = None;
            self.free.push(index);
        }
    }

    /// The paths that the imports of a file name.
    fn imports(&self, id: FileId) -> Vec<String> {
        let Some(file) = self.file(id) else {
            return Vec::new();
        };
        let (root, text) = (file.parse.syntax(), file.source.text());
        resolve::import_paths(self.language, self.rules, &root, text)
            .into_iter()
            .map(|(written, _)| relative(file.source.name(), &written))
            .collect()
    }

    /// The file numbered `id`.
    pub fn file(&self, id: FileId) -> Option<&ProjectFile> {
        self.files.get(usize::try_from(id.index()).ok()?)?.as_ref()
    }

    /// The file at `path`.
    pub fn file_at(&self, path: &str) -> Option<&ProjectFile> {
        self.files.get(*self.paths.get(path)?)?.as_ref()
    }

    /// The file that the import written at `offset` of the file `file` names, if the
    /// project has it.
    pub fn import_at(&self, file: FileId, offset: TextSize) -> Option<FileId> {
        let file = self.file(file)?;
        let (root, text) = (file.parse.syntax(), file.source.text());
        let (written, _) = resolve::import_paths(self.language, self.rules, &root, text)
            .into_iter()
            .find(|(_, range)| range.contains_inclusive(offset))?;
        let path = relative(file.source.name(), &written);
        Some(self.file_at(&path)?.source.id())
    }

    /// Every file, in the order first added.
    pub fn files(&self) -> impl Iterator<Item = &ProjectFile> {
        self.files.iter().flatten()
    }

    /// Binds the names of every file by the rules of the language.
    pub fn bind(&self) -> Bindings {
        ().resolve(self)
    }

    /// Binds the names by the rules of the language, asking `resolver` for computed
    /// names. This is what [`Resolver::resolve`] does unless a language changes it.
    pub fn bind_declared(&self, resolver: &(impl Resolver + ?Sized)) -> Bindings {
        resolve::bind(self, resolver)
    }

    /// The edits that give the name at `offset` of `file` the new name `new`, in
    /// every file that uses it. Refused if the new text is not a name, or if the
    /// change would make some name mean something else.
    pub fn rename(
        &self,
        bindings: &Bindings,
        file: FileId,
        offset: TextSize,
        new: &str,
    ) -> Result<Vec<TextEdit>, RenameError> {
        rename::rename(self, &(), bindings, file, offset, new)
    }

    /// Like [`rename`](Self::rename), checking the result with the language's own
    /// resolver.
    pub fn rename_with(
        &self,
        resolver: &dyn Resolver,
        bindings: &Bindings,
        file: FileId,
        offset: TextSize,
        new: &str,
    ) -> Result<Vec<TextEdit>, RenameError> {
        rename::rename(self, resolver, bindings, file, offset, new)
    }
}

/// How a language binds names, where the declared rules are not enough.
pub trait Resolver {
    /// The bindings of a project. Override to resolve in a way of your own, or call
    /// [`Project::bind_declared`] and change what it finds.
    fn resolve(&self, project: &Project<'_>) -> Bindings {
        project.bind_declared(self)
    }

    /// The names a definition declared `computed` introduces, with where each is
    /// written. `None` takes the name as written.
    fn computed_names(
        &self,
        file: &SourceFile,
        node: &SyntaxNode,
    ) -> Option<Vec<(String, TextRange)>> {
        let _ = (file, node);
        None
    }

    /// Names that are there without being written, such as built-in functions, as
    /// pairs of a kind of name and a name. Uses of them are not reported, and they
    /// are offered by completion.
    fn predefined(&self) -> &[(&str, &str)] {
        &[]
    }

    /// What a use declared `refers member of …` means. It takes the types of the
    /// language to know, so by default nothing is said about it.
    fn member(&self, access: &MemberAccess<'_>) -> Member {
        let _ = access;
        Member::Unknown
    }
}

/// A use of a member, for [`Resolver::member`] to look up.
#[derive(Debug)]
#[non_exhaustive]
pub struct MemberAccess<'a> {
    /// The project.
    pub project: &'a Project<'a>,
    /// The names bound so far: every plain use, and the members written earlier.
    pub bindings: &'a Bindings,
    /// The file of the use.
    pub file: FileId,
    /// The part naming whose member is meant.
    pub owner: SyntaxElement,
    /// The member's name.
    pub name: &'a str,
    /// Where it is written.
    pub range: TextRange,
}

/// The answer of [`Resolver::member`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Member {
    /// It means the definition at this place of [`Bindings::definitions`].
    Is(usize),
    /// The owner has no such member, which is reported.
    Missing,
    /// There is no telling, so nothing is reported.
    Unknown,
}

/// The declared rules and nothing else.
impl Resolver for () {}

/// A place that introduces a name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Definition {
    /// The file it is in.
    pub file: FileId,
    /// The name introduced.
    pub name: String,
    /// Where the name is written.
    pub range: TextRange,
    /// The node that introduces it.
    pub node: TextRange,
    /// The kind of name, as the rules name it.
    pub namespace: String,
    /// Whether the name was computed rather than written.
    pub computed: bool,
}

impl Definition {
    /// A definition of `name`, written at `range` of `file`.
    pub fn new(
        file: FileId,
        name: impl Into<String>,
        range: TextRange,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            file,
            name: name.into(),
            range,
            node: range,
            namespace: namespace.into(),
            computed: false,
        }
    }
}

/// A place that uses a name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reference {
    /// The file it is in.
    pub file: FileId,
    /// The name used.
    pub name: String,
    /// Where it is written.
    pub range: TextRange,
    /// The kind of name looked for.
    pub namespace: String,
    /// The definition it means, as its place in [`Bindings::definitions`].
    pub target: Option<usize>,
}

/// The names visible at one point.
#[derive(Clone, Debug)]
struct Scope {
    file: FileId,
    range: TextRange,
    parent: Option<usize>,
    /// Definitions made here, with the offset from which each is visible.
    names: Vec<(usize, TextSize)>,
    /// Definitions that other rules bring here: imports and included members.
    brought: Vec<usize>,
}

/// What binding a project found.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    definitions: Vec<Definition>,
    references: Vec<Reference>,
    diagnostics: Vec<Diagnostic>,
    scopes: Vec<Scope>,
    global: Vec<usize>,
    /// The paths that imports name and the project lacks.
    missing: Vec<String>,
}

impl Bindings {
    /// Every definition, in file and source order.
    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }

    /// Every use of a name, in file and source order.
    pub fn references(&self) -> &[Reference] {
        &self.references
    }

    /// Names that are not defined, defined twice, or imports that cannot be read.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// A warning for each definition that nothing uses. They are not among the
    /// [`diagnostics`](Self::diagnostics): a language that wants them adds them, as
    /// in `LanguageHooks::project_diagnostics`.
    pub fn unused(&self) -> Vec<Diagnostic> {
        let used: HashSet<usize> = self.references.iter().filter_map(|r| r.target).collect();
        self.definitions
            .iter()
            .enumerate()
            .filter(|(index, _)| !used.contains(index))
            .map(|(_, definition)| {
                let message = format!("`{}` is never used", definition.name);
                Diagnostic::warning("LM-B004", definition.file, definition.range, message)
                    .note("To fix this, use the name, or remove its definition.")
            })
            .collect()
    }

    /// The definitions that the node of the definition at `owner` holds: the names
    /// of the scopes it opens, which are its members.
    pub fn members_of(&self, owner: usize) -> impl Iterator<Item = usize> {
        let owner = self.definitions.get(owner).map(|d| (d.file, d.node));
        let inside = move |scope: &Scope| {
            owner.is_some_and(|(file, node)| scope.file == file && node.contains_range(scope.range))
        };
        // Only the scopes the node opens directly hold its members.
        self.scopes
            .iter()
            .filter(move |scope| {
                let parent = scope.parent.and_then(|index| self.scopes.get(index));
                inside(scope) && !parent.is_some_and(inside)
            })
            .flat_map(|scope| scope.names.iter().map(|&(index, _)| index))
    }

    /// Adds a definition, as a resolver of a language's own may, and gives its place.
    pub fn define(&mut self, definition: Definition) -> usize {
        self.definitions.push(definition);
        self.definitions.len() - 1
    }

    /// Makes the reference at `index` mean the definition at `target`, and drops the
    /// problem reported about it.
    pub fn retarget(&mut self, index: usize, target: usize) {
        let Some(reference) = self.references.get_mut(index) else {
            return;
        };
        reference.target = Some(target);
        let (file, range) = (reference.file, reference.range);
        self.diagnostics
            .retain(|found| (found.file, found.range) != (file, range));
    }

    /// The paths that imports name and the project does not hold, such as files that
    /// do not exist yet.
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Adds a problem.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// The place of the definition named at `offset`: the definition written there,
    /// or the one meant by the use written there.
    pub fn definition_index(&self, file: FileId, offset: TextSize) -> Option<usize> {
        let here =
            |other: FileId, range: TextRange| other == file && range.contains_inclusive(offset);
        self.definitions
            .iter()
            .position(|found| here(found.file, found.range))
            .or_else(|| {
                self.references
                    .iter()
                    .find(|found| here(found.file, found.range))?
                    .target
            })
    }

    /// The definition named at `offset`; see
    /// [`definition_index`](Self::definition_index).
    pub fn definition_at(&self, file: FileId, offset: TextSize) -> Option<&Definition> {
        self.definitions.get(self.definition_index(file, offset)?)
    }

    /// The uses that mean `definition`.
    pub fn references_to<'a>(
        &'a self,
        definition: &'a Definition,
    ) -> impl Iterator<Item = &'a Reference> {
        let index = self
            .definitions
            .iter()
            .position(|other| other == definition);
        self.references
            .iter()
            .filter(move |reference| index.is_some() && reference.target == index)
    }

    /// The innermost scope around `offset`.
    fn scope_at(&self, file: FileId, offset: TextSize) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.file == file && scope.range.contains_inclusive(offset))
            .min_by_key(|(_, scope)| scope.range.len())
            .map(|(index, _)| index)
    }

    /// The definitions of `namespace` that a use at `offset` could mean, nearest
    /// first, each name once.
    pub fn visible<'a>(
        &'a self,
        file: FileId,
        offset: TextSize,
        namespace: &'a str,
    ) -> impl Iterator<Item = &'a Definition> {
        let mut found: Vec<usize> = Vec::new();
        let mut known: HashSet<&str> = HashSet::new();
        let mut scope = self.scope_at(file, offset);
        let mut add = |index: usize| {
            let Some(definition) = self.definitions.get(index) else {
                return;
            };
            if definition.namespace == namespace && known.insert(&definition.name) {
                found.push(index);
            }
        };
        while let Some(current) = scope.and_then(|index| self.scopes.get(index)) {
            let here = current.names.iter().rev();
            here.filter(|&&(_, from)| from <= offset)
                .for_each(|&(index, _)| add(index));
            current.brought.iter().for_each(|&index| add(index));
            scope = current.parent;
        }
        self.global.iter().for_each(|&index| add(index));
        found
            .into_iter()
            .filter_map(|index| self.definitions.get(index))
    }
}
