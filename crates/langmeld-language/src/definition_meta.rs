//! Rust-authored bootstrap grammar for the readable `.langmeld` definition format.

mod embedded_sources {
    include!(concat!(env!("OUT_DIR"), "/definition_meta_sources.rs"));
}
mod api;

use langmeld_core::{SourceFile, SpecError, ValidatedLanguageSpec};
use std::sync::OnceLock;

/// Rebuilds the checked-in `.langmeld` grammar and verifies it against its Rust seed.
///
/// Rust builders provide the bootstrap grammar; the generic parser reads the
/// embedded modular source and the converter rebuilds the canonical model.
/// Initialization fails if parsing fails or the rebuilt model drifts from the
/// seed.
///
/// # Errors
/// Returns the language-specification validation errors if the seed is invalid.
pub fn langmeld_definition_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    static LANGUAGE: OnceLock<Result<ValidatedLanguageSpec, Vec<SpecError>>> = OnceLock::new();
    LANGUAGE.get_or_init(build_language).clone()
}

fn build_language() -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
    let seed = api::build()?;
    let prepared = crate::PreparedLanguage::new(seed.clone()).map_err(|error| {
        vec![SpecError {
            code: "LM1080",
            message: error.to_string(),
        }]
    })?;
    let root = SourceFile::new("language/language.langmeld", embedded_sources::ROOT);
    let modules = embedded_sources::MODULES
        .iter()
        .map(|(path, text)| SourceFile::new(*path, *text))
        .collect::<Vec<_>>();
    let rebuilt = super::definition_tree::parse_language_definition_modules_with_prepared(
        &root,
        &modules,
        &prepared,
        crate::DefinitionLimits::default(),
    )
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| SpecError {
                code: "LM1080",
                message: format!("{}: {}", diagnostic.code, diagnostic.message),
            })
            .collect::<Vec<_>>()
    })?;
    if rebuilt.spec() != seed.spec() {
        return Err(vec![SpecError {
            code: "LM1081",
            message:
                "the embedded `.langmeld` grammar differs from its Rust API bootstrap definition"
                    .to_owned(),
        }]);
    }
    Ok(rebuilt)
}

/// Returns the process-wide prepared parser for `.langmeld` definition source.
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
            "language.langmeld",
            "language Sample {\n  version: \"0.1.0\"\n  root: program\n}\n",
        ));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.nodes[0].construct, "language");
        assert_eq!(parsed.program.nodes[0].children.len(), 2);
    }

    #[test]
    fn embedded_definition_rebuilds_the_rust_seed_exactly() {
        let seed = super::api::build().expect("the Rust API seed validates");
        let rebuilt = langmeld_definition_language()
            .expect("the embedded modules parse and match the Rust API seed");
        assert_eq!(rebuilt.spec(), seed.spec());
    }
}
