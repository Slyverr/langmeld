//! Project configuration and bounded discovery of language source files.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use langmeld_core::{LanguageSpec, SourceFile, ValidatedLanguageSpec};
use serde::{Deserialize, Serialize};

const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 2_048;
const MAX_DIRECTORY_ENTRIES: usize = 100_000;

/// Project-level paths and source discovery settings stored in `langmeld.json`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    /// Version of the project-manifest format.
    pub schema_version: u32,
    /// Main language definition, relative to this manifest.
    pub language: PathBuf,
    /// Explicit program source files, relative to this manifest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<PathBuf>,
    /// Directories recursively scanned for program source files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_roots: Vec<PathBuf>,
    /// Source suffixes used when scanning roots; empty inherits the language definition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_extensions: Vec<String>,
}

/// A validated project view shared by CLI commands and editor integrations.
#[derive(Clone, Debug)]
pub struct ProjectSnapshot {
    /// Manifest settings used to load this snapshot.
    pub manifest: ProjectManifest,
    /// Fully parsed and validated language definition.
    pub language: ValidatedLanguageSpec,
    /// Explicit and discovered source files in stable project order.
    pub sources: Vec<SourceFile>,
}

impl ProjectManifest {
    /// Creates a starter project manifest.
    #[must_use]
    pub fn new(language: impl Into<PathBuf>) -> Self {
        Self {
            schema_version: 1,
            language: language.into(),
            sources: Vec::new(),
            source_roots: Vec::new(),
            source_extensions: Vec::new(),
        }
    }

    /// Serializes this manifest as stable, human-readable JSON with a final newline.
    pub fn to_json(&self) -> Result<String, String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported project manifest schema version {}; expected 1",
                self.schema_version
            ));
        }
        validate_manifest_paths(self)?;
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("cannot serialize project manifest: {error}"))?;
        json.push('\n');
        if json.len() > MAX_MANIFEST_BYTES {
            return Err(format!(
                "project manifest exceeds the {MAX_MANIFEST_BYTES}-byte limit"
            ));
        }
        Ok(json)
    }
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
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported project manifest schema version {}; expected 1",
            manifest.schema_version
        ));
    }
    validate_manifest_paths(&manifest)?;
    Ok(manifest)
}

/// Loads one standalone readable `.lm` or canonical JSON language definition.
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

/// Loads explicit source files using the same bounds and file checks as projects.
pub fn load_source_files(paths: &[PathBuf]) -> Result<Vec<SourceFile>, String> {
    load_project_sources_from_paths(paths.to_vec())
}

/// Loads, resolves, and validates a project from its manifest on disk.
pub fn load_project(manifest_path: &Path) -> Result<ProjectSnapshot, String> {
    let manifest = load_project_manifest(manifest_path)?;
    load_project_with_manifest(manifest_path, &manifest)
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
    let definition_path = root.join(&manifest.language);
    let language = load_language(&definition_path)?;
    let paths = project_source_paths(manifest_path, manifest, &language.spec().source_extensions)?;
    let sources = load_project_sources_from_paths(paths)?;
    Ok(ProjectSnapshot {
        manifest: manifest.clone(),
        language,
        sources,
    })
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
    write_atomically(manifest_path, contents.as_bytes())
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
    validate_relative_path(&manifest.language, "language")?;
    for path in &manifest.sources {
        validate_relative_path(path, "source")?;
    }
    for path in &manifest.source_roots {
        validate_relative_path(path, "source root")?;
    }
    Ok(())
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

fn read_bounded(path: &Path, limit: usize, role: &str) -> Result<Vec<u8>, String> {
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

fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        format!(
            "project manifest path '{}' has no file name",
            path.display()
        )
    })?;
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
                    "cannot create temporary manifest '{}': {error}",
                    temporary_path.display()
                ));
            }
        };
        let write_result = file
            .write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!(
                    "cannot write temporary manifest '{}': {error}",
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
                "cannot replace project manifest '{}': {error}",
                path.display()
            ));
        }
        return Ok(());
    }
    Err(format!(
        "cannot create a unique temporary manifest beside '{}'",
        path.display()
    ))
}
