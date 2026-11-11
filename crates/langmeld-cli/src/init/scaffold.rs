use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use super::{AgentGuidance, Template, templates::project_files};

pub(super) fn create_project(
    path: &Path,
    package_name: &str,
    language_name: &str,
    template: Template,
    agent_guidance: AgentGuidance,
) -> io::Result<()> {
    let files = project_files(package_name, language_name, template, agent_guidance);
    if path.exists() && !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("project path `{}` is not a directory", path.display()),
        ));
    }
    if path.exists() {
        for (relative, _) in &files {
            if path.join(relative).exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "`{}` already exists; no files were changed",
                        path.join(relative).display()
                    ),
                ));
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
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            if let Err(error) = file.write_all(contents.as_bytes()) {
                drop(file);
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
            created_files.push(destination);
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        for file in created_files.iter().rev() {
            let _ = fs::remove_file(file);
        }
        for directory in created_directories.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
        return Err(error);
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
