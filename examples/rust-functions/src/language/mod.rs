//! The compiled effective model and its Rust-authored base model.

mod spec;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_language.rs"));
}

pub(super) fn load_language()
-> Result<langmeld::core::ValidatedLanguageSpec, Vec<langmeld::core::Diagnostic>> {
    generated::validated_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| langmeld::core::Diagnostic::spec_error(error.code, error.message))
            .collect()
    })
}

pub(super) fn load_language_runtime()
-> Result<langmeld::core::ValidatedLanguageSpec, Vec<langmeld::core::Diagnostic>> {
    let base_language = spec::validated_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| langmeld::core::Diagnostic::spec_error(error.code, error.message))
            .collect::<Vec<_>>()
    })?;
    langmeld::language::load_project_language_with_base_language(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("langmeld.json")
            .as_path(),
        &base_language,
    )
    .map_err(|error| {
        vec![langmeld::core::Diagnostic::spec_error(
            "LMSTARTER001",
            error,
        )]
    })
}
