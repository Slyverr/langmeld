use std::path::Path;

use langmeld_language::{
    ExtensionDependency, ExtensionPackage, ManifestExtension, load_project_manifest,
    write_project_extensions,
};

use crate::cli::{ExtensionAddArgs, ExtensionCommand, ExtensionUpdateArgs};

pub(crate) fn manage_extensions(
    project_path: &Path,
    command: ExtensionCommand,
) -> Result<(), String> {
    match command {
        ExtensionCommand::List => list_extensions(project_path),
        ExtensionCommand::Add(arguments) => add_extension(project_path, arguments),
        ExtensionCommand::Update(arguments) => update_extension(project_path, arguments),
        ExtensionCommand::Remove(arguments) => {
            let manifest = load_project_manifest(project_path)?;
            let (id, path) = (arguments.id, arguments.path);
            let mut extensions = manifest.extensions;
            let index = extensions
                .iter()
                .position(|extension| match extension {
                    ManifestExtension::Path(declared) => path.as_ref() == Some(declared),
                    ManifestExtension::Package(package) => id.as_deref() == Some(&package.id),
                })
                .ok_or_else(|| "extension declaration was not found".to_owned())?;
            extensions.remove(index);
            write_project_extensions(project_path, extensions)?;
            println!("removed extension");
            Ok(())
        }
    }
}

fn list_extensions(project_path: &Path) -> Result<(), String> {
    let manifest = load_project_manifest(project_path)?;
    if manifest.extensions.is_empty() {
        println!("no extensions declared");
        return Ok(());
    }
    for extension in &manifest.extensions {
        match extension {
            ManifestExtension::Path(path) => {
                println!("{} (unversioned local definition)", path.display());
            }
            ManifestExtension::Package(package) => {
                println!(
                    "{}@{} ({})",
                    package.id,
                    package.version,
                    package.path.display()
                );
            }
        }
    }
    Ok(())
}

fn add_extension(project_path: &Path, arguments: ExtensionAddArgs) -> Result<(), String> {
    let mut manifest = load_project_manifest(project_path)?;
    let extension = if let Some(path) = arguments.path_only {
        ManifestExtension::Path(path)
    } else {
        let id = required(arguments.id, "--id")?;
        let package = ExtensionPackage {
            id,
            version: required(arguments.version, "--version")?,
            path: required(arguments.path, "--path")?,
            language_version: required(arguments.language_version, "--language-version")?,
            dependencies: parse_dependencies(arguments.dependencies)?,
        };
        if manifest.extensions.iter().any(|extension| {
            matches!(extension, ManifestExtension::Package(existing) if existing.id == package.id)
        }) {
            return Err(format!("extension package `{}` is already declared", package.id));
        }
        ManifestExtension::Package(package)
    };
    manifest.extensions.push(extension);
    write_project_extensions(project_path, manifest.extensions)?;
    println!("added extension");
    Ok(())
}

fn update_extension(project_path: &Path, arguments: ExtensionUpdateArgs) -> Result<(), String> {
    let changed = arguments.version.is_some()
        || arguments.path.is_some()
        || arguments.language_version.is_some()
        || !arguments.dependencies.is_empty()
        || arguments.clear_dependencies;
    if !changed {
        return Err("update requires at least one field to change".to_owned());
    }
    let mut manifest = load_project_manifest(project_path)?;
    let package = manifest
        .extensions
        .iter_mut()
        .find_map(|extension| match extension {
            ManifestExtension::Package(package) if package.id == arguments.id => Some(package),
            _ => None,
        })
        .ok_or_else(|| format!("extension package `{}` was not found", arguments.id))?;
    if let Some(version) = arguments.version {
        package.version = version;
    }
    if let Some(path) = arguments.path {
        package.path = path;
    }
    if let Some(language_version) = arguments.language_version {
        package.language_version = language_version;
    }
    if arguments.clear_dependencies {
        package.dependencies.clear();
    } else if !arguments.dependencies.is_empty() {
        package.dependencies = parse_dependencies(arguments.dependencies)?;
    }
    write_project_extensions(project_path, manifest.extensions)?;
    println!("updated extension {}", arguments.id);
    Ok(())
}

fn parse_dependencies(values: Vec<String>) -> Result<Vec<ExtensionDependency>, String> {
    values
        .into_iter()
        .map(|value| {
            let (id, version) = value.rsplit_once('@').ok_or_else(|| {
                format!("invalid dependency `{value}`; expected <id>@<version-requirement>")
            })?;
            if id.is_empty() || version.is_empty() {
                return Err(format!(
                    "invalid dependency `{value}`; expected <id>@<version-requirement>"
                ));
            }
            Ok(ExtensionDependency {
                id: id.to_owned(),
                version: version.to_owned(),
            })
        })
        .collect()
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("package add requires {flag}"))
}
