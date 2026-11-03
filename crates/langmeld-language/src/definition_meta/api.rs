//! Rust builder definition for Langmeld's self-describing language format.

mod constructs;
mod language;
mod syntax;
mod types;

use langmeld_core::{LanguageBuilder, ValidatedLanguageSpec};

pub(super) fn build() -> Result<ValidatedLanguageSpec, Vec<langmeld_core::SpecError>> {
    LanguageBuilder::new("LangmeldDefinition")
        .schema_version(1)
        .version("0.1.0")
        .root("language")
        .source_extension(".lm")
        .constructs(language::definitions())
        .constructs(types::definitions())
        .constructs(constructs::definitions())
        .constructs(syntax::definitions())
        .build()
}
