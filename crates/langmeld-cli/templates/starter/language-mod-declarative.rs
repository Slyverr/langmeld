//! The compiled language model and explicit dynamic project loader.

mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_language.rs"));
}

pub(super) fn load_language(
) -> Result<langmeld::core::ValidatedLanguageSpec, Vec<langmeld::core::Diagnostic>> {
    generated::validated_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| langmeld::core::Diagnostic::spec_error(error.code, error.message))
            .collect()
    })
}

pub(super) fn load_language_runtime(
) -> Result<langmeld::core::ValidatedLanguageSpec, Vec<langmeld::core::Diagnostic>> {
    langmeld::language::load_project_language(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("langmeld.json").as_path(),
    )
    .map_err(|error| vec![langmeld::core::Diagnostic::spec_error("LMSTARTER001", error)])
}
