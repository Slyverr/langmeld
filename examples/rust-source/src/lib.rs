//! A source-language host that delegates Rust parsing to `syn`.

use langmeld::{
    core::{Diagnostic, SourceFile, ValidatedLanguageSpec},
    language::{ConfiguredLanguageParser, ParseResult, PreparedLanguage},
};

mod language {
    use super::{Diagnostic, ValidatedLanguageSpec};

    mod generated {
        include!(concat!(env!("OUT_DIR"), "/langmeld_language.rs"));
    }

    pub(super) fn load() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
        generated::validated_language().map_err(|errors| {
            errors
                .into_iter()
                .map(|error| Diagnostic::spec_error(error.code, error.message))
                .collect()
        })
    }
}

mod host_ast;
mod parser;

/// Typed projections generated from the Rust source language model.
pub mod ast {
    include!(concat!(env!("OUT_DIR"), "/langmeld_ast.rs"));
}

pub use host_ast::{RustFunctionParameter, RustFunctionSignature};

/// Returns the compiled Rust source language model.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load()
}

/// Prepares Langmeld with the host's Rust parser callback.
pub fn parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    let prepared = PreparedLanguage::new(load_language()?)
        .map_err(|error| vec![Diagnostic::spec_error("LMRS1002", error.to_string())])?;
    prepared
        .with_custom_parsers(parser::callbacks())
        .map_err(|error| vec![Diagnostic::spec_error("LMRS1003", error.to_string())])
        .map(|parser| parser.with_custom_lexer(parser::lexer()))
}

/// Parses a Rust source file and exposes generic plus host-native syntax data.
pub fn parse_source(source: &SourceFile) -> Result<ParseResult, Vec<Diagnostic>> {
    Ok(parser()?.parse(source))
}
