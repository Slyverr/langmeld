//! Local declarative extension metadata, dependency ordering, and composition.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use langmeld_core::{LanguageSpec, SpecError, ValidatedLanguageSpec};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{DefinitionLimits, definition::parse_language_definition_project_spec_with_limits};

use super::read_bounded;

/// One local extension definition or versioned package entry.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ManifestExtension {
    /// An unversioned definition path relative to the project manifest.
    Path(PathBuf),
    /// A local package with identity, compatibility, and dependency metadata.
    Package(ExtensionPackage),
}

impl ManifestExtension {
    pub(super) fn path(&self) -> &Path {
        match self {
            Self::Path(path) => path,
            Self::Package(package) => &package.path,
        }
    }
}

/// Metadata for one local declarative extension package.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionPackage {
    /// Stable lowercase dot-separated package ID.
    pub id: String,
    /// Exact semantic version selected by this project.
    pub version: String,
    /// Definition path relative to the project manifest.
    pub path: PathBuf,
    /// Semantic version requirement for the target language.
    pub language_version: String,
    /// Other packages required by this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ExtensionDependency>,
}

/// A local package dependency and accepted semantic-version range.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionDependency {
    /// Required package ID declared in the same project.
    pub id: String,
    /// Accepted semantic-version range.
    pub version: String,
}

pub(super) fn resolve_project_language(
    project_root: &Path,
    extensions: &[ManifestExtension],
    base: &ValidatedLanguageSpec,
) -> Result<ValidatedLanguageSpec, String> {
    if extensions.is_empty() {
        return Ok(base.clone());
    }
    let order = extension_order(extensions, base)?;
    let mut definitions = Vec::with_capacity(extensions.len());
    for index in order {
        let extension = &extensions[index];
        let path = project_root.join(extension.path());
        let mut definition = load_extension_definition(&path)?;
        if matches!(extension, ManifestExtension::Package(_)) {
            definition.version = base.spec().version.clone();
        }
        check_target(&definition, base)?;
        definitions.push(definition);
    }
    compose_extensions(base, &definitions).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn load_extension_definition(path: &Path) -> Result<LanguageSpec, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect extension definition '{}': {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "extension definition '{}' is not a regular file",
            path.display()
        ));
    }
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let bytes = read_bounded(path, 4 * 1024 * 1024, "extension definition")?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "cannot parse extension definition '{}': {error}",
                path.display()
            )
        })
    } else {
        parse_language_definition_project_spec_with_limits(path, DefinitionLimits::default())
            .map_err(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
    }
}

fn check_target(extension: &LanguageSpec, base: &ValidatedLanguageSpec) -> Result<(), String> {
    if extension.name != base.spec().name
        || extension.version != base.spec().version
        || extension.root != base.spec().root
        || extension.schema_version != base.spec().schema_version
    {
        return Err(format!(
            "extension definition `{}` does not target {} {} with root `{}`",
            extension.name,
            base.spec().name,
            base.spec().version,
            base.spec().root
        ));
    }
    Ok(())
}

/// Composes compatible partial definitions into a validated effective language.
///
/// Declarations are appended in extension order. Duplicate identifiers and
/// invalid references are rejected by canonical validation.
///
/// # Errors
/// Returns compatibility or canonical-spec validation errors.
pub fn compose_extensions(
    base: &ValidatedLanguageSpec,
    extensions: &[LanguageSpec],
) -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    let mut effective = base.spec().clone();
    let mut errors = Vec::new();
    for extension in extensions {
        if extension.name != effective.name
            || extension.version != effective.version
            || extension.root != effective.root
            || extension.schema_version != effective.schema_version
        {
            errors.push(SpecError {
                code: "LM2101",
                message: format!(
                    "extension definition `{}` does not target {} {} with root `{}`",
                    extension.name, effective.name, effective.version, effective.root
                ),
            });
            continue;
        }
        for source_extension in &extension.source_extensions {
            if !effective.source_extensions.contains(source_extension) {
                effective.source_extensions.push(source_extension.clone());
            }
        }
        effective
            .categories
            .extend(extension.categories.iter().cloned());
        effective.rules.extend(extension.rules.iter().cloned());
        effective.enums.extend(extension.enums.iter().cloned());
        effective.types.extend(extension.types.iter().cloned());
        effective
            .behaviors
            .extend(extension.behaviors.iter().cloned());
        effective
            .constructs
            .extend(extension.constructs.iter().cloned());
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    effective.validate()
}

fn extension_order(
    extensions: &[ManifestExtension],
    base: &ValidatedLanguageSpec,
) -> Result<Vec<usize>, String> {
    let mut package_indices = BTreeMap::<&str, usize>::new();
    let mut versions = BTreeMap::<&str, Version>::new();
    for (index, extension) in extensions.iter().enumerate() {
        let ManifestExtension::Package(package) = extension else {
            continue;
        };
        if !valid_package_id(&package.id) {
            return Err(format!("invalid extension package ID `{}`", package.id));
        }
        if package_indices.insert(&package.id, index).is_some() {
            return Err(format!(
                "extension package `{}` is listed more than once",
                package.id
            ));
        }
        let version = Version::parse(&package.version).map_err(|error| {
            format!(
                "invalid semantic version `{}` for extension `{}`: {error}",
                package.version, package.id
            )
        })?;
        let language_requirement =
            VersionReq::parse(&package.language_version).map_err(|error| {
                format!(
                    "invalid language version requirement `{}` for extension `{}`: {error}",
                    package.language_version, package.id
                )
            })?;
        let language_version = Version::parse(&base.spec().version).map_err(|error| {
            format!("base language version `{}` is not semantic and cannot be checked against extension `{}`: {error}", base.spec().version, package.id)
        })?;
        if !language_requirement.matches(&language_version) {
            return Err(format!(
                "extension `{}` version {} requires language `{}` {}; project uses {}",
                package.id,
                version,
                base.spec().name,
                language_requirement,
                language_version
            ));
        }
        versions.insert(&package.id, version);
    }

    let mut dependencies = vec![Vec::new(); extensions.len()];
    for (index, extension) in extensions.iter().enumerate() {
        let ManifestExtension::Package(package) = extension else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for dependency in &package.dependencies {
            if !valid_package_id(&dependency.id) || !seen.insert(dependency.id.as_str()) {
                return Err(format!(
                    "invalid or duplicate dependency `{}` in extension `{}`",
                    dependency.id, package.id
                ));
            }
            let requirement = VersionReq::parse(&dependency.version).map_err(|error| {
                format!("invalid version requirement `{}` for dependency `{}` of extension `{}`: {error}", dependency.version, dependency.id, package.id)
            })?;
            let Some(&dependency_index) = package_indices.get(dependency.id.as_str()) else {
                return Err(format!(
                    "extension `{}` requires package `{}` {}, but that package is not declared in the project",
                    package.id, dependency.id, requirement
                ));
            };
            let selected = &versions[dependency.id.as_str()];
            if !requirement.matches(selected) {
                return Err(format!(
                    "extension `{}` requires package `{}` {}, project selects {}",
                    package.id, dependency.id, requirement, selected
                ));
            }
            dependencies[index].push(dependency_index);
        }
    }

    let mut resolved = vec![false; extensions.len()];
    let mut order = Vec::with_capacity(extensions.len());
    while order.len() < extensions.len() {
        let next = (0..extensions.len()).find(|index| {
            !resolved[*index]
                && dependencies[*index]
                    .iter()
                    .all(|dependency| resolved[*dependency])
        });
        let Some(index) = next else {
            let remaining = extensions
                .iter()
                .enumerate()
                .filter(|(index, _)| !resolved[*index])
                .map(|(_, extension)| match extension {
                    ManifestExtension::Path(path) => path.display().to_string(),
                    ManifestExtension::Package(package) => package.id.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "extension dependency cycle detected among: {remaining}"
            ));
        };
        resolved[index] = true;
        order.push(index);
    }
    Ok(order)
}

fn valid_package_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}
