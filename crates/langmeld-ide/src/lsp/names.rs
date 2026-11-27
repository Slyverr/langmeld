//! What the server answers about names: where they are defined and used, renaming
//! them, completing them, and what the rules about them add to colours and fixes.

use std::collections::{HashMap, HashSet};

use langmeld_meta::EditorRule;
use langmeld_syntax::{FileId, SourceFile, SyntaxElement, TextRange, TextSize, TokenClass};
use lsp_server::ErrorCode;
use lsp_types as lsp;

use super::features::{View, completion_kind, token_type, word_around};
use super::{LanguageHooks, Refusal, Server, position};
use crate::bind::{Definition, RenameError, Resolver, namespaces_at};

/// The most symbols of a workspace given for one query.
const SYMBOLS: usize = 1_000;

/// A name written somewhere: where, and which definition it means.
struct Occurrence {
    range: TextRange,
    definition: usize,
}

impl<H: LanguageHooks> Server<'_, H> {
    /// The file and offset a request points at, in a document of the language.
    fn pointed(&self, at: &lsp::TextDocumentPositionParams) -> Option<(View<'_>, TextSize)> {
        let view = self.view(&at.text_document.uri)?;
        let offset = position::offset(view.file, at.position, view.encoding);
        Some((view, offset))
    }

    fn occurrence(&self, file: FileId, offset: TextSize) -> Option<Occurrence> {
        if !self.binds() {
            return None;
        }
        let definition = self.bindings.definition_index(file, offset)?;
        let here =
            |other: FileId, range: TextRange| other == file && range.contains_inclusive(offset);
        let found = self.bindings.definitions().get(definition)?;
        let range = if here(found.file, found.range) {
            found.range
        } else {
            let mut uses = self.bindings.references().iter();
            uses.find(|used| here(used.file, used.range))?.range
        };
        Some(Occurrence { range, definition })
    }

    fn location(&self, file: FileId, range: TextRange) -> Option<lsp::Location> {
        let (uri, source) = self.place(file)?;
        Some(lsp::Location {
            uri,
            range: position::range(source, range, self.encoding),
        })
    }

    /// The editor's kind of each symbol of a file, by where its name starts.
    fn kinds(&self, file: FileId) -> HashMap<TextSize, String> {
        let Some(file) = self.project.file(file) else {
            return HashMap::new();
        };
        let view = View {
            language: self.language,
            editor: self.editor,
            file: file.source(),
            parse: file.parse(),
            encoding: self.encoding,
        };
        view.declarations()
            .into_iter()
            .map(|(_, name, kind)| (name.start(), kind))
            .collect()
    }

    /// The kind of each definition asked for, reading each file once.
    fn kind_of<'a>(
        &self,
        known: &'a mut HashMap<FileId, HashMap<TextSize, String>>,
        definition: &Definition,
    ) -> Option<&'a str> {
        known
            .entry(definition.file)
            .or_insert_with(|| self.kinds(definition.file))
            .get(&definition.range.start())
            .map(String::as_str)
    }

    pub(super) fn definition(&self, at: &lsp::TextDocumentPositionParams) -> Option<lsp::Location> {
        let (view, offset) = self.pointed(at)?;
        if let Some(found) = self.hooks.definition(view.file, view.parse, offset) {
            return Some(lsp::Location {
                uri: at.text_document.uri.clone(),
                range: position::range(view.file, found, view.encoding),
            });
        }
        if let Some(found) = self.occurrence(view.file.id(), offset) {
            let found = self.bindings.definitions().get(found.definition)?;
            return self.location(found.file, found.range);
        }
        // The path of an import leads to the file it names.
        let imported = self.project.import_at(view.file.id(), offset)?;
        self.location(imported, TextRange::empty(0.into()))
    }

    pub(super) fn references(
        &self,
        at: &lsp::TextDocumentPositionParams,
        declaration: bool,
    ) -> Option<Vec<lsp::Location>> {
        let (view, offset) = self.pointed(at)?;
        let found = self.occurrence(view.file.id(), offset)?;
        let definition = self.bindings.definitions().get(found.definition)?;
        let mut places: Vec<(FileId, TextRange)> = self
            .bindings
            .references()
            .iter()
            .filter(|used| used.target == Some(found.definition))
            .map(|used| (used.file, used.range))
            .collect();
        if declaration {
            places.push((definition.file, definition.range));
        }
        let name = |file: FileId| self.project.file(file).map(|f| f.source().name());
        places.sort_by_key(|&(file, range)| (name(file), range.start()));
        places.dedup();
        places
            .into_iter()
            .map(|(file, range)| self.location(file, range))
            .collect()
    }

    pub(super) fn prepare_rename(
        &self,
        at: &lsp::TextDocumentPositionParams,
    ) -> Option<lsp::PrepareRenameResponse> {
        let (view, offset) = self.pointed(at)?;
        let found = self.occurrence(view.file.id(), offset)?;
        let definition = self.bindings.definitions().get(found.definition)?;
        (!definition.computed).then(|| lsp::PrepareRenameResponse::RangeWithPlaceholder {
            range: position::range(view.file, found.range, view.encoding),
            placeholder: definition.name.clone(),
        })
    }

    // The protocol keys the edits by address.
    #[allow(clippy::mutable_key_type)]
    pub(super) fn rename(
        &self,
        params: &lsp::RenameParams,
    ) -> Result<Option<lsp::WorkspaceEdit>, Refusal> {
        let refused = |problem: RenameError| Refusal(ErrorCode::RequestFailed, problem.to_string());
        let Some((view, offset)) = self.pointed(&params.text_document_position) else {
            return Ok(None);
        };
        if !self.binds() {
            return Ok(None);
        }
        let (file, new) = (view.file.id(), params.new_name.as_str());
        let edits = match self.hooks.resolver() {
            Some(resolver) => self
                .project
                .rename_with(resolver, &self.bindings, file, offset, new),
            None => self.project.rename(&self.bindings, file, offset, new),
        };
        let edits = edits.map_err(refused)?;
        let mut changes: HashMap<lsp::Uri, Vec<lsp::TextEdit>> = HashMap::new();
        for edit in edits {
            if let Some(found) = self.location(edit.file, edit.range) {
                changes.entry(found.uri).or_default().push(lsp::TextEdit {
                    range: found.range,
                    new_text: edit.replacement,
                });
            }
        }
        Ok(Some(lsp::WorkspaceEdit {
            changes: Some(changes),
            ..lsp::WorkspaceEdit::default()
        }))
    }

    pub(super) fn hover(&self, at: &lsp::TextDocumentPositionParams) -> Option<lsp::Hover> {
        let (view, offset) = self.pointed(at)?;
        let id = view.file.id();
        let markdown = |value, range: Option<TextRange>| lsp::Hover {
            contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                kind: lsp::MarkupKind::Markdown,
                value,
            }),
            range: range.map(|range| position::range(view.file, range, view.encoding)),
        };
        if let Some(value) = self.hooks.hover(&self.project, id, offset) {
            return Some(markdown(value, None));
        }
        let found = self.occurrence(id, offset)?;
        let definition = self.bindings.definitions().get(found.definition)?;
        let described = self.describe(definition)?;
        Some(markdown(described, Some(found.range)))
    }

    /// What is known about a definition: its name and kind, where it is, the line
    /// that introduces it and the documentation above.
    fn describe(&self, definition: &Definition) -> Option<String> {
        let file = self.project.file(definition.file)?;
        let source = file.source();
        let name = source.name().rsplit('/').next().unwrap_or(source.name());
        let line = source
            .line_index()
            .line_col(definition.range.start())
            .and_then(|at| source.line_index().line_range(at.line))
            .and_then(|range| {
                let range = usize::from(range.start())..usize::from(range.end());
                source.text().get(range)
            })
            .unwrap_or_default();
        let language = self.language.name().to_lowercase();
        let mut shown = format!(
            "`{}`: {}, in {name}\n\n```{language}\n{}\n```",
            definition.name,
            definition.namespace,
            line.trim()
        );
        let documentation = self.documentation(&file.parse().syntax(), definition.node.start());
        if !documentation.is_empty() {
            shown.push_str("\n\n");
            shown.push_str(&documentation);
        }
        Some(shown)
    }

    /// The text of the documentation comments just before `start`.
    fn documentation(&self, root: &langmeld_syntax::SyntaxNode, start: TextSize) -> String {
        let mut token = root.token_at_offset(start).right_biased();
        let mut lines = Vec::new();
        while let Some(before) = token.and_then(|token| token.prev_token()) {
            match self.language.token_class(before.kind()) {
                Some(TokenClass::DocComment) => {
                    let marks = |c: char| c.is_whitespace() || "/#*!-".contains(c);
                    lines.push(before.text().trim_matches(marks).to_owned());
                }
                _ if self.language.is_trivia(before.kind()) => {}
                _ => break,
            }
            token = Some(before);
        }
        lines.reverse();
        lines.join("\n")
    }

    pub(super) fn completion(
        &self,
        at: &lsp::TextDocumentPositionParams,
    ) -> Option<Vec<lsp::CompletionItem>> {
        let document = self.open.get(at.text_document.uri.as_str())?;
        if let Some((kind, file)) = &document.manifest {
            let offset = position::offset(file, at.position, self.encoding);
            return Some(self.manifests.get(*kind)?.completions(file, offset));
        }
        let (view, offset) = self.pointed(at)?;
        let mut items = view.completions(offset);
        let extra = self.hooks.completions(view.file, view.parse, offset);
        items.extend(view.words(offset, extra));
        if self.binds() {
            items.extend(self.names_at(&view, offset));
            items.sort_by(|a, b| a.label.cmp(&b.label));
        }
        Some(items)
    }

    /// The names that may be written at `offset`, replacing the word there.
    fn names_at(&self, view: &View<'_>, offset: TextSize) -> Vec<lsp::CompletionItem> {
        let word = word_around(view.file.text(), offset);
        let namespaces = if word.is_empty() {
            self.namespaces_in_gap(view.file, word.start())
        } else {
            namespaces_at(
                self.language,
                self.project.rules(),
                &view.parse.syntax(),
                word.start(),
            )
        };
        let replaced = position::range(view.file, word, view.encoding);
        let item = |name: &str, kind, namespace: &str| lsp::CompletionItem {
            label: name.to_owned(),
            kind: Some(kind),
            detail: Some(namespace.to_owned()),
            text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                range: replaced,
                new_text: name.to_owned(),
            })),
            ..lsp::CompletionItem::default()
        };
        let mut kinds = HashMap::new();
        let mut items = Vec::new();
        let predefined = self.hooks.resolver().map_or(&[][..], Resolver::predefined);
        for namespace in &namespaces {
            let mut offered = HashSet::new();
            let visible = self
                .bindings
                .visible(view.file.id(), word.start(), namespace);
            for definition in visible {
                let kind = completion_kind(self.kind_of(&mut kinds, definition));
                offered.insert(definition.name.as_str());
                items.push(item(&definition.name, kind, namespace));
            }
            // A name written hides the predefined one.
            items.extend(
                predefined
                    .iter()
                    .filter(|&&(found, name)| found == namespace && !offered.contains(name))
                    .map(|&(_, name)| item(name, completion_kind(None), namespace)),
            );
        }
        items
    }

    /// The kinds of name a use written where nothing is yet would look for: found
    /// by writing a name there, one of each kind, and reading the result.
    fn namespaces_in_gap(&self, file: &SourceFile, offset: TextSize) -> Vec<String> {
        let mut tried: Vec<&str> = Vec::new();
        let mut found = Vec::new();
        let written = self
            .bindings
            .definitions()
            .iter()
            .filter(|definition| !definition.computed)
            .map(|definition| (definition.namespace.as_str(), definition.name.as_str()));
        let predefined = self.hooks.resolver().map_or(&[][..], Resolver::predefined);
        for (namespace, name) in written.chain(predefined.iter().copied()) {
            if tried.contains(&namespace) {
                continue;
            }
            tried.push(namespace);
            let mut text = file.text().to_owned();
            let at = usize::from(offset).min(text.len());
            if !text.is_char_boundary(at) {
                continue;
            }
            text.insert_str(at, name);
            let Ok(written) = SourceFile::new(file.id(), file.name(), text) else {
                continue;
            };
            let parse = langmeld_syntax::parse(self.language, &written);
            let here = namespaces_at(self.language, self.project.rules(), &parse.syntax(), offset);
            if here.iter().any(|here| here == namespace) {
                found.push(namespace.to_owned());
            }
        }
        found
    }

    pub(super) fn workspace_symbols(&self, query: &str) -> Vec<lsp::SymbolInformation> {
        let query = query.to_lowercase();
        let mut files: Vec<_> = self.project.files().collect();
        files.sort_by(|a, b| a.source().name().cmp(b.source().name()));
        let listed =
            (self.editor.rules.iter()).any(|rule| matches!(rule, EditorRule::Symbol { .. }));
        let mut found = Vec::new();
        for file in files {
            let view = View {
                language: self.language,
                editor: self.editor,
                file: file.source(),
                parse: file.parse(),
                encoding: self.encoding,
            };
            // The editor rules say what a symbol is; without them, any definition.
            let symbols: Vec<(TextRange, String, String)> = if listed {
                view.declarations()
                    .into_iter()
                    .map(|(_, name, kind)| (name, view.text(name).to_owned(), kind))
                    .collect()
            } else {
                let id = file.source().id();
                self.bindings
                    .definitions()
                    .iter()
                    .filter(|definition| definition.file == id)
                    .map(|d| (d.range, d.name.clone(), String::new()))
                    .collect()
            };
            for (range, name, kind) in symbols {
                if found.len() >= SYMBOLS {
                    return found;
                }
                if !name.to_lowercase().contains(&query) {
                    continue;
                }
                let Some(location) = self.location(file.source().id(), range) else {
                    continue;
                };
                #[allow(deprecated)] // The field must be given.
                found.push(lsp::SymbolInformation {
                    name,
                    kind: super::features::symbol_kind(&kind),
                    tags: None,
                    deprecated: None,
                    location,
                    container_name: None,
                });
            }
        }
        found
    }

    // The protocol keys the edits by address.
    #[allow(clippy::mutable_key_type)]
    pub(super) fn code_actions(
        &self,
        params: &lsp::CodeActionParams,
    ) -> Option<Vec<lsp::CodeActionOrCommand>> {
        let document = self.open.get(params.text_document.uri.as_str())?;
        let before =
            |a: lsp::Position, b: lsp::Position| (a.line, a.character) <= (b.line, b.character);
        let mut actions = Vec::new();
        for problem in self.problems(document, &mut Vec::new()) {
            let Some(shown) = self.shown(&problem) else {
                continue;
            };
            let touches = before(shown.range.start, params.range.end)
                && before(params.range.start, shown.range.end);
            if !touches {
                continue;
            }
            for fix in &problem.fixes {
                let mut changes: HashMap<lsp::Uri, Vec<lsp::TextEdit>> = HashMap::new();
                for edit in &fix.edits {
                    if let Some(found) = self.location(edit.file, edit.range) {
                        changes.entry(found.uri).or_default().push(lsp::TextEdit {
                            range: found.range,
                            new_text: edit.replacement.clone(),
                        });
                    }
                }
                actions.push(lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
                    title: fix.title.clone(),
                    kind: Some(lsp::CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![shown.clone()]),
                    edit: Some(lsp::WorkspaceEdit {
                        changes: Some(changes),
                        ..lsp::WorkspaceEdit::default()
                    }),
                    ..lsp::CodeAction::default()
                }));
            }
        }
        Some(actions)
    }

    pub(super) fn semantic_tokens(&self, uri: &lsp::Uri) -> Option<lsp::SemanticTokens> {
        let view = self.view(uri)?;
        Some(lsp::SemanticTokens {
            result_id: None,
            data: view.semantic_tokens(&self.coloured_uses(&view)),
        })
    }

    /// The colours of the uses that the editor rules colour by what they mean, by
    /// where each starts.
    fn coloured_uses(&self, view: &View<'_>) -> HashMap<TextSize, u32> {
        let by_binding: Vec<&str> = (self.editor.rules.iter())
            .filter_map(|rule| match rule {
                EditorRule::Highlight { node, by, .. } if by == "by_binding" => Some(node.as_str()),
                _ => None,
            })
            .collect();
        let mut coloured = HashMap::new();
        if by_binding.is_empty() || !self.binds() {
            return coloured;
        }
        // Where the names of the listed nodes start.
        let listed: HashSet<TextSize> = view
            .parse
            .syntax()
            .descendants()
            .filter(|node| by_binding.contains(&self.language.kind_name(node.kind())))
            .map(|node| view.trimmed(&SyntaxElement::Node(node)).start())
            .collect();
        let id = view.file.id();
        let mut kinds = HashMap::new();
        for used in self.bindings.references() {
            if used.file != id || !listed.contains(&used.range.start()) {
                continue;
            }
            let meant = used
                .target
                .and_then(|target| self.bindings.definitions().get(target));
            if let Some(kind) = meant.and_then(|d| self.kind_of(&mut kinds, d)) {
                coloured.insert(used.range.start(), token_type(kind));
            }
        }
        coloured
    }
}
