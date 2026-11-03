//! Rust host entry points for the @NAME@ language.

use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{ConfiguredLanguageParser, ParseResult, PreparedLanguage};
use std::path::PathBuf;

/// Loads the modular language definition from `language/`.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    langmeld::language::parse_language_definition_project(&language_definition_path())
}

/// Prepares the declarative parser for the language.
pub fn parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    let prepared = PreparedLanguage::new(load_language()?).map_err(|error| {
        vec![Diagnostic::spec_error("LMSTARTER001", error.to_string())]
    })?;
    prepared.with_custom_parsers([]).map_err(|error| {
        vec![Diagnostic::spec_error("LMSTARTER002", error.to_string())]
    })
}

/// Parses one source snapshot and returns its syntax tree and diagnostics.
pub fn parse_source(source: &SourceFile) -> Result<ParseResult, Vec<Diagnostic>> {
    Ok(parser()?.parse(source))
}

fn language_definition_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("language/language.lm")
}
