//! The Rust-authored canonical language model.

mod spec;

pub(super) fn load_language(
) -> Result<langmeld::core::ValidatedLanguageSpec, Vec<langmeld::core::Diagnostic>> {
    spec::validated_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| langmeld::core::Diagnostic::spec_error(error.code, error.message))
            .collect()
    })
}
