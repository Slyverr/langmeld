use std::{
    collections::{HashMap, HashSet},
    error::Error,
    path::PathBuf,
    sync::{Mutex, RwLock},
};

use langmeld_core::{SourceFile, SourceLocation};
use langmeld_language::{
    DefinitionResolver, DefinitionSource, ParseResult, parse_language_definition_project,
};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, BackendSource, BackendWorkspaceCapabilities,
    DocumentSymbolInfo, LanguageBackend, LanguageDefinitionBackend,
};
use langmeld_project::{DirectoryDefinitionResolver, ProjectManifest};
use url::Url;

use crate::definition_completion::complete_definition_field;
use crate::definition_index::DefinitionIndex;
use crate::project::DefinitionProjectContext;

pub(crate) struct ProjectDefinitionBackend {
    root: PathBuf,
    root_definition: String,
    resolver: DirectoryDefinitionResolver,
    syntax_backend: LanguageDefinitionBackend,
    definition_index: RwLock<Option<DefinitionIndex>>,
}

impl ProjectDefinitionBackend {
    pub(crate) fn new(manifest_path: impl Into<PathBuf>) -> Result<Self, Box<dyn Error>> {
        let project = DefinitionProjectContext::new(manifest_path.into())?;
        let root_definition = canonical_definition_key(&project.root, &project.manifest);
        Ok(Self {
            root: project.root,
            root_definition,
            resolver: project.resolver,
            syntax_backend: LanguageDefinitionBackend,
            definition_index: RwLock::new(None),
        })
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
            .collect::<HashMap<_, _>>();

        let root = if let Some(source) = open_sources.get(&self.root_definition) {
            DefinitionSource::new(&self.root_definition, source.clone())
        } else {
            self.resolver.load(&self.root_definition).ok()?
        };
        let root_source = root.source().clone();
        let resolver = OpenFileOverlayResolver {
            resolver: &self.resolver,
            open_sources: &open_sources,
            loaded: Mutex::new(HashSet::from([self.root_definition.clone()])),
            sources: Mutex::new(vec![(self.root_definition.clone(), root_source)]),
        };
        let result = parse_language_definition_project(root, &resolver);
        let loaded_sources = resolver
            .sources
            .lock()
            .expect("definition source lock is not poisoned")
            .iter()
            .map(|(_, source)| source.clone())
            .collect::<Vec<_>>();
        *self
            .definition_index
            .write()
            .expect("definition reference lock is not poisoned") =
            Some(DefinitionIndex::from_sources(&loaded_sources));
        let loaded = resolver
            .loaded
            .lock()
            .expect("definition resolver lock is not poisoned");

        Some(
            sources
                .iter()
                .zip(keys)
                .map(|(source, key)| {
                    let syntax = self.syntax_backend.analyze(source);
                    let (_, syntax_state, syntax_diagnostics) = syntax.into_parts();
                    let diagnostics = if key.as_ref().is_some_and(|key| loaded.contains(key)) {
                        result
                            .diagnostics
                            .iter()
                            .filter(|diagnostic| match diagnostic.location.as_ref() {
                                Some(location) => location.source_name() == source.name(),
                                None => key.as_deref() == Some(&self.root_definition),
                            })
                            .cloned()
                            .collect()
                    } else {
                        syntax_diagnostics
                    };

                    BackendDocument::new(source, syntax_state, diagnostics)
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
        let Ok(root_uri) = root_url.as_str().parse() else {
            return BackendWorkspaceCapabilities::default();
        };
        BackendWorkspaceCapabilities {
            watched_files: vec![lsp_types::FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::Relative(lsp_types::RelativePattern {
                    base_uri: lsp_types::OneOf::Right(root_uri),
                    pattern: "**/*.langmeld".into(),
                }),
                kind: Some(
                    lsp_types::WatchKind::Create
                        | lsp_types::WatchKind::Change
                        | lsp_types::WatchKind::Delete,
                ),
            }],
            ..BackendWorkspaceCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<langmeld_language::CompletionCandidate> {
        let definition_index = self
            .definition_index
            .read()
            .expect("definition reference lock is not poisoned")
            .clone();
        if let Some(completions) = complete_definition_field(
            &self.root,
            source,
            document.state(),
            offset,
            definition_index.as_ref(),
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
        self.definition_index
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
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
        self.definition_index
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or_else(Vec::new, |index| {
                index.references_at(source, document.state(), offset, include_declaration)
            })
    }

    fn source_for_location(&self, location: &SourceLocation) -> Option<BackendSource> {
        let resolved = self.resolver.load(location.source_name()).ok()?;
        if resolved.source().identity() != location.source_identity()
            || resolved.source().name() != location.source_name()
        {
            return None;
        }
        let path = self.root.join(location.source_name()).canonicalize().ok()?;
        if !path.starts_with(&self.root) {
            return None;
        }
        let uri = Url::from_file_path(path).ok()?.as_str().parse().ok()?;
        let (_, source) = resolved.into_parts();
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
        let canonical = path.canonicalize().ok()?;
        let relative = canonical.strip_prefix(&self.root).ok()?;
        relative
            .to_str()
            .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
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
    resolver: &'a DirectoryDefinitionResolver,
    open_sources: &'a HashMap<String, SourceFile>,
    loaded: Mutex<HashSet<String>>,
    sources: Mutex<Vec<(String, SourceFile)>>,
}

impl DefinitionResolver for OpenFileOverlayResolver<'_> {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let resolved = self.resolver.resolve(from_key, import)?;
        let (resolved_key, resolved_source) = resolved.into_parts();
        let is_new_source = self
            .loaded
            .lock()
            .expect("definition resolver lock is not poisoned")
            .insert(resolved_key.clone());
        let source = if let Some(source) = self.open_sources.get(&resolved_key) {
            source.clone()
        } else {
            resolved_source
        };
        let mut sources = self
            .sources
            .lock()
            .expect("definition source lock is not poisoned");
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
