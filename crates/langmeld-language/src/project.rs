//! Project configuration and bounded discovery of language source files.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use langmeld_core::{LanguageSpec, SourceFile, ValidatedLanguageSpec};

use crate::DefinitionLimits;

mod extensions;
mod lock;
mod manifest;
pub use extensions::{
    ExtensionDependency, ExtensionPackage, ManifestExtension, compose_extensions,
};
pub use lock::{
    LockedDefinition, LockedExtension, LockedExtensionDependency, ProjectLockfile,
    write_project_extensions, write_project_lockfile,
};
pub use manifest::{ProjectBuildConfig, ProjectLanguageSource, ProjectManifest};

const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 2_048;
const MAX_DIRECTORY_ENTRIES: usize = 100_000;
const MAX_PROJECT_EXTENSIONS: usize = 128;

/// A validated project view shared by CLI commands and editor integrations.
#[derive(Clone, Debug)]
pub struct ProjectSnapshot {
    /// Manifest settings used to load this snapshot.
    pub manifest: ProjectManifest,
    /// Effective validated language model after declared extensions are composed.
    pub language: ValidatedLanguageSpec,
    /// Explicit and discovered source files in stable project order.
    pub sources: Vec<SourceFile>,
}

/// Loads a strict, versioned project manifest from disk.
pub fn load_project_manifest(path: &Path) -> Result<ProjectManifest, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect project manifest '{}': {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "project manifest '{}' is not a regular file",
            path.display()
        ));
    }
    let bytes = read_bounded(path, MAX_MANIFEST_BYTES, "project manifest")?;
    let manifest: ProjectManifest = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "cannot parse project manifest '{}': {error}",
            path.display()
        )
    })?;
    if !matches!(manifest.schema_version, 1 | 2) {
        return Err(format!(
            "unsupported project manifest schema version {}; expected 1 or 2",
            manifest.schema_version
        ));
    }
    validate_manifest_paths(&manifest)?;
    Ok(manifest)
}

/// Loads one standalone readable `.langmeld` or canonical JSON language definition.
pub fn load_language(path: &Path) -> Result<ValidatedLanguageSpec, String> {
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let bytes = read_bounded(path, MAX_MANIFEST_BYTES, "language definition")?;
        let spec: LanguageSpec = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "cannot parse language definition '{}': {error}",
                path.display()
            )
        })?;
        return spec.validate().map_err(|errors| {
            errors
                .into_iter()
                .map(|error| format!("{}: {}", error.code, error.message))
                .collect::<Vec<_>>()
                .join("\n")
        });
    }
    crate::parse_language_definition_project(path).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Loads and validates a project's effective language without discovering program sources.
///
/// This is useful to build scripts that compile language models without requiring
/// source roots such as `examples/` to exist during the build.
pub fn load_project_language(manifest_path: &Path) -> Result<ValidatedLanguageSpec, String> {
    let manifest = load_project_manifest(manifest_path)?;
    let Some(language_path) = manifest.declarative_language_path() else {
        return Err(rust_host_model_required());
    };
    let root = project_root(manifest_path)?;
    let base_language = load_language(&root.join(language_path))?;
    let language =
        extensions::resolve_project_language(&root, &manifest.extensions, &base_language)?;
    lock::verify_project_lockfile(manifest_path, &root, &manifest, &language)?;
    Ok(language)
}

/// Composes a Rust-host base model with its declared `.langmeld` extensions without loading sources.
pub fn load_project_language_with_base_language(
    manifest_path: &Path,
    base_language: &ValidatedLanguageSpec,
) -> Result<ValidatedLanguageSpec, String> {
    let manifest = load_project_manifest(manifest_path)?;
    load_project_language_with_manifest_and_base_language(manifest_path, &manifest, base_language)
}

/// Composes a proposed Rust-host manifest with its supplied base model without loading sources.
pub fn load_project_language_with_manifest_and_base_language(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    base_language: &ValidatedLanguageSpec,
) -> Result<ValidatedLanguageSpec, String> {
    manifest.to_json()?;
    if !matches!(
        &manifest.language_source,
        ProjectLanguageSource::RustHost {}
    ) {
        return Err(
            "cannot inject a Rust-host language model for a declarative project manifest"
                .to_owned(),
        );
    }
    lock::reject_rust_host_lockfile(manifest_path)?;
    let root = project_root(manifest_path)?;
    extensions::resolve_project_language(&root, &manifest.extensions, base_language)
}

/// Loads explicit source files using the same bounds and file checks as projects.
pub fn load_source_files(paths: &[PathBuf]) -> Result<Vec<SourceFile>, String> {
    load_project_sources_from_paths(paths.to_vec())
}

/// Loads, resolves, and validates a project from its manifest on disk.
pub fn load_project(manifest_path: &Path) -> Result<ProjectSnapshot, String> {
    let manifest = load_project_manifest(manifest_path)?;
    if matches!(
        &manifest.language_source,
        ProjectLanguageSource::RustHost {}
    ) {
        return Err(rust_host_model_required());
    }
    let project = load_project_with_manifest(manifest_path, &manifest)?;
    let root = project_root(manifest_path)?;
    lock::verify_project_lockfile(manifest_path, &root, &manifest, &project.language)?;
    Ok(project)
}

/// Loads a proposed project configuration without requiring it to be written first.
///
/// This is used by interactive configuration: the candidate definition and
/// source selection are fully validated before the manifest is replaced.
pub fn load_project_with_manifest(
    manifest_path: &Path,
    manifest: &ProjectManifest,
) -> Result<ProjectSnapshot, String> {
    manifest.to_json()?;
    let root = project_root(manifest_path)?;
    let Some(definition_path) = manifest.declarative_language_path() else {
        return Err(rust_host_model_required());
    };
    let definition_path = root.join(definition_path);
    let base_language = load_language(&definition_path)?;
    load_project_snapshot(manifest_path, manifest, &base_language)
}

/// Builds a project snapshot using the base language model registered by its Rust host.
///
/// The supplied model must be the base model before manifest extensions are
/// composed. Existing declarative definition locks do not apply to Rust-host
/// models; an existing lock file is rejected instead of being silently trusted.
/// The manifest is loaded using the normal bounded, symlink-aware loader.
///
/// # Errors
/// Returns an error if the manifest is not in Rust-host mode, extensions are
/// invalid, source discovery fails, or a declarative lock file is present.
pub fn load_project_with_base_language(
    manifest_path: &Path,
    base_language: &ValidatedLanguageSpec,
) -> Result<ProjectSnapshot, String> {
    let manifest = load_project_manifest(manifest_path)?;
    load_project_with_manifest_and_base_language(manifest_path, &manifest, base_language)
}

/// Validates a proposed Rust-host project manifest using its registered base model.
///
/// The supplied model must be the base model before manifest extensions are
/// composed. Use [`load_project_with_base_language`] to load the on-disk
/// manifest through the bounded file loader.
///
/// # Errors
/// Returns an error if the manifest is not in Rust-host mode, extensions are
/// invalid, source discovery fails, or a declarative lock file is present.
pub fn load_project_with_manifest_and_base_language(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    base_language: &ValidatedLanguageSpec,
) -> Result<ProjectSnapshot, String> {
    manifest.to_json()?;
    if !matches!(
        &manifest.language_source,
        ProjectLanguageSource::RustHost {}
    ) {
        return Err(
            "cannot inject a Rust-host language model for a declarative project manifest"
                .to_owned(),
        );
    }
    lock::reject_rust_host_lockfile(manifest_path)?;
    load_project_snapshot(manifest_path, manifest, base_language)
}

/// Loads project sources using an effective model compiled by the project host.
///
/// The model must already include every manifest extension. Declarative
/// manifests verify their integrity lock against current definition inputs;
/// Rust-host manifests reject declarative lockfiles.
pub fn load_project_with_compiled_language(
    manifest_path: &Path,
    effective_language: &ValidatedLanguageSpec,
) -> Result<ProjectSnapshot, String> {
    let manifest = load_project_manifest(manifest_path)?;
    let root = project_root(manifest_path)?;
    if matches!(manifest.language_source, ProjectLanguageSource::RustHost {}) {
        lock::reject_rust_host_lockfile(manifest_path)?;
    } else {
        lock::verify_project_lockfile(manifest_path, &root, &manifest, effective_language)?;
    }
    load_project_snapshot_with_language(manifest_path, &manifest, effective_language)
}

fn load_project_snapshot(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    base_language: &ValidatedLanguageSpec,
) -> Result<ProjectSnapshot, String> {
    let root = project_root(manifest_path)?;
    let language =
        extensions::resolve_project_language(&root, &manifest.extensions, base_language)?;
    load_project_snapshot_with_language(manifest_path, manifest, &language)
}

fn load_project_snapshot_with_language(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    language: &ValidatedLanguageSpec,
) -> Result<ProjectSnapshot, String> {
    let paths = project_source_paths(manifest_path, manifest, &language.spec().source_extensions)?;
    let sources = load_project_sources_from_paths(paths)?;
    Ok(ProjectSnapshot {
        manifest: manifest.clone(),
        language: language.clone(),
        sources,
    })
}

fn project_definition_paths(
    root: &Path,
    manifest: &ProjectManifest,
    base_language: &ValidatedLanguageSpec,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    let mut roots = manifest
        .declarative_language_path()
        .map(|path| vec![root.join(path)])
        .unwrap_or_default();
    roots.extend(
        extensions::extension_order(&manifest.extensions, base_language)?
            .into_iter()
            .map(|index| root.join(manifest.extensions[index].path())),
    );
    for definition_root in roots {
        for path in crate::definition::language_definition_source_paths(
            &definition_root,
            DefinitionLimits::default(),
        )? {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

/// Returns the definition and import files used by a manifest in resolution order.
///
/// For Rust-host manifests, `base_language` must be the validated host model and
/// only extension paths are returned. For declarative manifests, the manifest's
/// base definition is returned before its extensions.
pub fn project_definition_source_paths_for_manifest(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    base_language: &ValidatedLanguageSpec,
) -> Result<Vec<PathBuf>, String> {
    manifest.to_json()?;
    let root = project_root(manifest_path)?;
    project_definition_paths(&root, manifest, base_language)
}

/// Returns canonical project definition files in base and extension resolution order.
///
/// This function is separate from project loading so ordinary loads do not
/// parse imported definitions a second time merely to expose provenance.
///
/// # Errors
/// Returns file access, import, ordering, or definition parsing errors.
pub fn project_definition_source_paths(
    manifest_path: &Path,
    project: &ProjectSnapshot,
) -> Result<Vec<PathBuf>, String> {
    let root = project_root(manifest_path)?;
    let Some(language_path) = project.manifest.declarative_language_path() else {
        return Err(
            "Rust-host projects do not have declarative definition source paths".to_owned(),
        );
    };
    let base_language = load_language(&root.join(language_path))?;
    project_definition_paths(&root, &project.manifest, &base_language)
}

/// Loads explicit and discovered source files from a proposed manifest.
///
/// Source suffixes are taken from the effective language definition unless
/// overridden in the project manifest. Explicit sources retain manifest order;
/// discovered paths follow in lexical order and duplicates are removed.
pub fn load_project_sources(
    manifest_path: &Path,
    manifest: &ProjectManifest,
) -> Result<Vec<SourceFile>, String> {
    load_project_with_manifest(manifest_path, manifest).map(|project| project.sources)
}

/// Loads project sources using a Rust host's base language model.
///
/// The base model is extended by the manifest before its source extensions are
/// used for discovery.
pub fn load_project_sources_with_base_language(
    manifest_path: &Path,
    base_language: &ValidatedLanguageSpec,
) -> Result<Vec<SourceFile>, String> {
    load_project_with_base_language(manifest_path, base_language).map(|project| project.sources)
}

/// Atomically writes a validated project manifest next to the project's files.
pub fn write_project_manifest(
    manifest_path: &Path,
    manifest: &ProjectManifest,
) -> Result<(), String> {
    let contents = manifest.to_json()?;
    match fs::symlink_metadata(manifest_path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(format!(
                "project manifest '{}' is not a regular file",
                manifest_path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "cannot inspect project manifest '{}': {error}",
                manifest_path.display()
            ));
        }
    }
    write_atomically(manifest_path, contents.as_bytes(), "project manifest")
}

fn project_source_paths(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    language_extensions: &[String],
) -> Result<Vec<PathBuf>, String> {
    if manifest.sources.is_empty() && manifest.source_roots.is_empty() {
        return Err("project manifest must list `sources` or configure `source_roots`".to_owned());
    }
    if manifest.sources.len() > MAX_SOURCE_FILES {
        return Err(format!("project source count exceeds {MAX_SOURCE_FILES}"));
    }
    if manifest.source_roots.is_empty() && !manifest.source_extensions.is_empty() {
        return Err("`source_extensions` requires at least one `source_root`".to_owned());
    }

    let configured_extensions = if manifest.source_extensions.is_empty() {
        language_extensions
    } else {
        &manifest.source_extensions
    };
    let extensions = normalize_extensions(configured_extensions)?;
    if !manifest.source_roots.is_empty() && extensions.is_empty() {
        return Err(
            "source roots require source extensions from the manifest or language".to_owned(),
        );
    }

    let root = project_root(manifest_path)?;
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    for source in &manifest.sources {
        let path = root.join(source);
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }
    for source_root in &manifest.source_roots {
        let path = root.join(source_root);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect source root '{}': {error}", path.display()))?;
        if !metadata.file_type().is_dir() {
            return Err(format!(
                "source root '{}' is not a directory",
                path.display()
            ));
        }
        let discovered = discover_sources(&path, &extensions, MAX_SOURCE_FILES - paths.len())?;
        for source in discovered {
            if seen.insert(source.clone()) {
                paths.push(source);
                if paths.len() > MAX_SOURCE_FILES {
                    return Err(format!("project source count exceeds {MAX_SOURCE_FILES}"));
                }
            }
        }
    }
    if paths.is_empty() {
        return Err("project manifest resolved no source files".to_owned());
    }

    Ok(paths)
}

fn load_project_sources_from_paths(paths: Vec<PathBuf>) -> Result<Vec<SourceFile>, String> {
    if paths.len() > MAX_SOURCE_FILES {
        return Err(format!("project source count exceeds {MAX_SOURCE_FILES}"));
    }
    let mut unique_paths = Vec::with_capacity(paths.len());
    let mut seen = BTreeSet::new();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect source file '{}': {error}", path.display()))?;
        if !metadata.file_type().is_file() {
            return Err(format!("source '{}' is not a regular file", path.display()));
        }
        let path = fs::canonicalize(&path)
            .map_err(|error| format!("cannot resolve source file '{}': {error}", path.display()))?;
        if seen.insert(path.clone()) {
            unique_paths.push(path);
        }
    }
    let mut total_bytes = 0_usize;
    let mut sources = Vec::with_capacity(unique_paths.len());
    for path in unique_paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect source file '{}': {error}", path.display()))?;
        if !metadata.file_type().is_file() {
            return Err(format!("source '{}' is not a regular file", path.display()));
        }
        let bytes = read_bounded(&path, MAX_SOURCE_BYTES, "source file")?;
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_TOTAL_SOURCE_BYTES {
            return Err(format!(
                "project source contents exceed the {MAX_TOTAL_SOURCE_BYTES}-byte limit"
            ));
        }
        let text = String::from_utf8(bytes)
            .map_err(|error| format!("source '{}' is not UTF-8: {error}", path.display()))?;
        sources.push(SourceFile::new(path.display().to_string(), text));
    }
    Ok(sources)
}

fn validate_manifest_paths(manifest: &ProjectManifest) -> Result<(), String> {
    match (&manifest.schema_version, &manifest.language_source) {
        (1, ProjectLanguageSource::Declarative { path }) => {
            validate_relative_path(path, "language")?;
        }
        (2, ProjectLanguageSource::Declarative { path }) => {
            validate_relative_path(path, "language")?;
        }
        (2, ProjectLanguageSource::RustHost {}) => {}
        (1, ProjectLanguageSource::RustHost {}) => {
            return Err(
                "Rust-host language sources require project manifest schema version 2".to_owned(),
            );
        }
        (version, _) => {
            return Err(format!(
                "unsupported project manifest schema version {version}; expected 1 or 2"
            ));
        }
    }
    for path in &manifest.sources {
        validate_relative_path(path, "source")?;
    }
    for path in &manifest.source_roots {
        validate_relative_path(path, "source root")?;
    }
    if manifest.extensions.len() > MAX_PROJECT_EXTENSIONS {
        return Err(format!(
            "project extension count exceeds {MAX_PROJECT_EXTENSIONS}"
        ));
    }
    for extension in &manifest.extensions {
        validate_relative_path(extension.path(), "extension")?;
        if let ManifestExtension::Package(package) = extension {
            if package.id.trim().is_empty() {
                return Err("extension package ID must not be empty".to_owned());
            }
            if package.version.trim().is_empty() {
                return Err(format!(
                    "extension package `{}` version must not be empty",
                    package.id
                ));
            }
            if package.language_version.trim().is_empty() {
                return Err(format!(
                    "extension package `{}` language version requirement must not be empty",
                    package.id
                ));
            }
        }
    }
    if let Some(build) = &manifest.build {
        if build.backend.trim().is_empty() {
            return Err("project build backend ID must not be empty".to_owned());
        }
        if build
            .format
            .as_deref()
            .is_some_and(|format| format.trim().is_empty())
        {
            return Err("project build format must not be empty".to_owned());
        }
        if build
            .target
            .as_deref()
            .is_some_and(|target| target.trim().is_empty())
        {
            return Err("project build target must not be empty".to_owned());
        }
        if build.settings.keys().any(|key| key.trim().is_empty()) {
            return Err("project build setting keys must not be empty".to_owned());
        }
    }
    Ok(())
}

fn rust_host_model_required() -> String {
    "project manifest uses a Rust-host language source; load it from the host with `load_project_with_base_language` instead of asking the generic loader to execute project code".to_owned()
}

fn project_root(manifest_path: &Path) -> Result<PathBuf, String> {
    let parent = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::canonicalize(parent).map_err(|error| {
        format!(
            "cannot resolve project directory '{}': {error}",
            parent.display()
        )
    })
}

fn validate_relative_path(path: &Path, role: &str) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
    {
        return Err(format!(
            "project {role} path '{}' must be a non-empty relative path",
            path.display()
        ));
    }
    Ok(())
}

fn normalize_extensions(extensions: &[String]) -> Result<BTreeSet<String>, String> {
    let mut normalized = BTreeSet::new();
    for extension in extensions {
        let extension = extension.trim_start_matches('.');
        if extension.is_empty() {
            return Err("source extensions must not be empty".to_owned());
        }
        normalized.insert(extension.to_owned());
    }
    Ok(normalized)
}

fn discover_sources(
    root: &Path,
    extensions: &BTreeSet<String>,
    max_sources: usize,
) -> Result<Vec<PathBuf>, String> {
    let mut directories = vec![root.to_path_buf()];
    let mut sources = Vec::new();
    let mut inspected_entries = 0;
    while let Some(directory) = directories.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            format!(
                "cannot read source directory '{}': {error}",
                directory.display()
            )
        })?;
        let mut entries = entries
            .map(|entry| {
                entry.map_err(|error| {
                    format!(
                        "cannot inspect source directory '{}': {error}",
                        directory.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        inspected_entries += entries.len();
        if inspected_entries > MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "source discovery exceeds {MAX_DIRECTORY_ENTRIES} directory entries"
            ));
        }
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| {
                format!("cannot inspect source path '{}': {error}", path.display())
            })?;
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file() && has_extension(&path, extensions) {
                if sources.len() >= max_sources {
                    return Err(format!("project source count exceeds {MAX_SOURCE_FILES}"));
                }
                sources.push(path);
            }
        }
    }
    sources.sort();
    Ok(sources)
}

fn has_extension(path: &Path, extensions: &BTreeSet<String>) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    extensions.iter().any(|extension| {
        name.strip_suffix(extension)
            .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
    })
}

pub(super) fn read_bounded(path: &Path, limit: usize, role: &str) -> Result<Vec<u8>, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot read {role} '{}': {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {role} '{}': {error}", path.display()))?;
    if bytes.len() > limit {
        return Err(format!(
            "{role} '{}' exceeds the {limit}-byte limit",
            path.display()
        ));
    }
    Ok(bytes)
}

pub(super) fn write_atomically(path: &Path, contents: &[u8], role: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("{role} path '{}' has no file name", path.display()))?;
    for attempt in 0..32_u8 {
        let temporary_path = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            attempt
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot create temporary {role} '{}': {error}",
                    temporary_path.display()
                ));
            }
        };
        let write_result = file
            .write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!(
                    "cannot write temporary {role} '{}': {error}",
                    temporary_path.display()
                )
            });
        drop(file);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temporary_path, path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!(
                "cannot replace {role} '{}': {error}",
                path.display()
            ));
        }
        return Ok(());
    }
    Err(format!(
        "cannot create a unique temporary {role} beside '{}'",
        path.display()
    ))
}
