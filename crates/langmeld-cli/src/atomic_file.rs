use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use tempfile::NamedTempFile;

pub(crate) fn replace_existing(path: &Path, contents: &str) -> io::Result<()> {
    let target = fs::canonicalize(path)?;
    let metadata = fs::metadata(&target)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace non-file `{}`", target.display()),
        ));
    }

    let permissions = metadata.permissions();
    if permissions.readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to replace read-only file `{}`", target.display()),
        ));
    }

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
