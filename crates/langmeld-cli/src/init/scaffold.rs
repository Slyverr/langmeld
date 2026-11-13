use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use super::{AgentGuidance, Template, templates::project_files};
use crate::file_ops::{include_rollback_errors, rollback_created_paths, write_new_file};

pub(super) fn create_project(
    path: &Path,
    package_name: &str,
    language_name: &str,
    template: Template,
    typed_bindings: bool,
    agent_guidance: AgentGuidance,
    local_crate_root: Option<&Path>,
) -> io::Result<()> {
    let files = project_files(
        package_name,
        language_name,
        template,
        typed_bindings,
        agent_guidance,
        local_crate_root,
    )?;
    if path.exists() && !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("project path `{}` is not a directory", path.display()),
        ));
    }
    if path.exists() {
        for (relative, _) in &files {
            let destination = path.join(relative);
            match fs::symlink_metadata(&destination) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "`{}` already exists; no files were changed",
                            destination.display()
                        ),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }

    fs::create_dir_all(path)?;
    let root = path.canonicalize()?;
    let mut created_files = Vec::new();
    let mut created_directories = Vec::new();
    let write_result = (|| {
        for (relative, contents) in files {
            let destination = root.join(relative);
            let parent = destination.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "generated file has no parent")
            })?;
            ensure_parent_directories(&root, parent, &mut created_directories)?;
            write_new_file(&destination, &contents)?;
            created_files.push(destination);
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        let rollback_errors = rollback_created_paths(
            created_files.iter().rev().map(PathBuf::as_path),
            created_directories.iter().rev().map(PathBuf::as_path),
        );
        return Err(include_rollback_errors(error, rollback_errors));
    }
    Ok(())
}

fn ensure_parent_directories(
    root: &Path,
    parent: &Path,
    created: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let relative = parent.strip_prefix(root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "generated path escaped the project root",
        )
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "generated path contains an invalid directory component",
            ));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing to write through symlink `{}`", current.display()),
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("`{}` exists and is not a directory", current.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                created.push(current.clone());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
