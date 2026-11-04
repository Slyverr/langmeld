//! Rust builder definition for Langmeld's self-describing language format.

mod constructs;
mod expressions;
mod language;
mod syntax;
mod types;

use crate::LANGUAGE_DEFINITION_EXTENSION;
use langmeld_core::{LanguageBuilder, ValidatedLanguageSpec};

pub(super) fn build() -> Result<ValidatedLanguageSpec, Vec<langmeld_core::SpecError>> {
    LanguageBuilder::new("LangmeldDefinition")
        .schema_version(1)
        .version("0.1.0")
        .root("language")
        .source_extension(format!(".{LANGUAGE_DEFINITION_EXTENSION}"))
        .constructs(language::definitions())
        .constructs(types::definitions())
        .constructs(constructs::definitions())
        .constructs(syntax::definitions())
        .constructs(expressions::definitions())
        .build()
}
