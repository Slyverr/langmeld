//! Deterministic integrity locks for the definitions resolved by a project.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use langmeld_core::ValidatedLanguageSpec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DefinitionLimits, definition::language_definition_source_paths};

use super::{
    ExtensionDependency, ExtensionPackage, ManifestExtension, ProjectManifest,
    extensions::extension_order, load_language, read_bounded, write_atomically,
};

const LOCK_SCHEMA_VERSION: u32 = 1;
const LOCKFILE_NAME: &str = "langmeld.lock";
const MAX_LOCK_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEFINITION_BYTES: usize = 64 * 1024 * 1024;

/// One project lockfile recording resolved language inputs.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectLockfile {
    /// Version of this lockfile format.
    pub schema_version: u32,
    /// SHA-256 of the canonical language-resolution manifest projection.
    pub resolution_sha256: String,
    /// Ordered base, extension, and imported definition paths and byte hashes.
    pub definitions: Vec<LockedDefinition>,
    /// Ordered package identities and constraints selected by the project.
    pub extensions: Vec<LockedExtension>,
    /// SHA-256 of the canonical effective language JSON.
    pub effective_language_sha256: String,
}

/// One local language definition pinned by a project lockfile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockedDefinition {
    /// Definition path relative to the project manifest.
    pub path: PathBuf,
    /// SHA-256 of the exact file bytes.
    pub sha256: String,
}

/// One selected local extension package recorded in the lockfile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockedExtension {
    /// Stable package identifier.
    pub id: String,
    /// Exact selected semantic version.
    pub version: String,
    /// Language version requirement used for compatibility.
    pub language_version: String,
    /// Definition path relative to the project manifest.
    pub path: PathBuf,
    /// Package dependencies and their accepted ranges.
    pub dependencies: Vec<LockedExtensionDependency>,
}

/// One local package dependency and its accepted semantic-version range.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockedExtensionDependency {
    /// Required package identifier.
    pub id: String,
    /// Accepted semantic-version range.
    pub version: String,
}

#[derive(Serialize)]
struct ResolutionInputs<'a> {
    schema_version: u32,
    language: &'a Path,
    extensions: &'a [ManifestExtension],
}

#[derive(Deserialize)]
struct LockfileVersion {
    schema_version: u32,
}

/// Writes or refreshes the deterministic lock for the project manifest.
///
/// A missing lock is allowed during project loading. Refreshing one validates
/// all definition inputs and writes the replacement atomically.
///
/// # Errors
/// Returns an error for invalid project definitions, oversized inputs, or
/// lockfile read/write failures.
pub fn write_project_lockfile(manifest_path: &Path) -> Result<PathBuf, String> {
    let manifest = super::load_project_manifest(manifest_path)?;
    let Some(language_path) = manifest.declarative_language_path() else {
        return Err("Rust-host projects do not have declarative language locks".to_owned());
    };
    let lock_path = lockfile_path(manifest_path);
    ensure_lockfile_destination(&lock_path)?;
    let project_root = project_root(manifest_path)?;
    let base_path = project_root.join(language_path);
    let base_language = load_language(&base_path)?;
    let language = super::extensions::resolve_project_language(
        &project_root,
        &manifest.extensions,
        &base_language,
    )?;
    let lockfile = expected_lockfile(&project_root, &manifest, &language)?;
    let contents = serialize_lockfile(&lockfile)?;
    write_atomically(&lock_path, &contents, "project lockfile")?;
    Ok(lock_path)
}

/// Replaces the extension declarations and refreshes their lock as one
/// recoverable project update.
///
/// The candidate language is fully resolved before either file is changed.
/// If replacing the lock fails after the manifest write, the original
/// manifest bytes are restored.
pub fn write_project_extensions(
    manifest_path: &Path,
    extensions: Vec<ManifestExtension>,
) -> Result<(), String> {
    let mut manifest = super::load_project_manifest(manifest_path)?;
    let manifest_bytes =
        read_bounded(manifest_path, super::MAX_MANIFEST_BYTES, "project manifest")?;
    manifest.extensions = extensions;
    let manifest_contents = manifest.to_json()?;
    let project = super::load_project_with_manifest(manifest_path, &manifest)?;
    let lock_path = lockfile_path(manifest_path);
    ensure_lockfile_destination(&lock_path)?;
    let root = project_root(manifest_path)?;
    let lockfile = expected_lockfile(&root, &manifest, &project.language)?;
    let lock_contents = serialize_lockfile(&lockfile)?;

    write_atomically(
        manifest_path,
        manifest_contents.as_bytes(),
        "project manifest",
    )?;
    if let Err(lock_error) = write_atomically(&lock_path, &lock_contents, "project lockfile") {
        return match write_atomically(manifest_path, &manifest_bytes, "project manifest rollback") {
            Ok(()) => Err(lock_error),
            Err(rollback_error) => Err(format!("{lock_error}; additionally, {rollback_error}")),
        };
    }
    Ok(())
}

fn serialize_lockfile(lockfile: &ProjectLockfile) -> Result<Vec<u8>, String> {
    let mut contents = serde_json::to_vec_pretty(lockfile)
        .map_err(|error| format!("cannot serialize project lockfile: {error}"))?;
    contents.push(b'\n');
    if contents.len() > MAX_LOCK_BYTES {
        return Err(format!(
            "project lockfile exceeds the {MAX_LOCK_BYTES}-byte limit"
        ));
    }
    Ok(contents)
}

pub(super) fn verify_project_lockfile(
    manifest_path: &Path,
    project_root: &Path,
    manifest: &ProjectManifest,
    language: &ValidatedLanguageSpec,
) -> Result<(), String> {
    let path = lockfile_path(manifest_path);
    let bytes = match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(format!(
                "project lockfile '{}' is not a regular file",
                path.display()
            ));
        }
        Ok(_) => read_bounded(&path, MAX_LOCK_BYTES, "project lockfile")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "cannot inspect project lockfile '{}': {error}",
                path.display()
            ));
        }
    };
    let version: LockfileVersion = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "cannot parse project lockfile '{}': {error}",
            path.display()
        )
    })?;
    if version.schema_version != LOCK_SCHEMA_VERSION {
        return Err(format!(
            "unsupported project lockfile schema version {}; expected {}",
            version.schema_version, LOCK_SCHEMA_VERSION
        ));
    }
    let lockfile: ProjectLockfile = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "cannot parse project lockfile '{}': {error}",
            path.display()
        )
    })?;
    let expected = expected_lockfile(project_root, manifest, language)?;
    if lockfile != expected {
        return Err(format!(
            "project lockfile '{}' is stale; run `langmeld lock --project {}`",
            path.display(),
            manifest_path.display()
        ));
    }
    Ok(())
}

fn expected_lockfile(
    project_root: &Path,
    manifest: &ProjectManifest,
    language: &ValidatedLanguageSpec,
) -> Result<ProjectLockfile, String> {
    let Some(language_path) = manifest.declarative_language_path() else {
        return Err("Rust-host projects do not have declarative language locks".to_owned());
    };
    let base_language = load_language(&project_root.join(language_path))?;
    let order = extension_order(&manifest.extensions, &base_language)?;
    let mut roots: Vec<&Path> = Vec::with_capacity(1 + order.len());
    roots.push(language_path);
    roots.extend(order.iter().map(|index| manifest.extensions[*index].path()));

    let mut definitions = Vec::new();
    let mut seen_files = BTreeSet::new();
    let mut total_definition_bytes = 0_usize;
    for relative_root in roots {
        let absolute_root = project_root.join(relative_root);
        let canonical_root = fs::canonicalize(&absolute_root).map_err(|error| {
            format!(
                "cannot resolve language definition '{}': {error}",
                absolute_root.display()
            )
        })?;
        let canonical_directory = canonical_root.parent().ok_or_else(|| {
            format!(
                "language definition '{}' has no parent directory",
                absolute_root.display()
            )
        })?;
        let source_paths =
            language_definition_source_paths(&absolute_root, DefinitionLimits::default())?;
        for source_path in source_paths {
            if !seen_files.insert(source_path.clone()) {
                continue;
            }
            let relative_to_root = source_path.strip_prefix(canonical_directory).map_err(|_| {
                format!(
                    "imported definition '{}' is outside its root directory",
                    source_path.display()
                )
            })?;
            let locked_path = relative_root
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(relative_to_root);
            let bytes = read_bounded(&source_path, 4 * 1024 * 1024, "language definition")?;
            total_definition_bytes = total_definition_bytes.saturating_add(bytes.len());
            if total_definition_bytes > MAX_DEFINITION_BYTES {
                return Err(format!(
                    "project language definitions exceed the {MAX_DEFINITION_BYTES}-byte limit"
                ));
            }
            definitions.push(LockedDefinition {
                path: locked_path,
                sha256: sha256(&bytes),
            });
        }
    }

    let resolution_inputs = ResolutionInputs {
        schema_version: manifest.schema_version,
        language: language_path,
        extensions: &manifest.extensions,
    };
    let resolution_bytes = serde_json::to_vec(&resolution_inputs)
        .map_err(|error| format!("cannot serialize language-resolution inputs: {error}"))?;
    let effective_bytes = serde_json::to_vec(language.spec())
        .map_err(|error| format!("cannot serialize effective language: {error}"))?;
    let extensions = order
        .iter()
        .filter_map(|index| match &manifest.extensions[*index] {
            ManifestExtension::Path(_) => None,
            ManifestExtension::Package(package) => Some(locked_extension(package)),
        })
        .collect();

    Ok(ProjectLockfile {
        schema_version: LOCK_SCHEMA_VERSION,
        resolution_sha256: sha256(&resolution_bytes),
        definitions,
        extensions,
        effective_language_sha256: sha256(&effective_bytes),
    })
}

fn locked_extension(package: &ExtensionPackage) -> LockedExtension {
    LockedExtension {
        id: package.id.clone(),
        version: package.version.clone(),
        language_version: package.language_version.clone(),
        path: package.path.clone(),
        dependencies: package.dependencies.iter().map(locked_dependency).collect(),
    }
}

fn locked_dependency(dependency: &ExtensionDependency) -> LockedExtensionDependency {
    LockedExtensionDependency {
        id: dependency.id.clone(),
        version: dependency.version.clone(),
    }
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

fn lockfile_path(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(LOCKFILE_NAME)
}

pub(super) fn reject_rust_host_lockfile(manifest_path: &Path) -> Result<(), String> {
    let path = lockfile_path(manifest_path);
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(format!(
            "Rust-host projects do not use declarative language lockfiles; remove stale '{}'",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot inspect project lockfile '{}': {error}",
            path.display()
        )),
    }
}

fn ensure_lockfile_destination(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(format!(
            "project lockfile '{}' is not a regular file",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot inspect project lockfile '{}': {error}",
            path.display()
        )),
    }
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hexadecimal = String::with_capacity(digest.len() * 2);
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        hexadecimal.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hexadecimal.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    hexadecimal
}
