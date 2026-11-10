use langmeld_core::SourceFile;
use langmeld_language::{
    DefinitionResolver, DefinitionSource, parse, parse_language_definition_project,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Component, Path, PathBuf};

const PROGRAM: &str = "program { let title = \"Hello\"; draw title; }";

struct ProjectResolver {
    root: PathBuf,
    sources: RefCell<Vec<SourceFile>>,
}

impl ProjectResolver {
    fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("cannot resolve language project root: {error}"))?;
        Ok(Self {
            root,
            sources: RefCell::new(Vec::new()),
        })
    }

    fn sources(&self) -> Vec<SourceFile> {
        self.sources.borrow().clone()
    }
}

impl DefinitionResolver for ProjectResolver {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let from_path = Path::new(from_key);
        let import_path = Path::new(import);
        if !is_normal_relative_path(from_path) || !is_relative_path(import_path) {
            return Err("imports must use relative paths".into());
        }

        let containing_file = self.root.join(from_path).canonicalize().map_err(|error| {
            format!("cannot resolve importing definition `{from_key}`: {error}")
        })?;
        if !containing_file.starts_with(&self.root) {
            return Err(format!(
                "definition `{from_key}` is outside the project root"
            ));
        }
        let path = containing_file
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(import_path);
        let path = path
            .canonicalize()
            .map_err(|error| format!("cannot resolve import `{import}`: {error}"))?;
        if !path.starts_with(&self.root) {
            return Err(format!(
                "import `{import}` resolves outside the project root"
            ));
        }
        let key = path
            .strip_prefix(&self.root)
            .expect("the resolved import was checked to be inside the project root")
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read import `{key}`: {error}"))?;
        let source = SourceFile::new(key.clone(), text);
        self.sources.borrow_mut().push(source.clone());

        Ok(DefinitionSource::new(key, source))
    }
}

fn is_normal_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| !matches!(component, Component::RootDir | Component::Prefix(_)))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root_key = "language/toy.lm";
    let root_text = fs::read_to_string(project_root.join(root_key))
        .map_err(|error| format!("cannot read `{root_key}`: {error}"))?;
    let root_source = SourceFile::new(root_key, root_text);
    let resolver = ProjectResolver::new(project_root)?;
    let definition = parse_language_definition_project(
        DefinitionSource::new(root_key, root_source.clone()),
        &resolver,
    );
    if !definition.is_valid() {
        let mut sources = vec![root_source];
        sources.extend(resolver.sources());
        return Err(format_diagnostics(&sources, &definition.diagnostics));
    }
    let language = definition
        .language
        .ok_or_else(|| "definition did not produce a language".to_owned())?;

    let program_source = SourceFile::new("main.widget", PROGRAM);
    let parsed = parse(&program_source, &language);
    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics(&[program_source], &parsed.diagnostics));
    }

    println!("Language: {}", language.name());
    println!("Root construct: {}", parsed.constructs[0].name);
    for construct in &parsed.constructs[0].children {
        println!("  {} {:?}", construct.name, construct.fields);
    }
    Ok(())
}

fn format_diagnostics(sources: &[SourceFile], diagnostics: &[langmeld_core::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let location = diagnostic.location.as_ref();
            let source = location.and_then(|location| {
                sources
                    .iter()
                    .find(|source| source.name == location.source_name)
            });
            let (line, column) = match (source, location) {
                (Some(source), Some(location)) => source.line_column(location.span.start),
                _ => (1, 1),
            };
            let source_name = location
                .map(|location| location.source_name.as_str())
                .unwrap_or("<unknown>");
            format!(
                "{}:{line}:{column}: {}: {}",
                source_name, diagnostic.code, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
