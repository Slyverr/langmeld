use std::{
    env, fs,
    path::{Path, PathBuf},
};

use langmeld_core::{Diagnostic, Severity, SourceFile, format_diagnostics};
use langmeld_project::load_definition_project;

use crate::BuildError;

#[derive(Clone, Copy)]
enum ProjectOutputs {
    TypedBindings,
    EmbeddedSources,
    TypedBindingsAndEmbeddedSources,
}

/// Loads `langmeld.toml` from `CARGO_MANIFEST_DIR` and writes generated Rust
/// bindings to Cargo's `OUT_DIR`.
///
/// Add this to a build script, then include `langmeld_bindings.rs` from the
/// `OUT_DIR` in the library or binary. Cargo is told to rerun the build script
/// when the manifest or any imported `.langmeld` source changes.
///
/// # Errors
///
/// Returns an error if the project definition is invalid, unsupported by the
/// typed binding generator, or cannot be read or written.
pub fn generate_project() -> Result<(), BuildError> {
    generate_project_outputs(ProjectOutputs::TypedBindings)
}

/// Loads `langmeld.toml` and writes the root and imported definition sources
/// as Rust `include_str!` entries in Cargo's `OUT_DIR`.
///
/// The generated `langmeld_sources.rs` defines `LANGMELD_SOURCES` as a
/// project-relative path and `include_str!` pair for the root definition and
/// every imported definition. This lets hosts load definitions without
/// accessing the project filesystem at runtime, including in WebAssembly.
/// Include it at module scope with
/// `include!(concat!(env!("OUT_DIR"), "/langmeld_sources.rs"));`.
///
/// # Errors
///
/// Returns an error if the project definition is invalid or cannot be read or
/// written.
pub fn generate_embedded_sources() -> Result<(), BuildError> {
    generate_project_outputs(ProjectOutputs::EmbeddedSources)
}

/// Loads `langmeld.toml` and writes both typed bindings and embedded sources
/// to Cargo's `OUT_DIR` in one project load.
///
/// The generated files are `langmeld_bindings.rs` and `langmeld_sources.rs`.
/// Include each at module scope from the Rust host.
///
/// # Errors
///
/// Returns an error if the project definition is invalid, unsupported by the
/// typed binding generator, or cannot be read or written.
pub fn generate_project_with_embedded_sources() -> Result<(), BuildError> {
    generate_project_outputs(ProjectOutputs::TypedBindingsAndEmbeddedSources)
}

fn generate_project_outputs(outputs: ProjectOutputs) -> Result<(), BuildError> {
    let project_root = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or(BuildError::MissingEnvironment("CARGO_MANIFEST_DIR"))?;
    let output_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or(BuildError::MissingEnvironment("OUT_DIR"))?;

    let project = load_definition_project(&project_root)?;
    if !project.is_valid() {
        return Err(BuildError::InvalidDefinition(format_diagnostics(
            project.sources(),
            &project.definition().diagnostics,
        )));
    }

    for warning in cargo_warnings(project.sources(), &project.definition().diagnostics) {
        println!("cargo:warning={warning}");
    }

    let generated_bindings = if !matches!(outputs, ProjectOutputs::EmbeddedSources) {
        let language =
            project.definition().language.as_ref().ok_or_else(|| {
                BuildError::InvalidDefinition("no language was produced".to_owned())
            })?;
        Some(crate::generate_bindings(language)?)
    } else {
        None
    };

    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("langmeld.toml").display()
    );
    for path in project.watched_paths() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    if let Some(generated) = generated_bindings {
        write_output(&output_dir, "langmeld_bindings.rs", generated)?;
    }

    if !matches!(outputs, ProjectOutputs::TypedBindings) {
        let generated = embedded_sources(&project.manifest().definition, project.sources());
        write_output(&output_dir, "langmeld_sources.rs", generated)?;
    }

    Ok(())
}

fn write_output(output_dir: &Path, file_name: &str, contents: String) -> Result<(), BuildError> {
    let output_path = output_dir.join(file_name);
    fs::write(&output_path, contents).map_err(|source| BuildError::WriteOutput {
        path: output_path,
        source,
    })
}

fn embedded_sources(root_key: &str, sources: &[SourceFile]) -> String {
    let mut generated = format!(
        "/// Project-relative key of the root Langmeld definition.\n\
         pub const LANGMELD_ROOT_KEY: &str = {root_key:?};\n\n\
         /// Root and imported Langmeld project sources embedded at compile time.\n\
         pub const LANGMELD_SOURCES: &[(&str, &str)] = &[\n"
    );

    for source in sources {
        let source_name = format!("{:?}", source.name());
        generated.push_str("    (");
        generated.push_str(&source_name);
        generated.push_str(", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/\", ");
        generated.push_str(&source_name);
        generated.push_str("))),\n");
    }

    generated.push_str("];\n");
    generated
}

fn cargo_warnings(sources: &[SourceFile], diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .map(|diagnostic| format_diagnostics(sources, std::slice::from_ref(diagnostic)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use langmeld_core::Span;

    #[test]
    fn formats_only_warning_diagnostics_as_cargo_findings() {
        let source = SourceFile::new("language/main.langmeld", "language \"Example\" {}");
        let diagnostics = [
            Diagnostic::warning("TEST-W001", "example warning", &source, Span::new(0, 8)),
            Diagnostic::info(
                "LM-DEF-I001",
                "additional context",
                &source,
                Span::new(0, 8),
            ),
        ];

        assert_eq!(
            cargo_warnings(&[source], &diagnostics),
            ["language/main.langmeld:1:1: warning[TEST-W001]: example warning"]
        );
    }

    #[test]
    fn embeds_manifest_root_key_and_stable_relative_source_names() {
        let sources = [
            SourceFile::new("definitions/root.langmeld", "language \"Example\" {}"),
            SourceFile::new(
                "definitions/constructs/print.langmeld",
                "construct \"print\" {}",
            ),
        ];

        assert_eq!(
            embedded_sources("definitions/root.langmeld", &sources),
            concat!(
                "/// Project-relative key of the root Langmeld definition.\n",
                "pub const LANGMELD_ROOT_KEY: &str = \"definitions/root.langmeld\";\n",
                "\n",
                "/// Root and imported Langmeld project sources embedded at compile time.\n",
                "pub const LANGMELD_SOURCES: &[(&str, &str)] = &[\n",
                "    (\"definitions/root.langmeld\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/\", \"definitions/root.langmeld\"))),\n",
                "    (\"definitions/constructs/print.langmeld\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/\", \"definitions/constructs/print.langmeld\"))),\n",
                "];\n",
            )
        );
    }
}
