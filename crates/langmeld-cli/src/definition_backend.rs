use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    error::Error,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use langmeld_core::{SourceFile, SourceLocation};
use langmeld_language::{
    DefinitionResolver, DefinitionSource, DefinitionSourceParts, ParseResult,
    language_definition_grammar, parse, parse_language_definition_project,
};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, BackendSource, BackendWorkspaceCapabilities,
    DocumentSymbolInfo, LanguageBackend, LanguageDefinitionBackend,
};
use langmeld_project::{
    InMemoryDefinitionResolver, OverlayDefinitionResolver, ProjectLoadError, ProjectManifest,
};
use url::Url;

use crate::definition_completion::complete_definition_field;
use crate::definition_index::DefinitionIndex;
use crate::project::DefinitionProjectContext;

pub(crate) struct ProjectDefinitionBackend {
    root: PathBuf,
    root_definition: String,
    resolver: OverlayDefinitionResolver,
    syntax_backend: LanguageDefinitionBackend,
    definition_index: RwLock<Option<Arc<DefinitionIndex>>>,
}

impl ProjectDefinitionBackend {
    pub(crate) fn new(manifest_path: impl Into<PathBuf>) -> Result<Self, Box<dyn Error>> {
        let project = DefinitionProjectContext::new(manifest_path.into())?;
        let root_definition = canonical_definition_key(&project.root, &project.manifest);
        let resolver =
            OverlayDefinitionResolver::new(&project.root, InMemoryDefinitionResolver::new())?;
        Ok(Self {
            root: project.root,
            root_definition,
            resolver,
            syntax_backend: LanguageDefinitionBackend,
            definition_index: RwLock::new(None),
        })
    }

    fn current_definition_index(&self) -> Option<Arc<DefinitionIndex>> {
        self.definition_index
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn clear_definition_index(&self) {
        *self
            .definition_index
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

impl LanguageBackend for ProjectDefinitionBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        self.analyze_workspace(std::slice::from_ref(source))
            .and_then(|mut documents| documents.pop())
            .unwrap_or_else(|| self.syntax_backend.analyze(source))
    }

    fn analyze_workspace(
        &self,
        sources: &[SourceFile],
    ) -> Option<Vec<BackendDocument<Self::Document>>> {
        let keys = sources
            .iter()
            .map(|source| self.source_key(source))
            .collect::<Vec<_>>();
        let open_sources = sources
            .iter()
            .zip(&keys)
            .filter_map(|(source, key)| key.as_ref().map(|key| (key.clone(), source.clone())))
            .collect::<BTreeMap<_, _>>();

        let mut overlay = InMemoryDefinitionResolver::new();
        for (key, source) in &open_sources {
            if overlay.insert(key, source.clone()).is_err() {
                self.clear_definition_index();
                return None;
            }
        }
        let resolver = self.resolver.with_overlay(overlay);
        let Ok(root) = resolver.load(&self.root_definition) else {
            self.clear_definition_index();
            return None;
        };
        let root_source = root.source().clone();
        let resolver = OpenFileOverlayResolver {
            resolver: &resolver,
            loaded: RefCell::new(HashSet::from([self.root_definition.clone()])),
            sources: RefCell::new(vec![(self.root_definition.clone(), root_source)]),
        };
        let result = parse_language_definition_project(root, &resolver);
        let mut definition_index = DefinitionIndex::default();
        let mut parsed_sources = BTreeMap::new();
        for (key, source) in resolver.sources.into_inner() {
            let parsed = parse(&source, language_definition_grammar());
            definition_index.add_parsed_source(&source, &parsed);

            if open_sources.contains_key(&key) {
                parsed_sources.insert(key, parsed);
            }
        }
        let definition_index = Arc::new(definition_index);
        *self
            .definition_index
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(definition_index);
        let loaded = resolver.loaded.into_inner();

        Some(
            sources
                .iter()
                .zip(keys)
                .map(|(source, key)| {
                    if let Some(key) = key.as_ref().filter(|key| loaded.contains(*key)) {
                        let syntax_state = parsed_sources.remove(key).unwrap_or_else(|| {
                            self.syntax_backend.analyze(source).into_parts().state
                        });
                        let diagnostics = result
                            .diagnostics
                            .iter()
                            .filter(|diagnostic| match diagnostic.location.as_ref() {
                                Some(location) => location.source_name() == source.name(),
                                None => key == &self.root_definition,
                            })
                            .cloned()
                            .collect();
                        BackendDocument::new(source, syntax_state, diagnostics)
                    } else {
                        self.syntax_backend.analyze(source)
                    }
                })
                .collect(),
        )
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            definition: true,
            references: true,
            ..self.syntax_backend.capabilities()
        }
    }

    fn workspace_capabilities(&self) -> BackendWorkspaceCapabilities {
        let Ok(root_url) = Url::from_directory_path(&self.root) else {
            return BackendWorkspaceCapabilities::default();
        };
        let Ok(root_uri): Result<lsp_types::Uri, _> = root_url.as_str().parse() else {
            return BackendWorkspaceCapabilities::default();
        };
        let watched_files = ["**/*.langmeld", "**/*.lm"]
            .into_iter()
            .map(|pattern| lsp_types::FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::Relative(lsp_types::RelativePattern {
                    base_uri: lsp_types::OneOf::Right(root_uri.clone()),
                    pattern: pattern.into(),
                }),
                kind: Some(
                    lsp_types::WatchKind::Create
                        | lsp_types::WatchKind::Change
                        | lsp_types::WatchKind::Delete,
                ),
            })
            .collect();

        BackendWorkspaceCapabilities {
            watched_files,
            ..BackendWorkspaceCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<langmeld_language::CompletionCandidate> {
        let definition_index = self.current_definition_index();
        let source_key = self.source_key(source);
        if let Some(completions) = complete_definition_field(
            &self.root,
            source,
            document.state(),
            offset,
            definition_index.as_deref(),
            source_key.as_deref(),
        ) {
            return completions;
        }
        self.syntax_backend.complete(source, document, offset)
    }

    fn definition(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<SourceLocation> {
        if self.source_key(source).is_none() {
            return Vec::new();
        }
        self.current_definition_index()
            .as_deref()
            .map_or_else(Vec::new, |index| {
                index.definitions_at(source, document.state(), offset)
            })
    }

    fn references(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation> {
        if self.source_key(source).is_none() {
            return Vec::new();
        }
        self.current_definition_index()
            .as_deref()
            .map_or_else(Vec::new, |index| {
                index.references_at(source, document.state(), offset, include_declaration)
            })
    }

    fn source_for_location(&self, location: &SourceLocation) -> Option<BackendSource> {
        let resolved = self.resolver.load(location.source_name()).ok()?;
        if !location.is_valid_for(resolved.source()) {
            return None;
        }
        let path = self.root.join(location.source_name()).canonicalize().ok()?;
        if !path.starts_with(&self.root) {
            return None;
        }
        let uri = Url::from_file_path(path).ok()?.as_str().parse().ok()?;
        let DefinitionSourceParts { source, .. } = resolved.into_parts();
        Some(BackendSource::new(uri, source))
    }

    fn format_document(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        options: &lsp_types::FormattingOptions,
    ) -> Option<String> {
        self.syntax_backend
            .format_document(source, document, options)
    }

    fn document_symbols(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<DocumentSymbolInfo> {
        self.syntax_backend.document_symbols(source, document)
    }
}

impl ProjectDefinitionBackend {
    fn source_key(&self, source: &SourceFile) -> Option<String> {
        let path = Url::parse(source.name()).ok()?.to_file_path().ok()?;
        self.resolver.key_for_path(path).ok()
    }
}

fn canonical_definition_key(root: &std::path::Path, manifest: &ProjectManifest) -> String {
    let path = root.join(&manifest.definition);
    let canonical = path.canonicalize().unwrap_or(path);
    canonical
        .strip_prefix(root)
        .ok()
        .and_then(std::path::Path::to_str)
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .unwrap_or_else(|| manifest.definition.clone())
}

struct OpenFileOverlayResolver<'a> {
    resolver: &'a OverlayDefinitionResolver,
    loaded: RefCell<HashSet<String>>,
    sources: RefCell<Vec<(String, SourceFile)>>,
}

impl DefinitionResolver for OpenFileOverlayResolver<'_> {
    type Error = ProjectLoadError;

    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, Self::Error> {
        let resolved = self.resolver.resolve(from_key, import)?;
        let DefinitionSourceParts {
            key: resolved_key,
            source,
        } = resolved.into_parts();
        let is_new_source = self.loaded.borrow_mut().insert(resolved_key.clone());
        let mut sources = self.sources.borrow_mut();
        if is_new_source {
            sources.push((resolved_key.clone(), source.clone()));
        } else if let Some((_, loaded_source)) =
            sources.iter_mut().find(|(key, _)| key == &resolved_key)
        {
            *loaded_source = source.clone();
        }
        Ok(DefinitionSource::new(resolved_key, source))
    }
}
