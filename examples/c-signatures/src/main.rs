use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use clap::{Args, Parser, Subcommand};
use langmeld::{
    core::{Severity, SourceFile},
    language::{
        BackendDescriptor, BackendOptions, CompilationArtifact, Program, ProjectBuildConfig,
        ProjectSnapshot, load_source_files,
    },
};
use langmeld_lsp::LanguageServerConfig;

mod configure;

#[derive(Debug, Parser)]
#[command(
    about = "Check, build, configure, or run this language project.",
    subcommand_required = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse source files and run generic workspace analysis.
    Check(InputArgs),
    /// Compile source files with a backend registered by this project.
    Build(BuildArgs),
    /// Configure defaults for a registered backend in langmeld.json.
    Configure(configure::ConfigureArgs),
    /// Execute source files through the language runtime in src/lib.rs.
    Run(InputArgs),
    /// Serve the language through the host-configured Language Server Protocol.
    Lsp,
}

#[derive(Clone, Debug, Default, Args)]
#[command(group(clap::ArgGroup::new("input").multiple(false)))]
struct InputArgs {
    /// Load language and sources from langmeld.json.
    #[arg(long, group = "input", conflicts_with = "sources")]
    project: bool,
    /// Source files to process. Omit with --project.
    #[arg(value_name = "SOURCE", required_unless_present = "project")]
    sources: Vec<PathBuf>,
}

#[derive(Debug, Args)]
struct BuildArgs {
    #[command(flatten)]
    input: InputArgs,
    /// Registered backend ID. Defaults to the project choice or sole backend.
    #[arg(long)]
    backend: Option<String>,
    /// Backend output format. Defaults to the project choice or sole format.
    #[arg(long)]
    format: Option<String>,
    /// Backend-specific target such as an architecture.
    #[arg(long)]
    target: Option<String>,
    /// Request generated-to-source mappings.
    #[arg(long, conflicts_with = "no_source_maps")]
    source_maps: bool,
    /// Disable source mappings even when enabled in project defaults.
    #[arg(long)]
    no_source_maps: bool,
    /// Override a backend setting (repeat as --set KEY=VALUE).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    settings: Vec<String>,
    /// Output file; relative paths are resolved from the current directory.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

struct Prepared {
    project: Option<ProjectSnapshot>,
    language: langmeld::core::ValidatedLanguageSpec,
    sources: Vec<SourceFile>,
    programs: Vec<Program>,
    analysis: langmeld::language::WorkspaceAnalysis,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check(input) => process(input, true),
        Command::Run(input) => process(input, false),
        Command::Build(args) => build(args),
        Command::Configure(args) => configure::configure(args).map_err(|error| (2, error)),
        Command::Lsp => serve_lsp(),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::ExitCode::from(code)
        }
    }
}

fn serve_lsp() -> Result<(), (u8, String)> {
    let parser = csignatures_language::parser().map_err(|errors| {
        (
            2,
            errors
                .into_iter()
                .map(|error| format!("{}: {}", error.code, error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    langmeld_lsp::serve_configured(
        io::stdin().lock(),
        io::stdout().lock(),
        LanguageServerConfig::new(parser),
    )
    .map(|_| ())
    .map_err(|error| (2, error))
}

fn process(input: InputArgs, check_only: bool) -> Result<(), (u8, String)> {
    let prepared = prepare(input)?;
    if check_only {
        println!("checked {} source file(s)", prepared.sources.len());
        return Ok(());
    }
    csignatures_language::run_sources(&csignatures_language::RuntimeContext {
        language: &prepared.language,
        sources: &prepared.sources,
        programs: &prepared.programs,
        analysis: &prepared.analysis,
    })
    .map_err(|error| (2, error))
}

fn prepare(input: InputArgs) -> Result<Prepared, (u8, String)> {
    let (project, language, sources) = if input.project {
        let project = csignatures_language::load_project().map_err(|error| (2, error))?;
        (Some(project.clone()), project.language, project.sources)
    } else {
        let language = csignatures_language::load_language().map_err(|errors| {
            (
                2,
                errors
                    .into_iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })?;
        let sources = load_source_files(&input.sources).map_err(|error| (2, error))?;
        (None, language, sources)
    };
    let parser = csignatures_language::parser_for(language.clone()).map_err(|errors| {
        (
            2,
            errors
                .into_iter()
                .map(|error| format!("{}: {}", error.code, error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    let parsed: Vec<_> = sources.iter().map(|source| parser.parse(source)).collect();
    let programs: Vec<_> = parsed.iter().map(|result| result.program.clone()).collect();
    let program_refs: Vec<_> = programs.iter().collect();
    let analysis = csignatures_language::analyze_sources(&language, &sources, &program_refs)
        .map_err(|error| (2, error))?;
    let mut diagnostics: Vec<_> = parsed
        .into_iter()
        .map(|result| result.diagnostics)
        .collect();
    for item in &analysis.diagnostics {
        diagnostics[item.source_index].push(item.diagnostic.clone());
    }
    if report_diagnostics(&sources, diagnostics) {
        return Err((1, "source contains errors".to_owned()));
    }
    Ok(Prepared {
        project,
        language,
        sources,
        programs,
        analysis,
    })
}

fn report_diagnostics(
    sources: &[SourceFile],
    diagnostics: Vec<Vec<langmeld::core::Diagnostic>>,
) -> bool {
    let mut has_errors = false;
    for (source, findings) in sources.iter().zip(diagnostics) {
        if findings.is_empty() {
            println!("{}: no issues found", source.name);
            continue;
        }
        for diagnostic in findings {
            has_errors |= diagnostic.severity == Severity::Error;
            let (line, column) = diagnostic
                .span
                .map(|span| source.line_column(span.start))
                .unwrap_or((1, 1));
            println!(
                "{}:{line}:{column}: {:?}[{}]: {}",
                source.name, diagnostic.severity, diagnostic.code, diagnostic.message
            );
        }
    }
    has_errors
}

fn build(args: BuildArgs) -> Result<(), (u8, String)> {
    let prepared = prepare(args.input)?;
    let registry = csignatures_language::host::backend::BackendRegistry::from_project()
        .map_err(|error| (2, error.to_string()))?;
    let descriptors: Vec<_> = registry.descriptors().collect();
    if descriptors.is_empty() {
        return Err((
            2,
            "no backends are registered; add one to src/host/backend.rs before building".to_owned(),
        ));
    }
    let configured = prepared
        .project
        .as_ref()
        .and_then(|project| project.manifest.build.as_ref());
    let backend_id = args
        .backend
        .as_deref()
        .or_else(|| configured.map(|config| config.backend.as_str()))
        .map(str::to_owned)
        .or_else(|| (descriptors.len() == 1).then(|| descriptors[0].id.clone()))
        .ok_or_else(|| {
            let available = descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (
                2,
                format!("select a backend with --backend; available backends: {available}"),
            )
        })?;
    let descriptor = descriptors
        .iter()
        .copied()
        .find(|descriptor| descriptor.id == backend_id)
        .ok_or_else(|| {
            let available = descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (
                2,
                format!(
                    "backend `{backend_id}` is not registered; available backends: {available}"
                ),
            )
        })?;
    let format = select_format(args.format.as_deref(), configured, descriptor)?;
    let mut settings = configured
        .map(|config| config.settings.clone())
        .unwrap_or_default();
    let mut cli_setting_ids = BTreeSet::new();
    for value in args.settings {
        let (key, value) = value
            .split_once('=')
            .ok_or_else(|| (2, format!("invalid setting `{value}`; expected KEY=VALUE")))?;
        let key = key.trim();
        if key.is_empty() {
            return Err((2, "backend setting key cannot be empty".to_owned()));
        }
        if !cli_setting_ids.insert(key.to_owned()) {
            return Err((
                2,
                format!("backend setting `{key}` was supplied more than once"),
            ));
        }
        settings.insert(key.to_owned(), value.to_owned());
    }
    let source_maps = if args.source_maps {
        true
    } else if args.no_source_maps {
        false
    } else {
        configured
            .and_then(|config| config.source_maps)
            .unwrap_or(false)
    };
    let options = BackendOptions {
        format: format.id.clone(),
        target: args
            .target
            .or_else(|| configured.and_then(|config| config.target.clone())),
        source_maps,
        settings,
    };
    let files: Vec<_> = prepared.sources.iter().zip(&prepared.programs).collect();
    let artifact = registry
        .compile(&backend_id, &files, BTreeMap::new(), &options)
        .map_err(|error| (2, error.to_string()))?;
    let output = output_path(args.output, &prepared, &format.extension)?;
    let source_map = source_maps.then(|| source_map_json(&artifact, &prepared.sources));
    write_outputs(&output, &artifact, source_map.as_deref(), &prepared)?;
    println!("wrote {} ({})", output.display(), artifact.media_type);
    Ok(())
}

fn select_format<'a>(
    override_format: Option<&str>,
    configured: Option<&ProjectBuildConfig>,
    descriptor: &'a BackendDescriptor,
) -> Result<&'a langmeld::language::BackendFormat, (u8, String)> {
    let selected =
        override_format.or_else(|| configured.and_then(|config| config.format.as_deref()));
    if let Some(selected) = selected {
        return descriptor
            .formats
            .iter()
            .find(|format| format.id == selected)
            .ok_or_else(|| {
                let available = descriptor
                    .formats
                    .iter()
                    .map(|format| format.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                (2, format!("format `{selected}` is not available from backend `{}`; available formats: {available}", descriptor.id))
            });
    }
    if descriptor.formats.len() == 1 {
        return Ok(&descriptor.formats[0]);
    }
    let available = descriptor
        .formats
        .iter()
        .map(|format| format.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err((
        2,
        format!("select a format with --format; available formats: {available}"),
    ))
}

fn output_path(
    requested: Option<PathBuf>,
    prepared: &Prepared,
    extension: &str,
) -> Result<PathBuf, (u8, String)> {
    let cwd = std::env::current_dir()
        .map_err(|error| (2, format!("cannot read current directory: {error}")))?;
    let path = if let Some(path) = requested {
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    } else {
        let name = prepared.language.spec().name.to_ascii_lowercase();
        cwd.join("target").join(format!("{name}.{extension}"))
    };
    Ok(normalize_path(&path))
}

fn write_outputs(
    output: &Path,
    artifact: &CompilationArtifact,
    source_map: Option<&[u8]>,
    prepared: &Prepared,
) -> Result<(), (u8, String)> {
    let mut destinations = vec![(output.to_path_buf(), artifact.bytes.as_slice())];
    let map_path = source_map.map(|_| {
        output.with_file_name(format!(
            "{}.map.json",
            output.file_name().unwrap_or_default().to_string_lossy()
        ))
    });
    if let (Some(path), Some(bytes)) = (&map_path, source_map) {
        destinations.push((path.clone(), bytes));
    }
    if destinations.iter().any(|(path, _)| path.is_dir()) {
        return Err((2, "output destination is a directory".to_owned()));
    }
    if prepared.project.is_some() {
        let manifest = normalize_path(Path::new("langmeld.json"));
        for (path, _) in &destinations {
            if *path == manifest {
                return Err((2, "output path collides with langmeld.json".to_owned()));
            }
        }
    }
    for (path, _) in &destinations {
        for source in &prepared.sources {
            if paths_equal(path, Path::new(&source.name)) {
                return Err((
                    2,
                    format!("output path collides with source `{}`", source.name),
                ));
            }
        }
    }
    for (path, _) in &destinations {
        if let Ok(metadata) = fs::symlink_metadata(path)
            && (metadata.file_type().is_symlink() || !metadata.file_type().is_file())
        {
            return Err((
                2,
                format!(
                    "refusing to replace non-regular output `{}`",
                    path.display()
                ),
            ));
        }
    }
    let mut staged = Vec::with_capacity(destinations.len());
    for (path, bytes) in &destinations {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            (
                2,
                format!(
                    "cannot create output directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
        let temp = match unique_sibling(path, "stage") {
            Ok(path) => path,
            Err(error) => {
                cleanup_staged(&staged);
                return Err(error);
            }
        };
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => file,
            Err(error) => {
                cleanup_staged(&staged);
                return Err((
                    2,
                    format!("cannot create staged output `{}`: {error}", temp.display()),
                ));
            }
        };
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&temp);
            cleanup_staged(&staged);
            return Err((
                2,
                format!("cannot write staged output `{}`: {error}", temp.display()),
            ));
        }
        staged.push((path.clone(), temp));
    }
    install_staged(staged)
}

fn install_staged(staged: Vec<(PathBuf, PathBuf)>) -> Result<(), (u8, String)> {
    let mut backups = Vec::new();
    for (target, _) in &staged {
        if target.exists() {
            let backup = match unique_sibling(target, "backup") {
                Ok(path) => path,
                Err(error) => {
                    restore_backups(&backups);
                    cleanup_staged(&staged);
                    return Err(error);
                }
            };
            if let Err(error) = fs::rename(target, &backup) {
                restore_backups(&backups);
                cleanup_staged(&staged);
                return Err((
                    2,
                    format!(
                        "cannot stage existing output `{}`: {error}",
                        target.display()
                    ),
                ));
            }
            backups.push((target.clone(), backup));
        }
    }
    let mut installed = Vec::new();
    for (target, temp) in &staged {
        if let Err(error) = fs::rename(temp, target) {
            for path in &installed {
                let _ = fs::remove_file(path);
            }
            restore_backups(&backups);
            cleanup_staged(&staged);
            return Err((
                2,
                format!("cannot install output `{}`: {error}", target.display()),
            ));
        }
        installed.push(target.clone());
    }
    for (_, backup) in backups {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn source_map_json(artifact: &CompilationArtifact, sources: &[SourceFile]) -> Vec<u8> {
    let mappings: Vec<_> = artifact
        .source_map
        .iter()
        .map(|entry| {
            serde_json::json!({
                "generated": { "start": entry.generated.start, "end": entry.generated.end },
                "source": { "start": entry.source.start, "end": entry.source.end },
                "source_index": entry.source_index,
            })
        })
        .collect();
    serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1,
        "artifact": artifact.format,
        "sources": sources.iter().map(|source| source.name.as_str()).collect::<Vec<_>>(),
        "mappings": mappings,
    }))
    .expect("source map JSON values are serializable")
}

fn unique_sibling(path: &Path, label: &str) -> Result<PathBuf, (u8, String)> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    for _ in 0..100 {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.langmeld-{label}-{}-{id}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err((
        2,
        format!("cannot allocate temporary path beside `{}`", path.display()),
    ))
}

fn restore_backups(backups: &[(PathBuf, PathBuf)]) {
    for (target, backup) in backups.iter().rev() {
        let _ = fs::rename(backup, target);
    }
}

fn cleanup_staged(staged: &[(PathBuf, PathBuf)]) {
    for (_, path) in staged {
        let _ = fs::remove_file(path);
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
        || (left.exists()
            && right.exists()
            && left.canonicalize().ok() == right.canonicalize().ok())
}

fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}
