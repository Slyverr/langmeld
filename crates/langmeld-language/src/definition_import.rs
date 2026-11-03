//! File discovery and safe import resolution for readable `.lm` projects.

use langmeld_core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::{
    AstNode, DefinitionLimits, DefinitionProjectDiagnostic, PreparedLanguage,
    definition_tree::{
        convert_language_module_specs, error, limit_diagnostics, parse_definition_tree, text,
    },
};

const MAX_DIRECTORY_ENTRIES: usize = 100_000;
const MAX_DIRECTORY_DEPTH: usize = 128;

type ProjectDiagnostics = Vec<DefinitionProjectDiagnostic>;

pub(crate) fn parse_project(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_project_with_overlays(path, prepared, limits, &BTreeMap::new())
}

pub(crate) fn parse_project_with_overlays(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_project_with_overlays_and_diagnostics(path, prepared, limits, overlays)
        .map_err(diagnostics_without_source)
}

pub(crate) fn parse_project_with_overlays_and_diagnostics(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<ValidatedLanguageSpec, Vec<DefinitionProjectDiagnostic>> {
    let (spec, source_len) = parse_project_spec_and_length(path, prepared, limits, overlays)?;
    let result: Result<ValidatedLanguageSpec, ProjectDiagnostics> =
        spec.validate().map_err(|errors| {
            errors
                .into_iter()
                .map(|issue| DefinitionProjectDiagnostic {
                    source_path: None,
                    source: None,
                    diagnostic: Diagnostic::error(
                        issue.code,
                        issue.message,
                        Span::new(0, source_len),
                    ),
                })
                .collect()
        });
    limit_project_diagnostics(result, limits.max_diagnostics, source_len)
}

pub(crate) fn project_source_paths(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<Vec<PathBuf>, Vec<Diagnostic>> {
    let project = load_project(path, prepared, limits, &BTreeMap::new())
        .map_err(diagnostics_without_source)?;
    Ok(project.paths)
}

pub(crate) fn parse_project_spec_with_sources(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<(langmeld_core::LanguageSpec, Vec<PathBuf>, usize), Vec<Diagnostic>> {
    let project = load_project(path, prepared, limits, &BTreeMap::new())
        .map_err(diagnostics_without_source)?;
    let source_len = project
        .modules
        .last()
        .map_or(0, |(_, source)| source.text.len());
    let spec = limit_diagnostics(
        convert_language_module_specs(
            project
                .modules
                .iter()
                .map(|(module, source)| (module, source)),
        ),
        limits.max_diagnostics,
        source_len,
    )?;
    Ok((spec, project.paths, project.total_bytes))
}

fn parse_project_spec_and_length(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<(langmeld_core::LanguageSpec, usize), Vec<DefinitionProjectDiagnostic>> {
    let project = load_project(path, prepared, limits, overlays)?;
    let source_len = project
        .modules
        .last()
        .map_or(0, |(_, source)| source.text.len());
    let spec = limit_diagnostics(
        convert_language_module_specs(
            project
                .modules
                .iter()
                .map(|(module, source)| (module, source)),
        ),
        limits.max_diagnostics,
        source_len,
    )
    .map_err(unlocated_diagnostics)?;
    Ok((spec, source_len))
}

struct LoadedProject {
    modules: Vec<(AstNode, SourceFile)>,
    paths: Vec<PathBuf>,
    total_bytes: usize,
}

struct LoadState<'a> {
    root_path: PathBuf,
    root_directory: PathBuf,
    prepared: &'a PreparedLanguage,
    overlays: BTreeMap<PathBuf, &'a str>,
    limits: DefinitionLimits,
    total_bytes: usize,
    visiting: Vec<PathBuf>,
    loaded: BTreeSet<PathBuf>,
    modules: Vec<(AstNode, SourceFile)>,
    paths: Vec<PathBuf>,
}

fn load_project(
    path: &Path,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<LoadedProject, ProjectDiagnostics> {
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return Err(file_error_at(
            path,
            "the readable definition loader accepts `.lm` files only",
        ));
    }
    let root_path = fs::canonicalize(path).map_err(|error| {
        file_error_at(
            path,
            format!(
                "cannot resolve language definition '{}': {error}",
                path.display()
            ),
        )
    })?;
    if root_path
        .extension()
        .is_none_or(|extension| extension != "lm")
    {
        return Err(file_error_at(
            &root_path,
            format!(
                "language definition '{}' must have a `.lm` extension",
                root_path.display()
            ),
        ));
    }
    let root_directory = root_path.parent().ok_or_else(|| {
        file_error_at(
            &root_path,
            format!(
                "language definition '{}' has no parent directory",
                root_path.display()
            ),
        )
    })?;
    if overlays.len() > limits.max_files {
        return Err(file_error(format!(
            "language definition overlays exceed the limit of {} files",
            limits.max_files
        )));
    }
    let mut overlay_bytes = 0usize;
    let mut canonical_overlays = BTreeMap::new();
    for (overlay_path, text) in overlays {
        overlay_bytes = overlay_bytes.saturating_add(text.len());
        if overlay_bytes > limits.max_source_bytes {
            return Err(file_error(format!(
                "language definition overlays exceed the total parsing limit of {} bytes",
                limits.max_source_bytes
            )));
        }
        let canonical_path = fs::canonicalize(overlay_path).map_err(|error| {
            file_error_at(
                overlay_path,
                format!(
                    "cannot resolve language definition overlay '{}': {error}",
                    overlay_path.display()
                ),
            )
        })?;
        if !canonical_path.starts_with(root_directory) {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "language definition overlay '{}' escapes the language directory",
                    overlay_path.display()
                ),
            ));
        }
        if canonical_path
            .extension()
            .is_none_or(|extension| extension != "lm")
        {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "language definition overlay '{}' must have a `.lm` extension",
                    canonical_path.display()
                ),
            ));
        }
        let metadata = fs::metadata(&canonical_path).map_err(|error| {
            file_error_at(
                &canonical_path,
                format!(
                    "cannot inspect language definition overlay '{}': {error}",
                    canonical_path.display()
                ),
            )
        })?;
        if !metadata.is_file() {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "language definition overlay '{}' must name a regular file",
                    canonical_path.display()
                ),
            ));
        }
        if canonical_overlays
            .insert(canonical_path.clone(), text.as_str())
            .is_some()
        {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "multiple language definition overlays resolve to '{}'",
                    canonical_path.display()
                ),
            ));
        }
    }
    let mut state = LoadState {
        root_path: root_path.clone(),
        root_directory: root_directory.to_path_buf(),
        prepared,
        overlays: canonical_overlays,
        limits,
        total_bytes: 0,
        visiting: Vec::new(),
        loaded: BTreeSet::new(),
        modules: Vec::new(),
        paths: Vec::new(),
    };
    load_module(&root_path, &mut state)?;
    Ok(LoadedProject {
        modules: state.modules,
        paths: state.paths,
        total_bytes: state.total_bytes,
    })
}

fn load_module(path: &Path, state: &mut LoadState<'_>) -> Result<(), ProjectDiagnostics> {
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        file_error_at(
            path,
            format!(
                "cannot resolve imported definition '{}': {error}",
                path.display()
            ),
        )
    })?;
    if !canonical_path.starts_with(&state.root_directory) {
        return Err(file_error_at(
            &canonical_path,
            format!(
                "imported definition '{}' escapes the language directory",
                path.display()
            ),
        ));
    }
    if let Some(cycle_start) = state
        .visiting
        .iter()
        .position(|visiting| visiting == &canonical_path)
    {
        let mut cycle = state.visiting[cycle_start..]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        cycle.push(canonical_path.display().to_string());
        return Err(file_error_at(
            &canonical_path,
            format!("language definition import cycle: {}", cycle.join(" -> ")),
        ));
    }
    if state.loaded.contains(&canonical_path) {
        return Ok(());
    }
    if state.loaded.len().saturating_add(state.visiting.len()) >= state.limits.max_files {
        return Err(file_error(format!(
            "language definition imports exceed the limit of {} files",
            state.limits.max_files
        )));
    }
    if canonical_path
        .extension()
        .is_none_or(|extension| extension != "lm")
    {
        return Err(file_error_at(
            &canonical_path,
            format!(
                "imported definition '{}' must be a `.lm` file",
                canonical_path.display()
            ),
        ));
    }

    let remaining_bytes = state
        .limits
        .max_source_bytes
        .saturating_sub(state.total_bytes);
    let source_text = if let Some(text) = state.overlays.get(&canonical_path) {
        if text.len() > remaining_bytes {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "language definition project exceeds its total parsing limit of {} bytes",
                    state.limits.max_source_bytes
                ),
            ));
        }
        (*text).to_owned()
    } else {
        let metadata = fs::metadata(&canonical_path).map_err(|error| {
            file_error_at(
                &canonical_path,
                format!(
                    "cannot inspect language definition '{}': {error}",
                    canonical_path.display()
                ),
            )
        })?;
        if !metadata.is_file() {
            return Err(file_error_at(
                &canonical_path,
                format!(
                    "language definition '{}' must be a regular file",
                    canonical_path.display()
                ),
            ));
        }
        read_source(
            &canonical_path,
            remaining_bytes,
            state.limits.max_source_bytes,
        )?
    };
    state.total_bytes = state.total_bytes.saturating_add(source_text.len());
    let source = SourceFile::new(canonical_path.display().to_string(), source_text);
    let module =
        parse_definition_tree(&source, state.prepared, state.limits).map_err(|diagnostics| {
            sourced_diagnostics_with_source(&canonical_path, &source, diagnostics)
        })?;
    check_conventional_section(&canonical_path, &state.root_directory, &module, &source).map_err(
        |diagnostics| sourced_diagnostics_with_source(&canonical_path, &source, diagnostics),
    )?;

    state.visiting.push(canonical_path.clone());
    let result = (|| {
        if canonical_path == state.root_path {
            let (types, constructs) =
                discover_conventional_modules(&state.root_directory, state.limits.max_files)
                    .map_err(file_error)?;
            for path in types {
                load_module(&path, state)?;
            }
            load_imports(&module, &canonical_path, state)?;
            for path in constructs {
                load_module(&path, state)?;
            }
        } else {
            load_imports(&module, &canonical_path, state)?;
        }
        state.loaded.insert(canonical_path.clone());
        state.modules.push((module, source));
        state.paths.push(canonical_path.clone());
        Ok(())
    })();
    state.visiting.pop();
    result
}

fn load_imports(
    module: &AstNode,
    canonical_path: &Path,
    state: &mut LoadState<'_>,
) -> Result<(), ProjectDiagnostics> {
    for import in module
        .children
        .iter()
        .filter(|declaration| declaration.construct == "import_decl")
    {
        let import_name = text(import, "path")
            .map_err(|diagnostics| sourced_diagnostics(canonical_path, diagnostics))?;
        let import_path = Path::new(&import_name);
        if import_path.is_absolute()
            || import_path
                .components()
                .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
        {
            return Err(sourced_diagnostics(
                canonical_path,
                vec![error(
                    import,
                    format!("definition import `{import_name}` must be a relative path"),
                )],
            ));
        }
        if import_path
            .extension()
            .is_none_or(|extension| extension != "lm")
        {
            return Err(sourced_diagnostics(
                canonical_path,
                vec![error(
                    import,
                    format!("definition import `{import_name}` must name a `.lm` file"),
                )],
            ));
        }
        let resolved = canonical_path
            .parent()
            .expect("canonical definition path has a parent")
            .join(import_path);
        load_module(&resolved, state)?;
    }
    Ok(())
}

fn check_conventional_section(
    path: &Path,
    root_directory: &Path,
    module: &AstNode,
    source: &SourceFile,
) -> Result<(), Vec<Diagnostic>> {
    let expected = path
        .strip_prefix(root_directory)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .and_then(|folder| match folder {
            "types" => Some("types"),
            "constructs" => Some("constructs"),
            _ => None,
        });
    if expected.is_some_and(|section| module.syntax_variant != section) {
        return Err(vec![Diagnostic::error(
            "LMDEF046",
            format!(
                "language definition '{}' uses the wrong section for its conventional directory",
                path.display()
            ),
            Span::new(0, source.text.len()),
        )]);
    }
    Ok(())
}

fn discover_conventional_modules(
    root_directory: &Path,
    max_files: usize,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let mut remaining_entries = MAX_DIRECTORY_ENTRIES;
    let mut remaining_files = max_files;
    let types = discover_directory(
        &root_directory.join("types"),
        &mut remaining_entries,
        &mut remaining_files,
    )?;
    let constructs = discover_directory(
        &root_directory.join("constructs"),
        &mut remaining_entries,
        &mut remaining_files,
    )?;
    Ok((types, constructs))
}

fn discover_directory(
    directory: &Path,
    remaining_entries: &mut usize,
    remaining_files: &mut usize,
) -> Result<Vec<PathBuf>, String> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "conventional language directory '{}' cannot be a symlink",
                directory.display()
            ));
        }
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(format!(
                "conventional language path '{}' must be a directory",
                directory.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "cannot inspect conventional language directory '{}': {error}",
                directory.display()
            ));
        }
    }

    fn visit(
        directory: &Path,
        depth: usize,
        remaining_entries: &mut usize,
        remaining_files: &mut usize,
        files: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        if depth > MAX_DIRECTORY_DEPTH {
            return Err(format!(
                "conventional language directories exceed nesting limit {MAX_DIRECTORY_DEPTH}"
            ));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("cannot read '{}': {error}", directory.display()))?
        {
            if entries.len() >= *remaining_entries {
                return Err("conventional language directories exceed the entry limit".to_owned());
            }
            entries.push(
                entry
                    .map_err(|error| format!("cannot read '{}': {error}", directory.display()))?
                    .path(),
            );
        }
        *remaining_entries -= entries.len();
        entries.sort();
        for path in entries {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "conventional language entry '{}' cannot be a symlink",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                visit(&path, depth + 1, remaining_entries, remaining_files, files)?;
            } else if metadata.is_file()
                && path.extension().is_some_and(|extension| extension == "lm")
            {
                if *remaining_files == 0 {
                    return Err("conventional language files exceed the file limit".to_owned());
                }
                *remaining_files -= 1;
                files.push(path);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(directory, 0, remaining_entries, remaining_files, &mut files)?;
    Ok(files)
}

fn read_source(
    path: &Path,
    remaining_bytes: usize,
    project_limit: usize,
) -> Result<String, ProjectDiagnostics> {
    let file = File::open(path).map_err(|error| {
        file_error_at(
            path,
            format!(
                "cannot read imported definition '{}': {error}",
                path.display()
            ),
        )
    })?;
    let mut limited = file.take(remaining_bytes.saturating_add(1) as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|error| {
        file_error_at(
            path,
            format!(
                "cannot read imported definition '{}': {error}",
                path.display()
            ),
        )
    })?;
    if bytes.len() > remaining_bytes {
        return Err(file_error_at(
            path,
            format!("definition project exceeds its total parsing limit of {project_limit} bytes"),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        file_error_at(
            path,
            format!(
                "language definition '{}' is not UTF-8: {error}",
                path.display()
            ),
        )
    })
}

fn file_error(message: impl Into<String>) -> ProjectDiagnostics {
    unlocated_diagnostics(vec![Diagnostic::spec_error("LMBOOT001", message)])
}

fn file_error_at(path: &Path, message: impl Into<String>) -> ProjectDiagnostics {
    sourced_diagnostics(path, vec![Diagnostic::spec_error("LMBOOT001", message)])
}

fn sourced_diagnostics(path: &Path, diagnostics: Vec<Diagnostic>) -> ProjectDiagnostics {
    diagnostics
        .into_iter()
        .map(|diagnostic| DefinitionProjectDiagnostic {
            source_path: Some(path.to_path_buf()),
            source: None,
            diagnostic,
        })
        .collect()
}

fn sourced_diagnostics_with_source(
    path: &Path,
    source: &SourceFile,
    diagnostics: Vec<Diagnostic>,
) -> ProjectDiagnostics {
    let source = std::sync::Arc::new(source.clone());
    diagnostics
        .into_iter()
        .map(|diagnostic| DefinitionProjectDiagnostic {
            source_path: Some(path.to_path_buf()),
            source: Some(source.clone()),
            diagnostic,
        })
        .collect()
}

fn unlocated_diagnostics(diagnostics: Vec<Diagnostic>) -> ProjectDiagnostics {
    diagnostics
        .into_iter()
        .map(|diagnostic| DefinitionProjectDiagnostic {
            source_path: None,
            source: None,
            diagnostic,
        })
        .collect()
}

fn diagnostics_without_source(diagnostics: ProjectDiagnostics) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.diagnostic)
        .collect()
}

fn limit_project_diagnostics<T>(
    result: Result<T, ProjectDiagnostics>,
    max_diagnostics: usize,
    source_len: usize,
) -> Result<T, ProjectDiagnostics> {
    let Err(mut diagnostics) = result else {
        return result;
    };
    let limit = max_diagnostics.max(1);
    if diagnostics.len() > limit {
        diagnostics.truncate(limit.saturating_sub(1));
        diagnostics.push(DefinitionProjectDiagnostic {
            source_path: None,
            source: None,
            diagnostic: Diagnostic::error(
                "LMDEF102",
                format!("definition diagnostics exceed the limit of {limit}"),
                Span::new(0, source_len),
            ),
        });
    }
    Err(diagnostics)
}
