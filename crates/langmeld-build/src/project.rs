use std::{env, fs, path::PathBuf};

use langmeld_core::{Diagnostic, Severity, SourceFile, format_diagnostics};
use langmeld_project::load_definition_project;

use crate::{BuildError, generate_bindings};

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

    let language = project
        .definition()
        .language
        .as_ref()
        .ok_or_else(|| BuildError::InvalidDefinition("no language was produced".to_owned()))?;
    let generated = generate_bindings(language)?;

    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("langmeld.toml").display()
    );
    for path in project.watched_paths() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let output_path = output_dir.join("langmeld_bindings.rs");
    fs::write(&output_path, generated).map_err(|source| BuildError::WriteOutput {
        path: output_path,
        source,
    })
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
}
