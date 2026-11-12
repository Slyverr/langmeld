use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use tempfile::NamedTempFile;

pub(crate) fn write_new_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        drop(file);
        if let Err(cleanup_error) = fs::remove_file(path)
            && cleanup_error.kind() != io::ErrorKind::NotFound
        {
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; could not remove incomplete generated file `{}`: {cleanup_error}",
                    path.display()
                ),
            ));
        }
        return Err(error);
    }

    Ok(())
}

pub(crate) fn replace_existing(path: &Path, contents: &str) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace symlink `{}`", path.display()),
        ));
    }
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace non-file `{}`", path.display()),
        ));
    }

    let permissions = metadata.permissions();
    if permissions.readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to replace read-only file `{}`", path.display()),
        ));
    }

    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("file `{}` has no file name", path.display()),
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let target = fs::canonicalize(parent)?.join(file_name);
    let parent = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("file `{}` has no parent directory", target.display()),
        )
    })?;
    let mut temporary = NamedTempFile::new_in(parent)?;

    if let Err(error) = temporary.write_all(contents.as_bytes()) {
        return Err(with_temp_cleanup_error(temporary, error));
    }
    if let Err(error) = temporary.as_file().set_permissions(permissions) {
        return Err(with_temp_cleanup_error(temporary, error));
    }
    if let Err(error) = temporary.as_file().sync_all() {
        return Err(with_temp_cleanup_error(temporary, error));
    }

    match temporary.persist(&target) {
        Ok(_) => Ok(()),
        Err(error) => {
            let tempfile::PersistError { error, file } = error;
            Err(with_temp_cleanup_error(file, error))
        }
    }
}

fn with_temp_cleanup_error(temporary: NamedTempFile, error: io::Error) -> io::Error {
    let path = temporary.path().to_path_buf();

    match temporary.close() {
        Ok(()) => error,
        Err(cleanup_error) => io::Error::new(
            error.kind(),
            format!(
                "{error}; could not remove temporary file `{}`: {cleanup_error}",
                path.display()
            ),
        ),
    }
}
