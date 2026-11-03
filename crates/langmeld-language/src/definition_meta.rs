//! Rust-authored bootstrap grammar for the readable `.lm` definition format.

mod api;

use langmeld_core::{SpecError, ValidatedLanguageSpec};
use std::sync::OnceLock;

/// Builds and validates the trusted grammar used to parse `.lm` definitions.
///
/// The Rust builders are the bootstrap seed. Later stages parse the embedded
/// modular `.lm` description through this grammar and compare its converted
/// model with the seed.
///
/// # Errors
/// Returns the language-specification validation errors if the seed is invalid.
pub fn langmeld_definition_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    static LANGUAGE: OnceLock<Result<ValidatedLanguageSpec, Vec<SpecError>>> = OnceLock::new();
    LANGUAGE.get_or_init(api::build).clone()
}

/// Returns the process-wide prepared parser for `.lm` definition source.
///
/// Reusing this parser avoids compiling the bootstrap patterns again. Higher-level
/// definition loading APIs will also use this instance.
pub fn langmeld_definition_parser() -> Result<&'static crate::PreparedLanguage, Vec<SpecError>> {
    static PREPARED: OnceLock<Result<crate::PreparedLanguage, Vec<SpecError>>> = OnceLock::new();
    PREPARED
        .get_or_init(|| {
            langmeld_definition_language().and_then(|language| {
                crate::PreparedLanguage::new(language).map_err(|error| {
                    vec![SpecError {
                        code: "LM1080",
                        message: error.to_string(),
                    }]
                })
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::{langmeld_definition_language, langmeld_definition_parser};
    use langmeld_core::SourceFile;

    #[test]
    fn rust_seed_builds_and_reuses_its_prepared_parser() {
        let language = langmeld_definition_language().expect("the Rust seed validates");
        assert_eq!(language.spec().name, "LangmeldDefinition");
        assert_eq!(language.spec().root, "language");
        assert!(language.construct("construct").is_some());
        assert!(language.construct("typed_field").is_some());

        let first = langmeld_definition_parser().expect("seed patterns compile");
        let second = langmeld_definition_parser().expect("prepared seed is cached");
        assert!(std::ptr::eq(first, second));

        let parser = first
            .clone()
            .configure()
            .expect("the meta-language has no host parser callbacks");
        let parsed = parser.parse(&SourceFile::new(
            "language.lm",
            "language Sample {\n  version: \"0.1.0\"\n  root: program\n}\n",
        ));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.nodes[0].construct, "language");
        assert_eq!(parsed.program.nodes[0].children.len(), 2);
    }
}
