use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

use crate::{
    file_ops::{replace_existing, write_new_file},
    handler_generation::{ensure_new_handler_path, ensure_new_path},
};

pub(super) struct ConstructFiles<'a> {
    pub(super) root_path: &'a Path,
    pub(super) root_source: &'a str,
    pub(super) updated_root: &'a str,
    pub(super) registry_path: &'a Path,
    pub(super) registry_source: &'a str,
    pub(super) updated_registry: &'a str,
    pub(super) syntax_directory: &'a Path,
    pub(super) syntax_path: &'a Path,
    pub(super) syntax_source: &'a str,
    pub(super) handler_path: &'a Path,
    pub(super) handler_source: &'a str,
    pub(super) construct_name: &'a str,
    pub(super) handler_module_name: &'a str,
    pub(super) colliding_construct: Option<&'a str>,
}

pub(super) fn apply(files: ConstructFiles<'_>) -> io::Result<()> {
    ensure_new_path(files.syntax_path, "syntax file")?;
    ensure_new_handler_path(
        files.handler_path,
        files.construct_name,
        files.handler_module_name,
        files.colliding_construct,
    )?;

    let created_directory = match fs::symlink_metadata(files.syntax_directory) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "refusing to write into `{}`",
                        files.syntax_directory.display()
                    ),
                ));
            }
            false
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir(files.syntax_directory)?;
            true
        }
        Err(error) => return Err(error),
    };

    if let Err(error) = write_new_file(files.syntax_path, files.syntax_source) {
        let rollback_errors =
            rollback_generated_paths(&[], created_directory.then_some(files.syntax_directory));
        return Err(with_rollback_errors(error, rollback_errors));
    }

    if let Err(error) = write_new_file(files.handler_path, files.handler_source) {
        let rollback_errors = rollback_generated_paths(
            &[files.syntax_path],
            created_directory.then_some(files.syntax_directory),
        );
        return Err(with_rollback_errors(error, rollback_errors));
    }

    if let Err(error) = replace_existing(files.root_path, files.updated_root) {
        let mut rollback_errors = rollback_generated_paths(
            &[files.handler_path, files.syntax_path],
            created_directory.then_some(files.syntax_directory),
        );
        restore_file(
            files.root_path,
            files.root_source,
            "language definition",
            &mut rollback_errors,
        );
        return Err(with_rollback_errors(error, rollback_errors));
    }

    if let Err(error) = replace_existing(files.registry_path, files.updated_registry) {
        let mut rollback_errors = rollback_generated_paths(
            &[files.handler_path, files.syntax_path],
            created_directory.then_some(files.syntax_directory),
        );
        restore_file(
            files.registry_path,
            files.registry_source,
            "handler registry",
            &mut rollback_errors,
        );
        restore_file(
            files.root_path,
            files.root_source,
            "language definition",
            &mut rollback_errors,
        );
        return Err(with_rollback_errors(error, rollback_errors));
    }

    Ok(())
}

fn rollback_generated_paths(files: &[&Path], directory: Option<&Path>) -> Vec<String> {
    let mut errors = Vec::new();

    for path in files {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != ErrorKind::NotFound
        {
            errors.push(format!(
                "could not remove generated file `{}`: {error}",
                path.display()
            ));
        }
    }

    if let Some(directory) = directory
        && let Err(error) = fs::remove_dir(directory)
        && error.kind() != ErrorKind::NotFound
    {
        errors.push(format!(
            "could not remove generated directory `{}`: {error}",
            directory.display()
        ));
    }

    errors
}

fn restore_file(path: &Path, contents: &str, description: &str, errors: &mut Vec<String>) {
    if fs::read(path).is_ok_and(|current| current.as_slice() == contents.as_bytes()) {
        return;
    }

    if let Err(error) = replace_existing(path, contents) {
        errors.push(format!(
            "could not restore {description} `{}`: {error}",
            path.display()
        ));
    }
}

fn with_rollback_errors(error: io::Error, rollback_errors: Vec<String>) -> io::Error {
    if rollback_errors.is_empty() {
        return error;
    }

    io::Error::new(
        error.kind(),
        format!(
            "{error}; rollback was incomplete: {}",
            rollback_errors.join("; ")
        ),
    )
}
