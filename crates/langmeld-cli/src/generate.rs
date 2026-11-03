use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use langmeld_language::{generate_tooling, parse_language_definition_project};

use crate::cli::GenerateArgs;

type Files = BTreeMap<PathBuf, String>;

pub(crate) fn generate(arguments: GenerateArgs) -> Result<(), String> {
    let root = arguments.project_root;
    let definition = root.join("language/language.lm");
    let language = parse_language_definition_project(&definition)
        .map_err(|diagnostics| format_diagnostics(&definition, &diagnostics))?;
    let files = generate_tooling(&language)
        .map_err(|error| format!("cannot generate language tooling: {error}"))?
        .into_iter()
        .map(|(path, contents)| (PathBuf::from(path), contents))
        .collect::<Files>();
    for path in files.keys() {
        ensure_safe_relative(path)?;
    }

    let state = root.join(".langmeld/generated");
    if arguments.check {
        let actual = read_generated(&state)?;
        if actual != files {
            return Err(describe_difference(&actual, &files));
        }
        println!("generated tooling is up to date ({})", state.display());
        return Ok(());
    }

    let actual = read_generated(&state)?;
    if actual == files {
        println!(
            "generated tooling is already up to date ({})",
            state.display()
        );
        return Ok(());
    }
    replace_generated(&root, &files)?;
    println!(
        "generated {} tooling files in {}",
        files.len(),
        state.display()
    );
    Ok(())
}

fn format_diagnostics(path: &Path, diagnostics: &[langmeld_core::Diagnostic]) -> String {
    let detail = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "cannot load language definition '{}':\n{detail}",
        path.display()
    )
}

fn ensure_safe_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "generated artifact path `{}` is unsafe",
            path.display()
        ));
    }
    Ok(())
}

fn read_generated(root: &Path) -> Result<Files, String> {
    if let Some(parent) = root.parent() {
        match fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "Langmeld state path '{}' is not a plain directory",
                    parent.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Files::new()),
            Err(error) => return Err(format!("cannot inspect '{}': {error}", parent.display())),
        }
    }
    let metadata = match fs::symlink_metadata(root) {
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
    collect_files(root, root, &mut files)?;
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
            "generated output is missing '{}'; run `langmeld generate`",
            path.display()
        );
    }
    if let Some(path) = actual.keys().find(|path| !expected.contains_key(*path)) {
        return format!(
            "generated output has stale file '{}'; run `langmeld generate`",
            path.display()
        );
    }
    if let Some(path) = expected
        .iter()
        .find_map(|(path, contents)| (actual.get(path) != Some(contents)).then_some(path))
    {
        return format!(
            "generated output differs at '{}'; run `langmeld generate`",
            path.display()
        );
    }
    "generated output differs from the validated language definition".to_owned()
}

fn replace_generated(root: &Path, files: &Files) -> Result<(), String> {
    let state_root = root.join(".langmeld");
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
        if had_previous {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(format!(
            "cannot install generated output '{}': {error}",
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
