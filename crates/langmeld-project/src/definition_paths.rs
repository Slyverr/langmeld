use std::path::{Component, Path};

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

pub(super) fn resolve_import_key(
    from_key: &str,
    import: &str,
) -> Result<String, DefinitionPathError> {
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

/// Rejects Windows drive/stream syntax and native separators from portable paths.
fn is_portable_relative_path(path: &str) -> bool {
    !path.is_empty() && !path.contains('\\') && !path.contains(':') && !path.starts_with('/')
}
