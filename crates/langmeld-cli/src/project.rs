//! Finding the root file of a language project.

use std::path::{Path, PathBuf};

use langmeld_meta::{MANIFEST_NAME, Manifest};
use langmeld_syntax::{FileId, SourceFile};

/// A language project on the disk.
pub(crate) struct Project {
    /// The folder of the project: where `langmeld.toml` is, or the root file's folder.
    pub(crate) dir: PathBuf,
    /// The root `.langmeld` file.
    pub(crate) root: PathBuf,
}

/// Why a path cannot be used: text for the user.
pub(crate) struct Refused(pub(crate) String);

impl Refused {
    /// The reason as a failure of a command that reports its own `error:` line.
    pub(crate) fn failure(self) -> langmeld_cli::Failure {
        let text = self.0.trim_end();
        langmeld_cli::Failure::Unable(text.strip_prefix("error: ").unwrap_or(text).to_owned())
    }
}

/// The project that `path` names: a `.langmeld` file, or a folder with a manifest.
pub(crate) fn find(path: &Path) -> Result<Project, Refused> {
    if path.is_file() {
        let dir = path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok(Project {
            dir,
            root: path.to_path_buf(),
        });
    }
    if !path.is_dir() {
        return Err(Refused(format!(
            "error: `{}` is not a file or a folder",
            path.display()
        )));
    }
    let manifest_path = path.join(MANIFEST_NAME);
    let manifest = if manifest_path.is_file() {
        let text = std::fs::read_to_string(&manifest_path).map_err(|error| {
            Refused(format!(
                "error: cannot read `{}`: {error}",
                manifest_path.display()
            ))
        })?;
        let name = manifest_path.to_string_lossy().replace('\\', "/");
        let file = SourceFile::new(FileId::new(0), name, text)
            .map_err(|error| Refused(format!("error: {error}")))?;
        Manifest::parse(&file).map_err(|problems| {
            Refused(langmeld_meta::render(
                std::slice::from_ref(&file),
                &problems,
            ))
        })?
    } else {
        Manifest::default()
    };
    Ok(Project {
        dir: path.to_path_buf(),
        root: path.join(manifest.root),
    })
}

/// A path as it is shown: with `/`, and without a leading `./`.
pub(crate) fn shown(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_prefix("./").map_or(text.clone(), str::to_owned)
}
