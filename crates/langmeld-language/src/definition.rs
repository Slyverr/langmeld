//! Resource-bounded parsing APIs for readable language definitions.

use langmeld_core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec};
use std::{collections::BTreeMap, path::Path, path::PathBuf};

use crate::{definition_meta::langmeld_definition_parser, definition_tree};

/// Input bounds for readable language-definition loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefinitionLimits {
    /// Maximum UTF-8 definition size in bytes.
    pub max_source_bytes: usize,
    /// Maximum lexer tokens, including the end-of-file sentinel.
    pub max_tokens: usize,
    /// Maximum distinct definition files in an in-memory or file-based project.
    pub max_files: usize,
    /// Maximum diagnostics returned for malformed definitions.
    pub max_diagnostics: usize,
}

/// One definition-project diagnostic with its originating file when known.
///
/// Cross-module validation can report project-level issues without a reliable
/// source identity. Those findings have no `source_path`; consumers must not
/// assign their diagnostic span to an arbitrary project file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionProjectDiagnostic {
    /// Canonical path of the file that produced this diagnostic, when known.
    pub source_path: Option<PathBuf>,
    /// Diagnostic code, message, severity, and source range.
    pub diagnostic: Diagnostic,
}

impl Default for DefinitionLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_tokens: 250_000,
            max_files: 128,
            max_diagnostics: 10_000,
        }
    }
}

/// Parses a readable `.lm` definition with the built-in Rust-seeded grammar.
///
/// The generic parser produces the definition AST; the converter then builds
/// and validates the canonical language model.
///
/// # Errors
/// Returns source-located parser, conversion, or specification diagnostics.
pub fn parse_definition(
    source: &SourceFile,
) -> Result<langmeld_core::LanguageSpec, Vec<Diagnostic>> {
    parse_definition_with_limits(source, DefinitionLimits::default())
}

/// Parses a readable `.lm` definition with caller-selected source and token limits.
///
/// # Errors
/// Returns source-located parser, conversion, or specification diagnostics.
pub fn parse_definition_with_limits(
    source: &SourceFile,
    limits: DefinitionLimits,
) -> Result<langmeld_core::LanguageSpec, Vec<Diagnostic>> {
    if source.text.len() > limits.max_source_bytes {
        return Err(vec![Diagnostic::error(
            "LMDEF100",
            format!(
                "definition is {} bytes; parsing limit is {} bytes",
                source.text.len(),
                limits.max_source_bytes
            ),
            Span::new(0, source.text.len()),
        )]);
    }
    let prepared = prepared_definition_parser()?;
    let validated = definition_tree::parse_language_definition_with_prepared_and_limits(
        source, prepared, limits,
    )?;
    Ok(validated.spec().clone())
}

/// Parses a root `.lm` source and caller-supplied in-memory modules.
///
/// Modules are combined without filesystem discovery or import resolution.
/// Include every source explicitly; the aggregate source size is bounded by
/// [`DefinitionLimits`].
///
/// # Errors
/// Returns parser, conversion, import, or specification diagnostics.
pub fn parse_language_definition_modules(
    root: &SourceFile,
    modules: &[SourceFile],
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_modules_with_limits(root, modules, DefinitionLimits::default())
}

/// Parses in-memory definition modules with caller-selected resource limits.
///
/// # Errors
/// Returns parser, conversion, import, or specification diagnostics.
pub fn parse_language_definition_modules_with_limits(
    root: &SourceFile,
    modules: &[SourceFile],
    limits: DefinitionLimits,
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    let prepared = prepared_definition_parser()?;
    definition_tree::parse_language_definition_modules_with_prepared(
        root, modules, prepared, limits,
    )
}

/// Loads and converts a file-based `.lm` definition project with the built-in grammar.
///
/// Imports are resolved relative to the root file. The root directory is the
/// trust boundary for imported paths.
///
/// # Errors
/// Returns parser, conversion, import-resolution, or specification diagnostics.
pub fn parse_language_definition_project(
    root_path: &Path,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_project_with_limits(root_path, DefinitionLimits::default())
}

/// Loads a file-based `.lm` project with caller-selected resource limits.
///
/// # Errors
/// Returns parser, conversion, import-resolution, or specification diagnostics.
pub fn parse_language_definition_project_with_limits(
    root_path: &Path,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    crate::definition_import::parse_project(root_path, prepared_definition_parser()?, limits)
}

/// Loads a file-based `.lm` project while replacing selected on-disk snapshots
/// with caller-provided source text.
///
/// Overlay keys must identify existing `.lm` files inside the root definition
/// directory. Imported files not present in `overlays` are read from disk.
/// Overlay text and loaded disk sources share the project source-byte limit.
/// This supports editor validation without writing unsaved buffers to disk.
///
/// # Errors
/// Returns parser, conversion, import-resolution, specification, or overlay
/// boundary diagnostics.
pub fn parse_language_definition_project_with_overlays(
    root_path: &Path,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_project_with_overlays_and_limits(
        root_path,
        overlays,
        DefinitionLimits::default(),
    )
}

/// Loads a file-based `.lm` project with source overlays and caller-selected
/// resource limits.
///
/// # Errors
/// Returns parser, conversion, import-resolution, specification, or overlay
/// boundary diagnostics.
pub fn parse_language_definition_project_with_overlays_and_limits(
    root_path: &Path,
    overlays: &BTreeMap<PathBuf, String>,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    crate::definition_import::parse_project_with_overlays_and_diagnostics(
        root_path,
        prepared_definition_parser()?,
        limits,
        overlays,
    )
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.diagnostic)
            .collect()
    })
}

/// Loads a file-based `.lm` project with source overlays and returns each
/// diagnostic with the canonical file path that produced it when known.
///
/// Project-level conversion and specification findings may have no source
/// path or span when the definition model cannot identify one reliably.
///
/// # Errors
/// Returns parser, conversion, import-resolution, specification, or overlay
/// boundary diagnostics.
pub fn parse_language_definition_project_with_overlays_and_diagnostics(
    root_path: &Path,
    overlays: &BTreeMap<PathBuf, String>,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<DefinitionProjectDiagnostic>> {
    let prepared = prepared_definition_parser().map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| DefinitionProjectDiagnostic {
                source_path: None,
                diagnostic,
            })
            .collect::<Vec<_>>()
    })?;
    crate::definition_import::parse_project_with_overlays_and_diagnostics(
        root_path, prepared, limits, overlays,
    )
}

pub(crate) fn parse_language_definition_project_spec_with_sources(
    root_path: &Path,
    limits: DefinitionLimits,
) -> Result<(langmeld_core::LanguageSpec, Vec<std::path::PathBuf>, usize), Vec<Diagnostic>> {
    crate::definition_import::parse_project_spec_with_sources(
        root_path,
        prepared_definition_parser()?,
        limits,
    )
}

/// Returns canonical paths for a language definition and its imported modules.
///
/// JSON definitions return the canonical JSON file path; readable Langmeld
/// projects return roots and imports in parser resolution order.
///
/// # Errors
/// Returns file access, import, or definition parsing errors.
pub fn language_definition_source_paths(
    root_path: &Path,
    limits: DefinitionLimits,
) -> Result<Vec<std::path::PathBuf>, String> {
    if root_path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return std::fs::canonicalize(root_path)
            .map(|path| vec![path])
            .map_err(|error| {
                format!(
                    "cannot resolve language definition '{}': {error}",
                    root_path.display()
                )
            });
    }
    let prepared = prepared_definition_parser().map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    crate::definition_import::project_source_paths(root_path, prepared, limits).map_err(
        |diagnostics| {
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("\n")
        },
    )
}

/// Loads a file-based `.lm` project with a caller-supplied meta-language.
///
/// # Errors
/// Returns parser, conversion, import-resolution, or specification diagnostics.
pub fn parse_language_definition_project_with_meta(
    root_path: &Path,
    meta_language: &ValidatedLanguageSpec,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_project_with_meta_and_limits(
        root_path,
        meta_language,
        DefinitionLimits::default(),
    )
}

/// Loads a file-based `.lm` project with a supplied meta-language and limits.
///
/// # Errors
/// Returns parser, conversion, import-resolution, or specification diagnostics.
pub fn parse_language_definition_project_with_meta_and_limits(
    root_path: &Path,
    meta_language: &ValidatedLanguageSpec,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let prepared = crate::PreparedLanguage::new(meta_language.clone())
        .map_err(|error| vec![Diagnostic::spec_error("LM1080", error.to_string())])?;
    crate::definition_import::parse_project(root_path, &prepared, limits)
}

/// Parses a readable `.lm` source using a caller-supplied meta-language.
///
/// This is the bootstrap seam for alternate definition dialects. The built-in
/// format should use [`parse_definition`] so its prepared parser can be reused.
///
/// # Errors
/// Returns source-located parser, conversion, or specification diagnostics.
pub fn parse_language_definition_with_meta(
    source: &SourceFile,
    meta_language: &langmeld_core::ValidatedLanguageSpec,
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    definition_tree::parse_language_definition_with_meta(source, meta_language)
}

/// Parses in-memory definition modules with a caller-supplied meta-language.
///
/// Imports are not resolved; include every source explicitly.
///
/// # Errors
/// Returns parser, conversion, import, or specification diagnostics.
pub fn parse_language_definition_modules_with_meta(
    root: &SourceFile,
    modules: &[SourceFile],
    meta_language: &langmeld_core::ValidatedLanguageSpec,
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_modules_with_meta_and_limits(
        root,
        modules,
        meta_language,
        DefinitionLimits::default(),
    )
}

/// Parses in-memory modules using a caller-supplied meta-language and limits.
///
/// # Errors
/// Returns parser, conversion, import, or specification diagnostics.
pub fn parse_language_definition_modules_with_meta_and_limits(
    root: &SourceFile,
    modules: &[SourceFile],
    meta_language: &langmeld_core::ValidatedLanguageSpec,
    limits: DefinitionLimits,
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    definition_tree::parse_language_definition_modules_with_meta_and_limits(
        root,
        modules,
        meta_language,
        limits,
    )
}

/// Parses a `.lm` source using a caller-supplied meta-language and limits.
///
/// # Errors
/// Returns source-located parser, conversion, or specification diagnostics.
pub fn parse_language_definition_with_meta_and_limits(
    source: &SourceFile,
    meta_language: &langmeld_core::ValidatedLanguageSpec,
    limits: DefinitionLimits,
) -> Result<langmeld_core::ValidatedLanguageSpec, Vec<Diagnostic>> {
    definition_tree::parse_language_definition_with_meta_and_limits(source, meta_language, limits)
}

fn prepared_definition_parser() -> Result<&'static crate::PreparedLanguage, Vec<Diagnostic>> {
    langmeld_definition_parser().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| Diagnostic::spec_error(error.code, error.message))
            .collect::<Vec<_>>()
    })
}
