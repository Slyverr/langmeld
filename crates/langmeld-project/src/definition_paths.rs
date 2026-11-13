use std::path::{Component, Path};

use crate::{ProjectLoadError, error::path_error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DefinitionPathError {
    Invalid,
    OutsideRoot,
}

pub(super) fn normalize_source_key(path: &str) -> Result<String, DefinitionPathError> {
    if !is_portable_relative_path(path)
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(DefinitionPathError::Invalid);
    }

    let components = Path::new(path)
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or(DefinitionPathError::Invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err(DefinitionPathError::Invalid);
    }
    Ok(components.join("/"))
}

/// Resolves an import path against its importing definition's project key.
///
/// This performs the same portable-path and project-root checks used by the
/// project resolvers without loading a file. It is useful to validate import
/// paths before presenting them in editor tooling.
///
/// # Errors
///
/// Returns [`ProjectLoadError::InvalidPath`] for malformed or non-portable
/// paths, or [`ProjectLoadError::OutsideRoot`] when resolving the import would
/// escape the project root.
pub fn resolve_import_key(from_key: &str, import: &str) -> Result<String, ProjectLoadError> {
    resolve_import_key_inner(from_key, import).map_err(|error| path_error(import, error))
}

fn resolve_import_key_inner(from_key: &str, import: &str) -> Result<String, DefinitionPathError> {
    let from_key = normalize_source_key(from_key)?;
    if !is_portable_relative_path(import)
        || Path::new(import).components().any(|component| {
            !matches!(
                component,
                Component::Normal(_) | Component::CurDir | Component::ParentDir
            )
        })
    {
        return Err(DefinitionPathError::Invalid);
    }

    let mut components = from_key.split('/').collect::<Vec<_>>();
    components.pop();
    for component in Path::new(import).components() {
        match component {
            Component::Normal(name) => {
                components.push(name.to_str().ok_or(DefinitionPathError::Invalid)?)
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(DefinitionPathError::OutsideRoot);
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(DefinitionPathError::Invalid);
            }
        }
    }
    if components.is_empty() {
        return Err(DefinitionPathError::Invalid);
    }
    Ok(components.join("/"))
}

/// Rejects paths that cannot be represented consistently on Windows.
fn is_portable_relative_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && path.split('/').all(is_portable_component)
}

fn is_portable_component(component: &str) -> bool {
    if component.is_empty() || matches!(component, "." | "..") {
        return true;
    }

    if component.ends_with([' ', '.'])
        || component.chars().any(|character| {
            character <= '\u{1f}'
                || matches!(character, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*')
        })
    {
        return false;
    }

    !is_reserved_windows_device_name(component)
}

fn is_reserved_windows_device_name(component: &str) -> bool {
    // Windows reserves these device-name stems even when followed by an extension.
    let name = component
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');

    if ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
    {
        return true;
    }

    ["COM", "LPT"].iter().any(|prefix| {
        name.get(..3)
            .is_some_and(|name_prefix| name_prefix.eq_ignore_ascii_case(prefix))
            && matches!(
                name.get(3..),
                Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            )
    })
}
