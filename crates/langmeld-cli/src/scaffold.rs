use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
};

use langmeld_core::{SourceFile, ValidatedLanguageSpec};
use langmeld_language::{
    ProjectManifest, generate_rust_language_spec, generate_tooling,
    parse_language_definition_modules,
};

use crate::{cli::AuthoringMode, generate::host::generate_host_scaffold};

const ROOT_DEFINITION: &str = include_str!("../templates/starter/language.langmeld");
const CONSTRUCTS: &str = include_str!("../templates/starter/constructs.langmeld");

#[derive(Debug)]
pub(crate) struct ProjectOptions {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) agents: bool,
    pub(crate) agent_skill: bool,
    pub(crate) langmeld_path: Option<PathBuf>,
    pub(crate) library_only: bool,
    pub(crate) wasm: bool,
    pub(crate) authoring: AuthoringMode,
}

pub(crate) fn create_project(options: ProjectOptions) -> Result<(), String> {
    if !valid_language_name(&options.name) {
        return Err("language name must start with an ASCII letter and contain only ASCII letters or digits".to_owned());
    }
    let slug = options.name.to_ascii_lowercase();
    let root = ROOT_DEFINITION
        .replace("@NAME@", &options.name)
        .replace("@SLUG@", &slug);
    let constructs = CONSTRUCTS.to_owned();
    let language = validate_starter_definition(&root, &constructs)?;
    let dependency = local_dependency(&options)?;
    let mut files = starter_files(&options, &slug, &dependency, &root, &constructs, &language)?;
    let mut generated = generate_tooling(&language)
        .map_err(|error| format!("cannot generate starter tooling: {error}"))?;
    let host_scaffold = generate_host_scaffold(&language);
    for (relative, contents) in &host_scaffold {
        files.insert(PathBuf::from("src").join(relative), contents.clone());
    }
    generated.extend(host_scaffold);
    for (relative, contents) in generated {
        let relative = PathBuf::from(".langmeld/generated").join(relative);
        ensure_safe_relative_path(&relative)?;
        files.insert(relative, contents);
    }
    if options.agents {
        files.insert(
            PathBuf::from("AGENTS.md"),
            include_str!("../templates/starter/AGENTS.md").to_owned(),
        );
    }
    if options.agent_skill {
        files.insert(
            PathBuf::from(".agents/skills/langmeld/SKILL.md"),
            include_str!("../templates/starter/langmeld-skill.md").to_owned(),
        );
    }
    let output = absolute_path(&options.path)?;
    write_project(&output, &files)?;
    println!(
        "\n{}  {} is ready at {}\n\n  Next: cd {} && cargo test\n",
        console::style("✓").green().bold(),
        console::style(&options.name).bold(),
        console::style(&output.display()).cyan(),
        output.display()
    );
    Ok(())
}

fn validate_starter_definition(
    root: &str,
    constructs: &str,
) -> Result<ValidatedLanguageSpec, String> {
    let root = SourceFile::new("language/language.langmeld", root);
    let modules = [SourceFile::new(
        "language/constructs/program.langmeld",
        constructs,
    )];
    parse_language_definition_modules(&root, &modules).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn starter_files(
    options: &ProjectOptions,
    slug: &str,
    dependency: &str,
    root: &str,
    constructs: &str,
    language: &ValidatedLanguageSpec,
) -> Result<BTreeMap<PathBuf, String>, String> {
    let mut files = BTreeMap::new();
    let extension = format!("{slug}.lmprog");
    let package = format!("{slug}-language");
    let crate_name = format!("{slug}_language");
    let cli_target = if options.library_only {
        String::new()
    } else {
        format!(
            "default-run = \"{slug}-host\"\n\n[[bin]]\nname = \"{slug}-host\"\npath = \"src/main.rs\"\n"
        )
    };
    let cli_dependency = if options.library_only {
        String::new()
    } else {
        "clap = { version = \"4.6.7\", features = [\"derive\"] }\ninquire = \"0.9.4\"\nserde_json = \"1\"\n".to_owned()
    };
    let wasm_library = if options.wasm {
        "[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n"
    } else {
        ""
    };
    let wasm_dependencies = if options.wasm {
        "serde = { version = \"1\", features = [\"derive\"], optional = true }\nserde_json = { version = \"1\", optional = true }\nwasm-bindgen = { version = \"=0.2.129\", optional = true }\n"
    } else {
        ""
    };
    let wasm_features = if options.wasm {
        "[features]\ndefault = []\nwasm = [\"dep:serde\", \"dep:serde_json\", \"dep:wasm-bindgen\"]\n"
    } else {
        ""
    };
    let wasm_module = if options.wasm {
        "\nmod wasm;\npub use wasm::LanguageHost;\n"
    } else {
        ""
    };
    let wasm_readme = if options.wasm {
        include_str!("../templates/starter/README-wasm.md").replace("@CRATE@", &crate_name)
    } else {
        String::new()
    };
    let mut add = |path: &str, template: &str| {
        files.insert(
            PathBuf::from(path),
            template
                .replace("@NAME@", &options.name)
                .replace("@SLUG@", slug)
                .replace("@PACKAGE@", &package)
                .replace("@CRATE@", &crate_name)
                .replace("@EXT@", &extension)
                .replace("@CLI_TARGET@", &cli_target)
                .replace("@CLI_DEPENDENCY@", &cli_dependency)
                .replace("@WASM_LIBRARY@", wasm_library)
                .replace("@WASM_DEPENDENCIES@", wasm_dependencies)
                .replace("@WASM_FEATURES@", wasm_features)
                .replace("@WASM_MODULE@", wasm_module)
                .replace("@WASM_README@", &wasm_readme)
                .replace("@LANGMELD_DEPENDENCY@", dependency)
                .replace("@BUILD_DEPENDENCY@", dependency),
        );
    };
    add(
        "Cargo.toml",
        include_str!("../templates/starter/Cargo.toml.in"),
    );
    add(".gitignore", include_str!("../templates/starter/gitignore"));
    add("README.md", include_str!("../templates/starter/README.md"));
    let build_template = match options.authoring {
        AuthoringMode::Declarative => include_str!("../templates/starter/build-declarative.rs"),
        AuthoringMode::Rust | AuthoringMode::Combined => {
            include_str!("../templates/starter/build-rust.rs")
        }
    };
    add("build.rs", build_template);
    let library_template = match options.authoring {
        AuthoringMode::Declarative => include_str!("../templates/starter/lib.rs").to_owned(),
        AuthoringMode::Rust | AuthoringMode::Combined => {
            include_str!("../templates/starter/lib-rust.rs").to_owned()
        }
    };
    add("src/lib.rs", &library_template);
    add(
        "src/host/backend.rs",
        include_str!("../templates/starter/backend.rs"),
    );
    if !options.library_only {
        add("src/main.rs", include_str!("../templates/starter/main.rs"));
        add(
            "src/configure.rs",
            include_str!("../templates/starter/configure.rs"),
        );
    }
    add(
        "examples/hello.rs",
        include_str!("../templates/starter/hello.rs"),
    );
    add(
        "tests/language.rs",
        include_str!("../templates/starter/language_test.rs"),
    );
    let language_module = match options.authoring {
        AuthoringMode::Declarative => {
            include_str!("../templates/starter/language-mod-declarative.rs")
        }
        AuthoringMode::Rust | AuthoringMode::Combined => {
            include_str!("../templates/starter/language-mod.rs")
        }
    };
    files.insert(
        PathBuf::from("src/language/mod.rs"),
        language_module.to_owned(),
    );
    if matches!(
        options.authoring,
        AuthoringMode::Rust | AuthoringMode::Combined
    ) {
        files.insert(
            PathBuf::from("src/language/spec.rs"),
            generate_rust_language_spec(language),
        );
    }
    let mut manifest = match options.authoring {
        AuthoringMode::Declarative => ProjectManifest::new("language/language.langmeld"),
        AuthoringMode::Rust | AuthoringMode::Combined => ProjectManifest::new_rust_host(),
    };
    manifest.source_roots.push(PathBuf::from("examples"));
    if options.authoring == AuthoringMode::Combined {
        manifest
            .extensions
            .push(langmeld_language::ManifestExtension::Path(PathBuf::from(
                "language/extensions/starter.langmeld",
            )));
        files.insert(
            PathBuf::from("language/extensions/starter.langmeld"),
            extension_definition(&options.name),
        );
    }
    files.insert(PathBuf::from("langmeld.json"), manifest.to_json()?);
    if options.authoring == AuthoringMode::Declarative {
        files.insert(PathBuf::from("language/language.langmeld"), root.to_owned());
        files.insert(
            PathBuf::from("language/constructs/program.langmeld"),
            constructs.to_owned(),
        );
    }
    if options.wasm {
        files.insert(
            PathBuf::from("src/wasm.rs"),
            include_str!("../templates/starter/wasm.rs").to_owned(),
        );
    }
    let example_source = if options.authoring == AuthoringMode::Combined {
        format!(
            "program {{\n  print \"Hello from {}!\"\n  note \"A note from {}.\"\n}}\n",
            options.name, options.name
        )
    } else {
        format!("program {{\n  print \"Hello from {}!\"\n}}\n", options.name)
    };
    files.insert(
        PathBuf::from(format!("examples/hello.{extension}")),
        example_source,
    );
    Ok(files)
}

fn extension_definition(name: &str) -> String {
    format!(
        "language {name} {{\n  version: \"0.1.0\"\n  root: program\n  construct note {{\n    implements: program_statement\n    syntax {{ default: \"note {{text}}\" }}\n    fields {{ text: text required }}\n  }}\n}}\n",
    )
}

fn local_dependency(options: &ProjectOptions) -> Result<String, String> {
    let default_checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checkout = options
        .langmeld_path
        .as_deref()
        .or_else(|| is_langmeld_checkout(&default_checkout).then_some(default_checkout.as_path()));
    let Some(checkout) = checkout else {
        return Ok(format!(
            "version = \"{}\", default-features = false",
            env!("CARGO_PKG_VERSION")
        ));
    };
    let checkout = checkout.canonicalize().map_err(|error| {
        format!(
            "cannot resolve Langmeld checkout '{}': {error}",
            checkout.display()
        )
    })?;
    let manifest = checkout.join("crates/langmeld/Cargo.toml");
    if !manifest.is_file() {
        return Err(format!(
            "'{}' is not a Langmeld checkout (missing crates/langmeld/Cargo.toml)",
            checkout.display()
        ));
    }
    let project = canonical_project_path(&absolute_path(&options.path)?)?;
    let facade_path = checkout.join("crates/langmeld");
    let relative =
        relative_path(&project, &facade_path).unwrap_or_else(|| facade_path.to_path_buf());
    let path = serde_json::to_string(&relative.to_string_lossy())
        .map_err(|error| format!("cannot encode Langmeld path: {error}"))?;
    Ok(format!(
        "version = \"{}\", path = {path}, default-features = false",
        env!("CARGO_PKG_VERSION")
    ))
}

fn is_langmeld_checkout(path: &Path) -> bool {
    path.join("crates/langmeld/Cargo.toml").is_file()
        && path.join("crates/langmeld-core/Cargo.toml").is_file()
        && path.join("crates/langmeld-language/Cargo.toml").is_file()
        && path.join("crates/langmeld-expression/Cargo.toml").is_file()
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| format!("cannot read current directory: {error}"))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normalized.file_name().is_some() => {
                normalized.pop();
            }
            Component::ParentDir => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    if normalized.file_name().is_none() {
        return Err("project path must name a directory".to_owned());
    }
    Ok(normalized)
}

fn canonical_project_path(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| "project path must name a directory".to_owned())?;
    let mut existing_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut missing_components = Vec::new();
    while !existing_parent.exists() {
        let component = existing_parent
            .file_name()
            .ok_or_else(|| format!("cannot resolve project parent '{}'", path.display()))?;
        missing_components.push(component.to_os_string());
        existing_parent = existing_parent
            .parent()
            .ok_or_else(|| format!("cannot resolve project parent '{}'", path.display()))?;
    }
    let mut canonical = existing_parent.canonicalize().map_err(|error| {
        format!(
            "cannot resolve project parent '{}': {error}",
            existing_parent.display()
        )
    })?;
    for component in missing_components.iter().rev() {
        canonical.push(component);
    }
    canonical.push(file_name);
    Ok(canonical)
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return None;
    }
    let mut path = PathBuf::new();
    for component in &from[common..] {
        if !matches!(component, Component::Normal(_)) {
            return None;
        }
        path.push("..");
    }
    for component in &to[common..] {
        path.push(component.as_os_str());
    }
    Some(path)
}

fn ensure_safe_relative_path(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "unsafe generated project path `{}`",
            path.display()
        ));
    }
    Ok(())
}

fn write_project(output: &Path, files: &BTreeMap<PathBuf, String>) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(output) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "project path '{}' is not a plain directory",
                output.display()
            ));
        }
        if fs::read_dir(output)
            .map_err(|error| format!("cannot inspect '{}': {error}", output.display()))?
            .next()
            .is_some()
        {
            return Err(format!(
                "refusing to create a project in non-empty directory '{}'",
                output.display()
            ));
        }
    }
    for path in files.keys() {
        ensure_safe_relative_path(path)?;
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create project parent '{}': {error}",
            parent.display()
        )
    })?;
    let stage = parent.join(format!(".langmeld-init-{}", std::process::id()));
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
    let target_existed = output.exists();
    if target_existed {
        fs::remove_dir(output).map_err(|error| {
            format!(
                "cannot replace empty directory '{}': {error}",
                output.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&stage, output) {
        if target_existed {
            let _ = fs::create_dir(output);
        }
        return Err(format!(
            "cannot install project at '{}': {error}",
            output.display()
        ));
    }
    guard.committed = true;
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

pub(crate) fn valid_language_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

#[cfg(all(test, unix))]
mod tests {
    use super::canonical_project_path;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn canonical_project_path_resolves_a_symlinked_existing_parent() {
        use std::os::unix::fs::symlink;

        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "langmeld-canonical-project-path-{}-{id}",
            std::process::id()
        ));
        let target = directory.join("target");
        let alias = directory.join("alias");
        fs::create_dir_all(&target).expect("target directory is created");
        symlink(&target, &alias).expect("directory alias is created");

        let requested = alias.join("nested/project");
        let canonical = canonical_project_path(&requested)
            .expect("nonexistent project path resolves through its canonical parent");
        assert_eq!(
            canonical,
            target
                .canonicalize()
                .expect("target directory canonicalizes")
                .join("nested/project")
        );

        fs::remove_dir_all(directory).expect("test directory is removed");
    }
}
