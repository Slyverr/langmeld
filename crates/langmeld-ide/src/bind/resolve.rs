//! Applying the declared rules to the trees of a project.

use std::collections::{BTreeMap, HashMap};

use langmeld_meta::{BindRules, Binding, Path};
use langmeld_syntax::{
    Diagnostic, FileId, Fix, Label, Language, SourceFile, SyntaxElement, SyntaxKind, SyntaxNode,
    SyntaxToken, TextEdit, TextRange, TextSize, closest,
};

use super::{Bindings, Definition, Member, MemberAccess, Project, Reference, Resolver, Scope};
use crate::project::resolve as relative;

/// How many undefined names are given a suggestion.
const SUGGESTIONS: usize = 100;

type Computed<'a> = dyn Fn(&SourceFile, &SyntaxNode) -> Option<Vec<(String, TextRange)>> + 'a;

/// `LetStmt.name` as a node and a label.
fn split(path: &Path) -> (&str, Option<&str>) {
    let mut parts = path.parts.iter().map(String::as_str);
    (parts.next().unwrap_or_default(), parts.next())
}

/// The label that `of` and `from` parts of a line name: the last word of the path, so
/// that `Use.target` and `target` mean the same.
fn label_of(path: &Path) -> Option<&str> {
    path.parts.last().map(String::as_str)
}

/// Where an element is, without the space and comments inside it.
fn trimmed(language: &Language, element: &SyntaxElement) -> TextRange {
    let SyntaxElement::Node(node) = element else {
        return element.text_range();
    };
    // Only the ends of the node are walked, however large it is.
    let whole = node.text_range();
    let inside = |token: &SyntaxToken| whole.contains_range(token.text_range());
    let mut first = node.first_token();
    while let Some(token) = first.as_ref() {
        if !language.is_trivia(token.kind()) || !inside(token) {
            break;
        }
        first = token.next_token();
    }
    let Some(first) = first.filter(inside) else {
        return TextRange::empty(whole.start());
    };
    let mut last = node.last_token();
    while let Some(token) = last.as_ref() {
        if !language.is_trivia(token.kind()) || !inside(token) {
            break;
        }
        last = token.prev_token();
    }
    let last = last.filter(inside).unwrap_or_else(|| first.clone());
    first.text_range().cover(last.text_range())
}

/// The parts a rule's path names in `node`: its labelled parts, or itself.
fn parts(language: &Language, node: &SyntaxNode, label: Option<&str>) -> Vec<TextRange> {
    let found = match label {
        Some(label) => language.field(node, label),
        None => vec![SyntaxElement::Node(node.clone())],
    };
    found
        .iter()
        .map(|element| trimmed(language, element))
        .filter(|range| !range.is_empty())
        .collect()
}

/// A node, or a group of nodes, named by a rule.
fn is_node(language: &Language, node: &SyntaxNode, name: &str) -> bool {
    let kind = language.kind_name(node.kind());
    kind == name
        || language
            .schema()
            .union(name)
            .is_some_and(|members| members.iter().any(|member| member == kind))
}

/// The part of a file's text at `range`. Slicing the tree's text would walk it from
/// the start each time.
fn text_of(text: &str, range: TextRange) -> String {
    let range = usize::from(range.start())..usize::from(range.end());
    text.get(range).unwrap_or_default().to_owned()
}

/// The path text inside quotes, if it is quoted.
fn unquoted(text: &str) -> String {
    text.trim_matches(['"', '\'']).to_owned()
}

/// What the import rules find in a tree: each path and where it is written.
pub(super) fn import_paths(
    language: &Language,
    rules: &BindRules,
    root: &SyntaxNode,
    text: &str,
) -> Vec<(String, TextRange)> {
    let mut found = Vec::new();
    for rule in &rules.rules {
        let Binding::Imports { targets, from, .. } = rule else {
            continue;
        };
        // The node the path is written in: the one `from` names, or else the line's.
        let names: Vec<&str> = match from.parts.as_slice() {
            [_] | [] => targets.iter().map(|target| split(target).0).collect(),
            [node, ..] => vec![node.as_str()],
        };
        let label = label_of(from);
        // Whether nodes of each kind met are the one named.
        let mut named = HashMap::new();
        for node in root.descendants() {
            let wanted = named
                .entry(node.kind())
                .or_insert_with(|| names.iter().any(|name| is_node(language, &node, name)));
            if !*wanted {
                continue;
            }
            for range in parts(language, &node, label) {
                found.push((unquoted(&text_of(text, range)), range));
            }
        }
    }
    found
}

/// The problem of an import whose file the project does not hold.
fn cannot_read(path: &str, full: bool, file: FileId, range: TextRange) -> Diagnostic {
    if full {
        let message =
            format!("`{path}` was not read: the project already has all the files it holds");
        return Diagnostic::error("LM-B003", file, range, message)
            .note("To fix this, import fewer files, or split the project in two.");
    }
    let message = format!("the file `{path}` cannot be read");
    Diagnostic::error("LM-B003", file, range, message).note(
        "To fix this, check the path: it is written relative to the folder of this file, \
         with `/`, and the file must exist and be written in this language.",
    )
}

/// What the rules say about the nodes of one kind.
#[derive(Default)]
struct OfKind<'r> {
    opens: bool,
    /// The rules naming the kind, with the label each names.
    rules: Vec<(&'r Binding, Option<&'r str>)>,
}

impl<'r> OfKind<'r> {
    fn new(language: &'r Language, rules: &'r BindRules, node: &SyntaxNode) -> Self {
        let mut found = Self::default();
        for rule in &rules.rules {
            let targets = match rule {
                Binding::Scope { nodes, .. } => {
                    found.opens |= nodes.iter().any(|name| is_node(language, node, name));
                    continue;
                }
                Binding::Defines { targets, .. }
                | Binding::Refers { targets, .. }
                | Binding::Imports { targets, .. }
                | Binding::Member { targets, .. }
                | Binding::Includes { targets, .. } => targets,
                _ => continue,
            };
            for target in targets {
                let (name, label) = split(target);
                if is_node(language, node, name) {
                    found.rules.push((rule, label));
                }
            }
        }
        found
    }
}

/// The kinds of name looked for by uses written from `start`.
#[cfg(feature = "lsp")]
pub(crate) fn namespaces_at(
    language: &Language,
    rules: &BindRules,
    root: &SyntaxNode,
    start: TextSize,
) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    if start >= root.text_range().end() {
        return found;
    }
    let token = root.token_at_offset(start).right_biased();
    let Some(token) = token.filter(|token| token.text_range().start() == start) else {
        return found;
    };
    for node in token.parent().into_iter().flat_map(|node| node.ancestors()) {
        for rule in &rules.rules {
            let Binding::Refers {
                targets, namespace, ..
            } = rule
            else {
                continue;
            };
            let here = targets.iter().any(|target| {
                let (name, label) = split(target);
                is_node(language, &node, name)
                    && parts(language, &node, label)
                        .iter()
                        .any(|range| range.start() == start)
            });
            if here && !found.contains(namespace) {
                found.push(namespace.clone());
            }
        }
    }
    found
}

/// A rule that brings the members of what a reference means into a scope.
struct Include {
    file: FileId,
    /// The part whose scope gains the names.
    into: TextRange,
    /// The reference naming whose members.
    of: TextRange,
}

/// A rule that brings the names of another file into a scope.
struct Import {
    scope: usize,
    namespace: String,
    file: FileId,
}

struct Binder<'p, 'l> {
    project: &'p Project<'l>,
    computed: &'p Computed<'p>,
    bindings: Bindings,
    /// The scope each reference is written in.
    from: Vec<usize>,
    /// Whether each definition must be the only one of its name.
    unique: Vec<bool>,
    /// The outermost scope of each file.
    roots: HashMap<FileId, usize>,
    includes: Vec<Include>,
    imports: Vec<Import>,
    /// The definitions made in each scope under each name, in order.
    local: HashMap<usize, HashMap<String, Vec<(usize, TextSize)>>>,
    /// The definitions brought into each scope under each name.
    brought: HashMap<usize, HashMap<String, Vec<usize>>>,
    /// The global definitions of each name.
    global: HashMap<String, Vec<usize>>,
    /// The references to members, with the part naming whose.
    members: BTreeMap<usize, SyntaxElement>,
}

impl Binder<'_, '_> {
    fn open(&mut self, file: FileId, range: TextRange, parent: Option<usize>) -> usize {
        self.bindings.scopes.push(Scope {
            file,
            range,
            parent,
            names: Vec::new(),
            brought: Vec::new(),
        });
        self.bindings.scopes.len() - 1
    }

    fn file(&mut self, id: FileId) {
        let project = self.project;
        let language = project.language;
        let Some(file) = project.file(id) else { return };
        let root = file.parse.syntax();
        let outermost = self.open(id, root.text_range(), None);
        self.roots.insert(id, outermost);
        // Scopes under way, with where each node ends.
        let mut scopes = vec![(outermost, root.text_range().end())];
        let mut kinds: HashMap<SyntaxKind, OfKind<'_>> = HashMap::new();
        for node in root.descendants() {
            let start = node.text_range().start();
            while scopes.len() > 1 && scopes.last().is_some_and(|&(_, end)| end <= start) {
                scopes.pop();
            }
            let mut current = scopes.last().map_or(outermost, |&(scope, _)| scope);
            let kind = kinds
                .entry(node.kind())
                .or_insert_with(|| OfKind::new(language, project.rules, &node));
            if kind.opens && node != root {
                current = self.open(id, node.text_range(), Some(current));
                scopes.push((current, node.text_range().end()));
            }
            for &(rule, label) in &kind.rules {
                self.apply(file.source(), &node, current, rule, label);
            }
        }
    }

    /// Applies a rule naming the part `label` of `node`, which is in `scope`.
    fn apply(
        &mut self,
        source: &SourceFile,
        node: &SyntaxNode,
        scope: usize,
        rule: &Binding,
        label: Option<&str>,
    ) {
        let language = self.project.language;
        let id = source.id();
        match rule {
            Binding::Defines {
                namespace,
                modifiers,
                ..
            } => self.defines(source, node, scope, label, namespace, modifiers),
            Binding::Refers { namespace, .. } => {
                for range in parts(language, node, label) {
                    self.bindings.references.push(Reference {
                        file: id,
                        name: text_of(source.text(), range),
                        range,
                        namespace: namespace.clone(),
                        target: None,
                    });
                    self.from.push(scope);
                }
            }
            Binding::Imports {
                namespace, from, ..
            } => {
                for range in parts(language, node, label_of(from)) {
                    let written = unquoted(&text_of(source.text(), range));
                    let path = relative(source.name(), &written);
                    match self.project.file_at(&path) {
                        Some(imported) => self.imports.push(Import {
                            scope,
                            namespace: namespace.clone(),
                            file: imported.source.id(),
                        }),
                        None => {
                            let found = cannot_read(&path, self.project.is_full(), id, range);
                            self.bindings.diagnostics.push(found);
                            self.bindings.missing.push(path);
                        }
                    }
                }
            }
            Binding::Member { of, .. } => {
                let Some(owner) =
                    label_of(of).and_then(|of| language.field(node, of).into_iter().next())
                else {
                    return;
                };
                for range in parts(language, node, label) {
                    let index = self.bindings.references.len();
                    self.members.insert(index, owner.clone());
                    self.bindings.references.push(Reference {
                        file: id,
                        name: text_of(source.text(), range),
                        range,
                        namespace: "member".to_owned(),
                        target: None,
                    });
                    self.from.push(scope);
                }
            }
            Binding::Includes { of, .. } => {
                let into = parts(language, node, label);
                for of in parts(language, node, label_of(of)) {
                    for &into in &into {
                        self.includes.push(Include { file: id, into, of });
                    }
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)] // The parts of one rule applied to one node.
    fn defines(
        &mut self,
        source: &SourceFile,
        node: &SyntaxNode,
        scope: usize,
        label: Option<&str>,
        namespace: &str,
        modifiers: &[String],
    ) {
        let language = self.project.language;
        let has = |word: &str| modifiers.iter().any(|modifier| modifier == word);
        let mut into = scope;
        if has("in parent") {
            into = self
                .bindings
                .scopes
                .get(scope)
                .and_then(|scope| scope.parent)
                .unwrap_or(scope);
        }
        let computed = has("computed")
            .then(|| (self.computed)(source, node))
            .flatten();
        let is_computed = computed.is_some();
        let names = computed.unwrap_or_else(|| {
            parts(language, node, label)
                .into_iter()
                .map(|range| (text_of(source.text(), range), range))
                .collect()
        });
        let whole = trimmed(language, &SyntaxElement::Node(node.clone()));
        for (name, range) in names {
            let visible_from = if has("hoisted") {
                TextSize::new(0)
            } else if has("after") {
                whole.end()
            } else {
                range.end()
            };
            let index = self.bindings.define(Definition {
                file: source.id(),
                name,
                range,
                node: whole,
                namespace: namespace.to_owned(),
                computed: is_computed,
            });
            // A name shared by every file is one that may not be defined twice.
            self.unique.push(has("unique") || has("global"));
            let name = self.bindings.definitions.get(index);
            let name = name.map(|found| found.name.clone()).unwrap_or_default();
            // A global name belongs to every file alike, so that two files agree on
            // which definition of a repeated one counts: the first.
            if has("global") {
                self.bindings.global.push(index);
                self.global.entry(name).or_default().push(index);
            } else if let Some(scope) = self.bindings.scopes.get_mut(into) {
                scope.names.push((index, visible_from));
                let places = self.local.entry(into).or_default().entry(name).or_default();
                places.push((index, visible_from));
            }
        }
    }

    /// Reports names that must be the only ones and are not.
    fn repeats(&mut self) {
        let definitions = &self.bindings.definitions;
        let mut found = Vec::new();
        let locals = self.bindings.scopes.iter().map(|scope| {
            let names = scope.names.iter().map(|&(index, _)| index);
            names.collect::<Vec<_>>()
        });
        for group in locals.chain([self.bindings.global.clone()]) {
            let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
            for index in group {
                let Some(again) = definitions.get(index) else {
                    continue;
                };
                let key = (again.name.as_str(), again.namespace.as_str());
                let &mut first = seen.entry(key).or_insert(index);
                let unique = self.unique.get(index) == Some(&true);
                if let Some(first) = definitions.get(first).filter(|_| unique && first != index) {
                    let message = format!("`{}` is defined twice", again.name);
                    found.push(
                        Diagnostic::error("LM-B002", again.file, again.range, message)
                            .label(Label::new(first.file, first.range, "first defined here"))
                            .note("To fix this, rename one of the two, or remove one."),
                    );
                }
            }
        }
        self.bindings.diagnostics.extend(found);
    }

    fn is_in(&self, index: usize, namespace: &str) -> bool {
        let definition = self.bindings.definitions.get(index);
        definition.is_some_and(|found| found.namespace == namespace)
    }

    /// What `name` means for a use at `offset` of `scope`.
    fn lookup(&self, scope: usize, name: &str, namespace: &str, offset: TextSize) -> Option<usize> {
        let mut current = Some(scope);
        while let Some(index) = current {
            // The latest of the names visible here hides the earlier ones.
            let places = self.local.get(&index).and_then(|names| names.get(name));
            let here = places.and_then(|places| {
                places
                    .iter()
                    .filter(|&&(found, from)| from <= offset && self.is_in(found, namespace))
                    .max_by_key(|(_, from)| *from)
                    .map(|&(found, _)| found)
            });
            let found = here.or_else(|| {
                let brought = self.brought.get(&index)?.get(name)?;
                brought
                    .iter()
                    .copied()
                    .find(|&found| self.is_in(found, namespace))
            });
            if found.is_some() {
                return found;
            }
            current = self.bindings.scopes.get(index)?.parent;
        }
        let global = self.global.get(name)?;
        global
            .iter()
            .copied()
            .find(|&found| self.is_in(found, namespace))
    }

    fn add_brought(&mut self, scope: usize, names: Vec<usize>) {
        for &index in &names {
            if let Some(found) = self.bindings.definitions.get(index) {
                let names = self.brought.entry(scope).or_default();
                names.entry(found.name.clone()).or_default().push(index);
            }
        }
        if let Some(scope) = self.bindings.scopes.get_mut(scope) {
            scope.brought.extend(names);
        }
    }

    /// Puts into scopes the names that imports and includes bring.
    fn bring(&mut self) {
        for import in std::mem::take(&mut self.imports) {
            let names: Vec<usize> = self
                .roots
                .get(&import.file)
                .and_then(|&root| self.bindings.scopes.get(root))
                .into_iter()
                .flat_map(|root| root.names.iter().map(|&(index, _)| index))
                .filter(|&index| self.is_in(index, &import.namespace))
                .collect();
            self.add_brought(import.scope, names);
        }
        let includes = std::mem::take(&mut self.includes);
        if includes.is_empty() {
            return;
        }
        let used: HashMap<(FileId, TextRange), usize> = self
            .bindings
            .references
            .iter()
            .enumerate()
            .map(|(index, found)| ((found.file, found.range), index))
            .collect();
        // The scopes of each file.
        let mut of_file: HashMap<FileId, Vec<usize>> = HashMap::new();
        for (index, scope) in self.bindings.scopes.iter().enumerate() {
            of_file.entry(scope.file).or_default().push(index);
        }
        for include in includes {
            let target = used.get(&(include.file, include.of)).and_then(|&index| {
                let reference = self.bindings.references.get(index)?;
                let scope = *self.from.get(index)?;
                let offset = include.of.start();
                self.lookup(scope, &reference.name, &reference.namespace, offset)
            });
            let Some(owner) = target.and_then(|t| self.bindings.definitions.get(t)) else {
                continue;
            };
            let (file, node) = (owner.file, owner.node);
            let scopes = &self.bindings.scopes;
            let inside = |index: usize| {
                scopes
                    .get(index)
                    .is_some_and(|scope| node.contains_range(scope.range))
            };
            // The scopes the owner's node opens directly.
            let members: Vec<usize> = of_file
                .get(&file)
                .into_iter()
                .flatten()
                .filter(|&&index| {
                    let parent = scopes.get(index).and_then(|scope| scope.parent);
                    inside(index) && !parent.is_some_and(inside)
                })
                .filter_map(|&index| scopes.get(index))
                .flat_map(|scope| scope.names.iter().map(|&(index, _)| index))
                .collect();
            // The innermost scope that holds the part which gains the names: the part
            // itself if it opens one, and otherwise the scope around it.
            let into = of_file
                .get(&include.file)
                .into_iter()
                .flatten()
                .copied()
                .filter_map(|index| Some((index, scopes.get(index)?.range)))
                .filter(|(_, range)| range.contains_range(include.into))
                .min_by_key(|(_, range)| range.len())
                .map(|(index, _)| index);
            if let Some(into) = into {
                self.add_brought(into, members);
            }
        }
    }

    /// Finds what every use means, and reports those that mean nothing.
    fn resolve(&mut self, predefined: &[(&str, &str)]) {
        let mut suggested = 0;
        for index in 0..self.bindings.references.len() {
            if self.members.contains_key(&index) {
                continue;
            }
            let Some(reference) = self.bindings.references.get(index) else {
                continue;
            };
            let Some(&scope) = self.from.get(index) else {
                continue;
            };
            let offset = reference.range.start();
            let target = self.lookup(scope, &reference.name, &reference.namespace, offset);
            if let Some(reference) = self.bindings.references.get_mut(index) {
                reference.target = target;
            }
            if target.is_none()
                && let Some(reference) = self.bindings.references.get(index)
                && !predefined.iter().any(|&(namespace, name)| {
                    (namespace, name) == (&reference.namespace, &reference.name)
                })
            {
                let message = format!("`{}` is not defined", reference.name);
                let mut found =
                    Diagnostic::error("LM-B001", reference.file, reference.range, message);
                // A suggestion looks at every visible name, so only so many get one.
                let suggest = suggested < SUGGESTIONS;
                suggested += 1;
                let candidates: Vec<&str> = suggest
                    .then(|| {
                        self.bindings
                            .visible(reference.file, offset, &reference.namespace)
                    })
                    .into_iter()
                    .flatten()
                    .map(|d| d.name.as_str())
                    .chain(
                        predefined
                            .iter()
                            .filter(|&&(namespace, _)| suggest && namespace == reference.namespace)
                            .map(|&(_, name)| name),
                    )
                    .collect();
                found = match closest(&reference.name, &candidates) {
                    Some(name) => {
                        let edit = TextEdit::new(reference.file, reference.range, name);
                        found
                            .note(format!("did you mean `{name}`?"))
                            .fix(Fix::new(format!("use `{name}`"), vec![edit]))
                    }
                    None => found.note(format!(
                        "To fix this, define `{0}` before it is used, check the spelling, or \
                         import the file that defines it.",
                        reference.name
                    )),
                };
                self.bindings.diagnostics.push(found);
            }
        }
    }

    /// Asks the language what each use of a member means, in source order, so that
    /// the owner of `a.b.c` is known before its member.
    fn members(&mut self, resolver: &(impl Resolver + ?Sized)) {
        for (index, owner) in std::mem::take(&mut self.members) {
            let Some(reference) = self.bindings.references.get(index) else {
                continue;
            };
            let (file, range, name) = (reference.file, reference.range, reference.name.clone());
            let answer = resolver.member(&MemberAccess {
                project: self.project,
                bindings: &self.bindings,
                file,
                owner,
                name: &name,
                range,
            });
            match answer {
                Member::Is(target) if target < self.bindings.definitions.len() => {
                    if let Some(reference) = self.bindings.references.get_mut(index) {
                        reference.target = Some(target);
                    }
                }
                Member::Missing => {
                    let message = format!("there is no member `{name}`");
                    let found = Diagnostic::error("LM-B005", file, range, message)
                        .note("To fix this, check the spelling of the member, or define it.");
                    self.bindings.diagnostics.push(found);
                }
                Member::Is(_) | Member::Unknown => {}
            }
        }
    }
}

pub(super) fn bind(project: &Project<'_>, resolver: &(impl Resolver + ?Sized)) -> Bindings {
    let computed = |file: &SourceFile, node: &SyntaxNode| resolver.computed_names(file, node);
    let mut binder = Binder {
        project,
        computed: &computed,
        bindings: Bindings::default(),
        from: Vec::new(),
        unique: Vec::new(),
        roots: HashMap::new(),
        includes: Vec::new(),
        imports: Vec::new(),
        local: HashMap::new(),
        brought: HashMap::new(),
        global: HashMap::new(),
        members: BTreeMap::new(),
    };
    let files: Vec<_> = project.files().map(|file| file.source.id()).collect();
    for file in files {
        binder.file(file);
    }
    binder.repeats();
    binder.bring();
    binder.resolve(resolver.predefined());
    binder.members(resolver);
    let mut bindings = binder.bindings;
    bindings
        .diagnostics
        .sort_by_key(|d| (d.file, d.range.start(), d.code));
    bindings
}
