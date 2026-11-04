use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::tooling::GeneratedArtifacts;

type Files = BTreeMap<PathBuf, String>;

/// Result of writing a complete generated-artifact set into a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedArtifactWrite {
    /// The expected files already matched the project's generated directory.
    UpToDate {
        /// Number of generated files in the complete artifact set.
        file_count: usize,
    },
    /// The generated directory was replaced with the expected artifact set.
    Updated {
        /// Number of generated files installed.
        file_count: usize,
    },
}

/// Checks that a project's complete generated-artifact directory matches `expected`.
///
/// The project owns `.langmeld/generated`; this function reads that directory
/// without modifying it. Symlinks and special files anywhere inside the owned
/// output are rejected.
///
/// # Errors
/// Returns an actionable message if the output is unsafe, unreadable, or stale.
pub fn check_generated_artifacts(
    project_root: &Path,
    expected: &GeneratedArtifacts,
) -> Result<(), String> {
    let expected = normalize_artifacts(expected)?;
    if expected.is_empty() {
        return Err("generated artifact set is empty".to_owned());
    }
    let actual = read_generated(project_root)?;
    if actual != expected {
        return Err(describe_difference(&actual, &expected));
    }
    Ok(())
}

/// Safely writes the complete generated-artifact set to `.langmeld/generated`.
///
/// Existing generated output is staged and replaced as a unit. Files outside
/// the generated directory are not touched. An empty artifact set is rejected
/// so an accidental empty result cannot erase prior output.
///
/// # Errors
/// Returns an actionable message if an artifact path or existing output is
/// unsafe, the output cannot be read or written, or replacement fails.
pub fn write_generated_artifacts(
    project_root: &Path,
    artifacts: &GeneratedArtifacts,
) -> Result<GeneratedArtifactWrite, String> {
    let expected = normalize_artifacts(artifacts)?;
    if expected.is_empty() {
        return Err("generated artifact set is empty".to_owned());
    }
    let actual = read_generated(project_root)?;
    let file_count = expected.len();
    if actual == expected {
        return Ok(GeneratedArtifactWrite::UpToDate { file_count });
    }
    replace_generated(project_root, &expected)?;
    Ok(GeneratedArtifactWrite::Updated { file_count })
}

fn normalize_artifacts(artifacts: &GeneratedArtifacts) -> Result<Files, String> {
    let mut files = Files::new();
    for (relative, contents) in artifacts {
        let path = PathBuf::from(relative);
        ensure_safe_relative(relative, &path)?;
        files.insert(path, contents.clone());
    }
    for path in files.keys() {
        let mut parent = path.parent();
        while let Some(directory) = parent {
            if files.contains_key(directory) {
                return Err(format!(
                    "generated artifact paths conflict at '{}'",
                    directory.display()
                ));
            }
            parent = directory.parent();
        }
    }
    Ok(files)
}

fn ensure_safe_relative(original: &str, path: &Path) -> Result<(), String> {
    if original.is_empty()
        || original.contains('\\')
        || original.contains(':')
        || original
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("generated artifact path `{original}` is unsafe"));
    }
    Ok(())
}

fn read_generated(project_root: &Path) -> Result<Files, String> {
    let state_root = project_root.join(".langmeld");
    match fs::symlink_metadata(&state_root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!(
                "Langmeld state path '{}' is not a plain directory",
                state_root.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Files::new()),
        Err(error) => {
            return Err(format!(
                "cannot inspect '{}': {error}",
                state_root.display()
            ));
        }
    }

    let root = state_root.join("generated");
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Files::new()),
        Err(error) => return Err(format!("cannot inspect '{}': {error}", root.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "generated output '{}' is not a plain directory",
            root.display()
        ));
    }
    let mut files = Files::new();
    collect_files(&root, &root, &mut files)?;
    Ok(files)
}

fn collect_files(root: &Path, directory: &Path, files: &mut Files) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot read generated output '{}': {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read generated entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "generated output contains a symlink: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("cannot relativize '{}': {error}", path.display()))?
                .to_path_buf();
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
            files.insert(relative, contents);
        } else {
            return Err(format!(
                "generated output contains a special file: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn describe_difference(actual: &Files, expected: &Files) -> String {
    if let Some(path) = expected.keys().find(|path| !actual.contains_key(*path)) {
        return format!(
            "generated output is missing '{}'; regenerate the project's tooling",
            path.display()
        );
    }
    if let Some(path) = actual.keys().find(|path| !expected.contains_key(*path)) {
        return format!(
            "generated output has stale file '{}'; regenerate the project's tooling",
            path.display()
        );
    }
    if let Some(path) = expected
        .iter()
        .find_map(|(path, contents)| (actual.get(path) != Some(contents)).then_some(path))
    {
        return format!(
            "generated output differs at '{}'; regenerate the project's tooling",
            path.display()
        );
    }
    "generated output differs from the validated language definition".to_owned()
}

fn replace_generated(project_root: &Path, files: &Files) -> Result<(), String> {
    let state_root = project_root.join(".langmeld");
    match fs::symlink_metadata(&state_root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!(
                "Langmeld state path '{}' is not a plain directory",
                state_root.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&state_root).map_err(|error| {
                format!(
                    "cannot create Langmeld state directory '{}': {error}",
                    state_root.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "cannot inspect '{}': {error}",
                state_root.display()
            ));
        }
    }

    let stage = state_root.join(format!(".generated-stage-{}", std::process::id()));
    fs::create_dir(&stage).map_err(|error| {
        format!(
            "cannot create staging directory '{}': {error}",
            stage.display()
        )
    })?;
    let mut guard = StagingDirectory {
        path: stage.clone(),
        committed: false,
    };
    for (relative, contents) in files {
        let path = stage.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create '{}': {error}", parent.display()))?;
        }
        fs::write(&path, contents)
            .map_err(|error| format!("cannot write '{}': {error}", path.display()))?;
    }

    let destination = state_root.join("generated");
    let backup = state_root.join(format!(".generated-backup-{}", std::process::id()));
    match fs::symlink_metadata(&backup) {
        Ok(_) => {
            return Err(format!(
                "previous generated backup '{}' must be reviewed before another replacement",
                backup.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "cannot inspect generated backup '{}': {error}",
                backup.display()
            ));
        }
    }
    let had_previous = destination.exists();
    if had_previous {
        fs::rename(&destination, &backup).map_err(|error| {
            format!(
                "cannot stage previous generated output '{}': {error}",
                destination.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&stage, &destination) {
        let rollback = if had_previous {
            fs::rename(&backup, &destination)
                .err()
                .map(|rollback_error| format!("; rollback also failed: {rollback_error}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        return Err(format!(
            "cannot install generated output '{}': {error}{rollback}",
            destination.display()
        ));
    }
    guard.committed = true;
    if had_previous {
        fs::remove_dir_all(&backup).map_err(|error| {
            format!(
                "generated output updated but old files remain at '{}': {error}",
                backup.display()
            )
        })?;
    }
    Ok(())
}

struct StagingDirectory {
    path: PathBuf,
    committed: bool,
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
