use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use super::merge::merge_host_scaffold;

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct PlannedFile {
    path: PathBuf,
    original: Option<Vec<u8>>,
    replacement: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct HostInstallPlan {
    host_directory: PathBuf,
    files: Vec<PlannedFile>,
    preserved: Vec<PathBuf>,
}

pub(crate) fn prepare_host_install(
    artifacts: &BTreeMap<String, String>,
    project_root: &Path,
) -> Result<HostInstallPlan, String> {
    let source_root = project_root.join("src");
    ensure_plain_directory(&source_root)?;
    let host_directory = source_root.join("host");
    ensure_directory_or_missing(&host_directory)?;
    let lib_path = source_root.join("lib.rs");
    let lib_source = read_plain_file(&lib_path)?;
    let mut files = BTreeMap::<PathBuf, PlannedFile>::new();
    let mut preserved = Vec::new();

    for (relative, generated) in artifacts {
        let Some(host_relative) = relative.strip_prefix("host/") else {
            continue;
        };
        let relative_path = Path::new(host_relative);
        ensure_safe_relative(relative_path)?;
        let destination = host_directory.join(relative_path);
        ensure_safe_parent(&host_directory, relative_path)?;
        let original = read_optional_plain_file(&destination)?;
        let Some(original_bytes) = original.as_ref() else {
            files.insert(
                destination.clone(),
                PlannedFile {
                    path: destination,
                    original: None,
                    replacement: generated.as_bytes().to_vec(),
                },
            );
            continue;
        };
        if original_bytes == generated.as_bytes() {
            continue;
        }

        let replacement = match host_relative {
            "parser.rs" | "analysis.rs" | "lexer.rs" => {
                let existing = std::str::from_utf8(original_bytes).map_err(|error| {
                    format!(
                        "cannot merge non-UTF-8 host file '{}': {error}",
                        destination.display()
                    )
                })?;
                merge_host_scaffold(existing, generated).map_err(|error| {
                    format!("cannot update '{}': {error}", destination.display())
                })?
            }
            "mod.rs" => {
                let existing = std::str::from_utf8(original_bytes).map_err(|error| {
                    format!(
                        "cannot merge non-UTF-8 host module '{}': {error}",
                        destination.display()
                    )
                })?;
                merge_module_declarations(existing, generated)
            }
            _ => {
                preserved.push(destination);
                continue;
            }
        };
        if replacement.as_bytes() != original_bytes {
            files.insert(
                destination.clone(),
                PlannedFile {
                    path: destination,
                    original,
                    replacement: replacement.into_bytes(),
                },
            );
        }
    }

    if !has_module_declaration(&lib_source, "host") {
        let mut replacement = lib_source.as_bytes().to_vec();
        if !replacement.is_empty() && !replacement.ends_with(b"\n") {
            replacement.push(b'\n');
        }
        replacement.extend_from_slice(b"pub mod host;\n");
        files.insert(
            lib_path.clone(),
            PlannedFile {
                path: lib_path.clone(),
                original: Some(lib_source.into_bytes()),
                replacement,
            },
        );
    }

    Ok(HostInstallPlan {
        host_directory,
        files: files.into_values().collect(),
        preserved,
    })
}

fn ensure_safe_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("host artifact path '{}' is unsafe", path.display()));
    }
    Ok(())
}

fn ensure_plain_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "host installation requires a real directory at '{}'; refusing links and non-directories",
            path.display()
        )),
        Err(error) => Err(format!("cannot inspect '{}': {error}", path.display())),
    }
}

fn ensure_directory_or_missing(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "host installation requires a real directory at '{}'; refusing links and non-directories",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect '{}': {error}", path.display())),
    }
}

fn ensure_safe_parent(root: &Path, relative: &Path) -> Result<(), String> {
    let mut parent = root.to_path_buf();
    if let Some(components) = relative.parent() {
        for component in components.components() {
            let Component::Normal(name) = component else {
                return Err(format!(
                    "host artifact path '{}' is unsafe",
                    relative.display()
                ));
            };
            parent.push(name);
            match fs::symlink_metadata(&parent) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(format!(
                        "refusing to follow host path component '{}'",
                        parent.display()
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!("cannot inspect '{}': {error}", parent.display()));
                }
            }
        }
    }
    Ok(())
}

fn read_plain_file(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("host installation requires '{}': {error}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to follow or replace non-regular file '{}'",
            path.display()
        ));
    }
    fs::read_to_string(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))
}

fn read_optional_plain_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => fs::read(path)
            .map(Some)
            .map_err(|error| format!("cannot read '{}': {error}", path.display())),
        Ok(_) => Err(format!(
            "refusing to follow or replace host destination '{}'",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot inspect '{}': {error}", path.display())),
    }
}

fn merge_module_declarations(existing: &str, generated: &str) -> String {
    let existing_names = module_names(existing);
    let missing = generated
        .lines()
        .filter_map(module_name)
        .filter(|name| !existing_names.contains(name))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return existing.to_owned();
    }
    let mut merged = existing.to_owned();
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    for name in missing {
        merged.push_str("pub mod ");
        merged.push_str(&name);
        merged.push_str(";\n");
    }
    merged
}

fn module_names(source: &str) -> std::collections::BTreeSet<String> {
    source.lines().filter_map(module_name).collect()
}

fn module_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.starts_with("//") || line.starts_with("/*") || !line.ends_with(';') {
        return None;
    }
    let after_mod = line.rsplit_once("mod ")?.1;
    let name = after_mod.strip_suffix(';')?.trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|character| character == '_' || character.is_ascii_alphanumeric()))
    .then(|| name.to_owned())
}

fn has_module_declaration(source: &str, module: &str) -> bool {
    module_names(source).contains(module)
}

pub(crate) fn apply_host_install(plan: HostInstallPlan) -> Result<(), String> {
    apply_host_install_with_failure(plan, None)
}

fn apply_host_install_with_failure(
    plan: HostInstallPlan,
    fail_after_applied: Option<usize>,
) -> Result<(), String> {
    let source_root = plan
        .host_directory
        .parent()
        .ok_or_else(|| "host source directory has no parent".to_owned())?;
    ensure_plain_directory(source_root)?;
    let created_directory = match fs::symlink_metadata(&plan.host_directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => false,
        Ok(_) => {
            return Err(format!(
                "host installation requires a real directory at '{}'; refusing links and non-directories",
                plan.host_directory.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&plan.host_directory).map_err(|error| {
                format!(
                    "cannot create host source directory '{}': {error}",
                    plan.host_directory.display()
                )
            })?;
            true
        }
        Err(error) => {
            return Err(format!(
                "cannot inspect '{}': {error}",
                plan.host_directory.display()
            ));
        }
    };

    let mut staged = Vec::new();
    for file in &plan.files {
        match stage_file(file) {
            Ok(temp) => staged.push((file, temp)),
            Err(error) => {
                remove_staged(&staged);
                if created_directory {
                    let _ = fs::remove_dir(&plan.host_directory);
                }
                return Err(error);
            }
        }
    }

    for file in &plan.files {
        let parent = file
            .path
            .parent()
            .ok_or_else(|| format!("'{}' has no parent", file.path.display()))?;
        if let Err(error) = ensure_plain_directory(parent) {
            remove_staged(&staged);
            if created_directory {
                let _ = fs::remove_dir(&plan.host_directory);
            }
            return Err(error);
        }
        let current = match read_optional_plain_file(&file.path) {
            Ok(current) => current,
            Err(error) => {
                remove_staged(&staged);
                if created_directory {
                    let _ = fs::remove_dir(&plan.host_directory);
                }
                return Err(error);
            }
        };
        if current != file.original {
            remove_staged(&staged);
            if created_directory {
                let _ = fs::remove_dir(&plan.host_directory);
            }
            return Err(format!(
                "'{}' changed during host installation; no changes were applied",
                file.path.display()
            ));
        }
    }

    let mut applied = Vec::<AppliedFile>::new();
    for (file, temp) in &staged {
        if fail_after_applied.is_some_and(|limit| applied.len() >= limit) {
            let error = rollback_error("injected host install failure".to_owned(), &applied);
            remove_staged(&staged);
            if created_directory {
                let _ = fs::remove_dir(&plan.host_directory);
            }
            return Err(error);
        }
        if let Some(parent) = file.path.parent()
            && let Err(error) = ensure_plain_directory(parent)
        {
            let error = rollback_error(error, &applied);
            remove_staged(&staged);
            if created_directory {
                let _ = fs::remove_dir(&plan.host_directory);
            }
            return Err(error);
        }
        let backup = if file.original.is_some() {
            let backup = match unique_sibling(&file.path, "backup") {
                Ok(backup) => backup,
                Err(error) => {
                    let error = rollback_error(error, &applied);
                    remove_staged(&staged);
                    if created_directory {
                        let _ = fs::remove_dir(&plan.host_directory);
                    }
                    return Err(error);
                }
            };
            if let Err(error) = fs::rename(&file.path, &backup) {
                let error = rollback_error(
                    format!(
                        "cannot stage previous host file '{}': {error}",
                        file.path.display()
                    ),
                    &applied,
                );
                remove_staged(&staged);
                if created_directory {
                    let _ = fs::remove_dir(&plan.host_directory);
                }
                return Err(error);
            }
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(temp, &file.path) {
            if let Some(backup_path) = &backup {
                let _ = fs::rename(backup_path, &file.path);
            }
            let error = rollback_error(
                format!(
                    "cannot install host file '{}': {error}",
                    file.path.display()
                ),
                &applied,
            );
            remove_staged(&staged);
            if created_directory {
                let _ = fs::remove_dir(&plan.host_directory);
            }
            return Err(error);
        }
        applied.push(AppliedFile {
            path: file.path.clone(),
            backup,
            replacement: file.replacement.clone(),
        });
    }

    for file in applied {
        if let Some(backup) = file.backup {
            let _ = fs::remove_file(backup);
        }
    }
    println!("installed or refreshed Rust host modules under src/host/");
    for path in plan.preserved {
        eprintln!(
            "langmeld: preserved customized host file '{}'",
            path.display()
        );
    }
    Ok(())
}

struct AppliedFile {
    path: PathBuf,
    backup: Option<PathBuf>,
    replacement: Vec<u8>,
}

fn stage_file(file: &PlannedFile) -> Result<PathBuf, String> {
    let temp = unique_sibling(&file.path, "stage")?;
    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| format!("cannot stage host file '{}': {error}", file.path.display()))?;
    if let Err(error) = staged
        .write_all(&file.replacement)
        .and_then(|()| staged.sync_all())
    {
        drop(staged);
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "cannot write staged host file '{}': {error}",
            file.path.display()
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(&file.path) {
        let _ = staged.set_permissions(metadata.permissions());
    }
    Ok(temp)
}

fn unique_sibling(path: &Path, kind: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("'{}' has no parent directory", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("host-file");
    loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.langmeld-{kind}-{}-{id}",
            std::process::id()
        ));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => continue,
            Err(error) => return Err(format!("cannot inspect '{}': {error}", candidate.display())),
        }
    }
}

fn remove_staged(staged: &[(&PlannedFile, PathBuf)]) {
    for (_, path) in staged {
        let _ = fs::remove_file(path);
    }
}

fn rollback_error(error: String, applied: &[AppliedFile]) -> String {
    match rollback(applied) {
        Ok(()) => error,
        Err(rollback) => format!("{error}; rollback was incomplete: {rollback}"),
    }
}

fn rollback(applied: &[AppliedFile]) -> Result<(), String> {
    let mut errors = Vec::new();
    for file in applied.iter().rev() {
        match fs::read(&file.path) {
            Ok(contents) if contents == file.replacement => {
                if let Err(error) = fs::remove_file(&file.path) {
                    errors.push(format!("cannot remove '{}' ({error})", file.path.display()));
                    continue;
                }
            }
            Ok(_) => {
                errors.push(format!("'{}' changed during rollback", file.path.display()));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                errors.push(format!(
                    "cannot inspect '{}' ({error})",
                    file.path.display()
                ));
                continue;
            }
        }
        if let Some(backup) = &file.backup
            && let Err(error) = fs::rename(backup, &file.path)
        {
            errors.push(format!(
                "cannot restore '{}' from '{}' ({error})",
                file.path.display(),
                backup.display()
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::{HostInstallPlan, PlannedFile, apply_host_install_with_failure};
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn failed_install_restores_replaced_files_and_removes_created_files() {
        let root = std::env::temp_dir().join(format!(
            "langmeld-host-install-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let host = root.join("src/host");
        fs::create_dir_all(&host).expect("host test directory creates");
        let first = host.join("analysis.rs");
        let second = host.join("parser.rs");
        fs::write(&first, "old analysis").expect("old content writes");
        let plan = HostInstallPlan {
            host_directory: host,
            files: vec![
                PlannedFile {
                    path: first.clone(),
                    original: Some(b"old analysis".to_vec()),
                    replacement: b"new analysis".to_vec(),
                },
                PlannedFile {
                    path: second.clone(),
                    original: None,
                    replacement: b"new parser".to_vec(),
                },
            ],
            preserved: Vec::new(),
        };
        assert!(apply_host_install_with_failure(plan, Some(1)).is_err());
        assert_eq!(
            fs::read(&first).expect("original file restored"),
            b"old analysis"
        );
        assert!(!second.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generated_module_additions_preserve_existing_declarations() {
        let existing = "pub mod parser;\nmod local;\n";
        let generated = "pub mod parser;\npub mod analysis;\npub mod runtime;\n";
        assert_eq!(
            super::merge_module_declarations(existing, generated),
            "pub mod parser;\nmod local;\npub mod analysis;\npub mod runtime;\n"
        );
    }
}
