//! Resource-bounded parsing APIs for readable language definitions.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{definition_meta::langmeld_definition_parser, definition_tree};

/// Input bounds for readable language-definition loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefinitionLimits {
    /// Maximum UTF-8 definition size in bytes.
    pub max_source_bytes: usize,
    /// Maximum lexer tokens, including the end-of-file sentinel.
    pub max_tokens: usize,
    /// Maximum diagnostics returned for malformed definitions.
    pub max_diagnostics: usize,
}

impl Default for DefinitionLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_tokens: 250_000,
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
    let prepared = langmeld_definition_parser().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| Diagnostic::spec_error(error.code, error.message))
            .collect::<Vec<_>>()
    })?;
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
    let prepared = langmeld_definition_parser().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| Diagnostic::spec_error(error.code, error.message))
            .collect::<Vec<_>>()
    })?;
    definition_tree::parse_language_definition_modules_with_prepared(
        root, modules, prepared, limits,
    )
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
